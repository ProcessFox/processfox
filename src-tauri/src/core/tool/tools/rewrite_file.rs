use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::{ensure_inside_sandbox, resolve_for_preview};
use crate::core::tool::{HitlPreview, Tool, ToolContext, ToolOutput, ToolSchema};

#[derive(Debug, Default)]
pub struct RewriteFileTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    content: String,
}

const ALLOWED_EXTENSIONS: &[&str] = &["md", "markdown", "txt", "text"];

fn is_text_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    ALLOWED_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

#[async_trait]
impl Tool for RewriteFileTool {
    fn name(&self) -> &'static str {
        "rewrite_file"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Replace the entire contents of a Markdown or plain-text file in the agent's \
                 folder with new content. Works for .md, .markdown, .txt, and .text files only \
                 — DOCX has its own tools. Always read the file first so you can pass back the \
                 full new content (the user sees a diff before approving). To extend without \
                 risking deletions, use `append_to_md` instead."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder. Must end in .md, .markdown, .txt, or .text."
                    },
                    "content": {
                        "type": "string",
                        "description": "The complete new contents of the file. Whatever was there before is replaced."
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
        let before = resolved
            .map(|p| std::fs::read_to_string(&p).unwrap_or_default())
            .unwrap_or_default();
        Some(HitlPreview::RewriteFile {
            path: parsed.path,
            before,
            after: parsed.content,
            creates_file: !exists,
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> CoreResult<ToolOutput> {
        let parsed: Input = serde_json::from_value(input).map_err(CoreError::from)?;
        if !is_text_extension(&parsed.path) {
            return Err(CoreError::PathInvalid(format!(
                "{} muss auf .md/.markdown/.txt/.text enden — für .docx siehe write_docx",
                parsed.path
            )));
        }
        let rel = std::path::PathBuf::from(&parsed.path);
        let target = ensure_inside_sandbox(&ctx.agent_folder, &rel)?;
        let was_file = target.is_file();

        std::fs::write(&target, parsed.content.as_bytes())?;

        let bytes = parsed.content.len();
        Ok(ToolOutput::text(format!(
            "Rewrote {} ({bytes} bytes, {}).",
            parsed.path,
            if was_file { "replaced" } else { "created" }
        )))
    }
}
