use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::io::Write;

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::{ensure_inside_sandbox, resolve_for_preview};
use crate::core::tool::{HitlPreview, Tool, ToolContext, ToolOutput, ToolSchema};

#[derive(Debug, Default)]
pub struct AppendToMdTool;

/// Read the last ~600 chars of a file as UTF-8 (lossy on bad bytes) so the
/// HITL preview can show "this is what's currently at the bottom of the file."
fn read_tail(path: &std::path::Path) -> Option<String> {
    const MAX_BYTES: usize = 600;
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let start = bytes.len().saturating_sub(MAX_BYTES);
    let slice = &bytes[start..];
    let mut text = String::from_utf8_lossy(slice).into_owned();
    if start > 0 {
        text.insert_str(0, "…\n");
    }
    Some(text)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    content: String,
    /// Optional separator inserted between existing content and the new
    /// block. Default `"\n\n"` so appends form readable paragraphs.
    #[serde(default)]
    separator: Option<String>,
}

#[async_trait]
impl Tool for AppendToMdTool {
    fn name(&self) -> &'static str {
        "append_to_md"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Append a block of text to a Markdown or text file in the agent's folder. \
                 Creates the file if it doesn't exist. The content is added after a blank \
                 line by default. Use this for journaling, logging, or extending a running \
                 document — every append asks the user for confirmation before it touches disk."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'journal.md' or 'notes/2026-04.md'."
                    },
                    "content": {
                        "type": "string",
                        "description": "Text to append. Markdown is fine; line breaks are preserved."
                    },
                    "separator": {
                        "type": "string",
                        "description": "Optional separator between existing content and the new block. Defaults to a blank line."
                    }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }
    }

    fn requires_approval(&self, input: &JsonValue, ctx: &ToolContext) -> Option<HitlPreview> {
        let parsed: Input = serde_json::from_value(input.clone()).ok()?;
        let resolved = resolve_for_preview(&ctx.agent_folder, std::path::Path::new(&parsed.path));
        let exists = resolved.is_some();
        let existing_tail = resolved.as_deref().and_then(read_tail);
        Some(HitlPreview::AppendToFile {
            path: parsed.path,
            content: parsed.content,
            creates_file: !exists,
            existing_tail,
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> CoreResult<ToolOutput> {
        let parsed: Input = serde_json::from_value(input).map_err(CoreError::from)?;

        let rel = std::path::PathBuf::from(&parsed.path);
        let target = ensure_inside_sandbox(&ctx.agent_folder, &rel)?;

        let separator = parsed.separator.as_deref().unwrap_or("\n\n");
        let already_exists = target.exists();

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target)?;

        if already_exists {
            // Make sure there's a separator between old content and new.
            let metadata = file.metadata()?;
            if metadata.len() > 0 {
                file.write_all(separator.as_bytes())?;
            }
        }
        file.write_all(parsed.content.as_bytes())?;
        file.flush()?;

        let bytes = parsed.content.len();
        Ok(ToolOutput::text(format!(
            "Appended {bytes} bytes to {} ({})",
            parsed.path,
            if already_exists {
                "existing file"
            } else {
                "new file"
            }
        )))
    }
}
