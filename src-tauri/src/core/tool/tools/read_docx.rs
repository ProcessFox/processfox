use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::io::Read;
use std::path::PathBuf;

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::ensure_in_agent_folder;
use crate::core::tool::{Tool, ToolContext, ToolOutput, ToolSchema};

const MAX_OUTPUT_BYTES: usize = 200 * 1024;

#[derive(Debug, Default)]
pub struct ReadDocxTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
}

#[async_trait]
impl Tool for ReadDocxTool {
    fn name(&self) -> &'static str {
        "read_docx"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Extract plain text from a Microsoft Word (.docx) file inside the agent's folder. \
                 Returns paragraphs separated by blank lines. Tables and images are stripped; \
                 only running text is preserved."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'offers/draft.docx'."
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> CoreResult<ToolOutput> {
        let parsed: Input = serde_json::from_value(input).map_err(CoreError::from)?;
        let target = ensure_in_agent_folder(&ctx.agent_folder, &PathBuf::from(&parsed.path))?;

        if !target.is_file() {
            return Err(CoreError::PathInvalid(format!(
                "'{}' is not a file",
                target.display()
            )));
        }

        let target_owned = target.clone();
        let extracted = tokio::task::spawn_blocking(move || extract_docx_text(&target_owned))
            .await
            .map_err(|e| CoreError::Llm(format!("DOCX-Extraktion abgebrochen: {e}")))??;

        let total_bytes = extracted.len();
        let mtime = crate::core::tool::mtime_suffix(&target);
        let body = if total_bytes > MAX_OUTPUT_BYTES {
            let truncated: String = extracted.chars().take(MAX_OUTPUT_BYTES / 4).collect();
            format!(
                "--- {} ({} bytes extracted, truncated{}) ---\n{}\n\n[truncated — extracted text exceeds {} KB]",
                parsed.path,
                total_bytes,
                mtime,
                truncated,
                MAX_OUTPUT_BYTES / 1024
            )
        } else if extracted.trim().is_empty() {
            format!(
                "--- {} ({} bytes{}) ---\n[empty extraction — document had no text content]",
                parsed.path, total_bytes, mtime
            )
        } else {
            format!(
                "--- {} ({} bytes{}) ---\n{}",
                parsed.path, total_bytes, mtime, extracted
            )
        };

        Ok(ToolOutput::text(body))
    }
}

/// Open a .docx (zip), find `word/document.xml`, and concatenate `<w:t>`
/// run-text contents. Paragraph boundaries (`<w:p>`) emit a newline; line
/// breaks (`<w:br>`) emit a soft newline.
pub(super) fn extract_docx_text(path: &std::path::Path) -> CoreResult<String> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| CoreError::Llm(format!("DOCX kein gültiges ZIP: {e}")))?;
    let mut entry = zip
        .by_name("word/document.xml")
        .map_err(|e| CoreError::Llm(format!("word/document.xml nicht gefunden: {e}")))?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;

    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if local_name(e.name().as_ref()) == b"t" => {
                in_text = true;
            }
            Ok(Event::Text(e)) if in_text => {
                out.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::End(ref e)) if local_name(e.name().as_ref()) == b"t" => {
                in_text = false;
            }
            Ok(Event::Empty(ref e)) if local_name(e.name().as_ref()) == b"br" => {
                out.push('\n');
            }
            Ok(Event::End(ref e)) if local_name(e.name().as_ref()) == b"p" => {
                out.push_str("\n\n");
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(CoreError::Llm(format!("DOCX-XML-Parse-Fehler: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }
    // Collapse runs of three or more newlines.
    let collapsed = out
        .lines()
        .collect::<Vec<_>>()
        .join("\n")
        .replace("\n\n\n\n", "\n\n");
    Ok(collapsed)
}

/// Strip XML namespace prefix (`w:t` → `t`).
fn local_name(qname: &[u8]) -> &[u8] {
    qname
        .iter()
        .position(|&b| b == b':')
        .map(|i| &qname[i + 1..])
        .unwrap_or(qname)
}
