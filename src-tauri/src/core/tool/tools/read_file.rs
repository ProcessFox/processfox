use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::path::PathBuf;

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::ensure_in_agent_folder;
use crate::core::tool::{Tool, ToolContext, ToolOutput, ToolSchema};

/// Maximum bytes to read from a single file. Keeps the LLM context bounded
/// and stops a stray `read_file` on a giant log from eating the window.
const MAX_BYTES: u64 = 512 * 1024;

#[derive(Debug, Default)]
pub struct ReadFileTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Read a UTF-8 text file (.md, .txt, .csv, .json, source code) from the agent's \
                 folder and return its contents. Refuses non-text files and caps reads at 512 KB \
                 — use dedicated tools for PDF/DOCX/XLSX instead (coming later)."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'notes.md' or 'reports/2026.csv'."
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

        let metadata = std::fs::metadata(&target)?;
        let size = metadata.len();
        let truncate = size > MAX_BYTES;
        let bytes_to_read = if truncate { MAX_BYTES } else { size };

        use std::io::Read;
        let mut file = std::fs::File::open(&target)?;
        let mut buf = vec![0u8; bytes_to_read as usize];
        file.read_exact(&mut buf)?;

        let text = match std::str::from_utf8(&buf) {
            Ok(s) => s.to_string(),
            Err(_) => {
                return Err(CoreError::Llm(format!(
                    "'{}' is not valid UTF-8 text. Use a binary-aware tool instead.",
                    parsed.path
                )));
            }
        };

        let mut body = format!(
            "--- {} ({} bytes{}) ---\n{text}",
            parsed.path,
            size,
            crate::core::tool::mtime_suffix(&target)
        );
        if truncate {
            body.push_str(&format!(
                "\n\n[truncated — showing first {MAX_BYTES} bytes of {size}]"
            ));
        }

        Ok(ToolOutput::text(body))
    }
}
