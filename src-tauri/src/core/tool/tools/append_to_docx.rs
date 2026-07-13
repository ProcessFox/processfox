use async_trait::async_trait;
use docx_rs::read_docx;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::{ensure_inside_sandbox, resolve_for_preview};
use crate::core::tool::{HitlPreview, Tool, ToolContext, ToolOutput, ToolSchema};

use super::read_docx::extract_docx_text;
use super::write_docx::{
    append_blocks_to_docx, new_docx_with_heading_styles, parse_blocks, render_preview_text,
};

#[derive(Debug, Default)]
pub struct AppendToDocxTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    content: String,
}

/// Pull the trailing ~600 characters of plaintext from an existing docx so
/// the HITL preview can show "this is what's already at the end of the file."
fn read_docx_tail(path: &std::path::Path) -> Option<String> {
    const MAX_BYTES: usize = 600;
    let text = extract_docx_text(path).ok()?;
    let trimmed = text.trim_end_matches('\n');
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed.len().saturating_sub(MAX_BYTES);
    let mut snippet = trimmed[start..].to_string();
    if start > 0 {
        snippet.insert_str(0, "…\n");
    }
    Some(snippet)
}

#[async_trait]
impl Tool for AppendToDocxTool {
    fn name(&self) -> &'static str {
        "append_to_docx"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Append new content to the end of an existing Word (.docx) file in the agent's \
                 folder, preserving the existing document. If the file does not exist yet, it \
                 is created. The content uses the same Markdown-light syntax as `write_docx`: \
                 # / ## / ### for headings, '- ' for bullets, blank lines between paragraphs. \
                 The user is shown a preview (including the tail of the existing file) and \
                 must approve before anything is written."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'minutes.docx'. Must end in .docx."
                    },
                    "content": {
                        "type": "string",
                        "description": "Blocks to append. Use # / ## / ### for headings, '- ' prefix for bullets, blank lines between paragraphs."
                    }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }
    }

    fn requires_approval(&self, input: &JsonValue, ctx: &ToolContext) -> Option<HitlPreview> {
        let parsed: Input = serde_json::from_value(input.clone()).ok()?;
        let blocks = parse_blocks(&parsed.content);
        let preview_text = render_preview_text(&blocks);
        let resolved = resolve_for_preview(&ctx.agent_folder, std::path::Path::new(&parsed.path));
        let exists = resolved.is_some();
        let existing_tail = resolved.as_deref().and_then(read_docx_tail);
        Some(HitlPreview::AppendToDocx {
            path: parsed.path,
            block_count: blocks.len(),
            preview_text,
            creates_file: !exists,
            existing_tail,
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> CoreResult<ToolOutput> {
        let parsed: Input = serde_json::from_value(input).map_err(CoreError::from)?;
        if !parsed.path.to_lowercase().ends_with(".docx") {
            return Err(CoreError::PathInvalid(format!(
                "{} muss auf .docx enden",
                parsed.path
            )));
        }
        let rel = std::path::PathBuf::from(&parsed.path);
        let target = ensure_inside_sandbox(&ctx.agent_folder, &rel)?;
        let blocks = parse_blocks(&parsed.content);

        let docx = if target.is_file() {
            let bytes = std::fs::read(&target)?;
            read_docx(&bytes).map_err(|e| CoreError::Llm(format!("docx parse failed: {e}")))?
        } else {
            new_docx_with_heading_styles()
        };
        let docx = append_blocks_to_docx(docx, &blocks);

        let file = std::fs::File::create(&target)?;
        docx.build()
            .pack(file)
            .map_err(|e| CoreError::Llm(format!("docx pack failed: {e}")))?;

        Ok(ToolOutput::text(format!(
            "Appended {} blocks to {}.",
            blocks.len(),
            parsed.path
        )))
    }
}
