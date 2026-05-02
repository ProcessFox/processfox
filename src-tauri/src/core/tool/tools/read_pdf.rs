use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::path::PathBuf;

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::ensure_in_agent_folder;
use crate::core::tool::{Tool, ToolContext, ToolOutput, ToolSchema};

/// Cap on the extracted text size we hand back to the LLM. Bigger PDFs get
/// truncated with a clear marker so the model knows it didn't see the rest.
const MAX_OUTPUT_BYTES: usize = 200 * 1024;

#[derive(Debug, Default)]
pub struct ReadPdfTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
}

#[async_trait]
impl Tool for ReadPdfTool {
    fn name(&self) -> &'static str {
        "read_pdf"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Extract text from a PDF file inside the agent's folder. Returns plain text \
                 with original line breaks where the layout allows. Handles digital PDFs; for \
                 scanned PDFs without OCR the result will be empty or garbled."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'reports/2026.pdf'."
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

        // pdf-extract is CPU-bound; run on the blocking pool so we don't tie
        // up the async runtime on a long PDF.
        let target_owned = target.clone();
        let extracted =
            tokio::task::spawn_blocking(move || pdf_extract::extract_text(&target_owned))
                .await
                .map_err(|e| CoreError::Llm(format!("PDF-Extraktion abgebrochen: {e}")))?
                .map_err(|e| CoreError::Llm(format!("PDF konnte nicht gelesen werden: {e}")))?;

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
                "--- {} ({} bytes{}) ---\n[empty extraction — likely a scanned PDF without OCR]",
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
