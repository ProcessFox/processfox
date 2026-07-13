use async_trait::async_trait;
use docx_rs::{Docx, Paragraph, Run, Style, StyleType};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::{ensure_inside_sandbox, resolve_for_preview};
use crate::core::tool::{HitlPreview, Tool, ToolContext, ToolOutput, ToolSchema};

#[derive(Debug, Default)]
pub struct WriteDocxTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    content: String,
}

/// Lightweight block model so the tool can take a single string and turn it
/// into a docx with paragraphs, headings, and bulleted lines. We don't pull
/// in a full Markdown parser — the supported syntax is intentionally small.
pub(super) enum Block {
    Heading(u8, String),
    Paragraph(String),
    Bullet(String),
}

pub(super) fn parse_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut buf = String::new();

    let flush_paragraph = |buf: &mut String, blocks: &mut Vec<Block>| {
        let trimmed = buf.trim();
        if !trimmed.is_empty() {
            blocks.push(Block::Paragraph(trimmed.to_string()));
        }
        buf.clear();
    };

    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            flush_paragraph(&mut buf, &mut blocks);
            continue;
        }
        if let Some(rest) = line.strip_prefix("### ") {
            flush_paragraph(&mut buf, &mut blocks);
            blocks.push(Block::Heading(3, rest.trim().to_string()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            flush_paragraph(&mut buf, &mut blocks);
            blocks.push(Block::Heading(2, rest.trim().to_string()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            flush_paragraph(&mut buf, &mut blocks);
            blocks.push(Block::Heading(1, rest.trim().to_string()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            flush_paragraph(&mut buf, &mut blocks);
            blocks.push(Block::Bullet(rest.trim().to_string()));
            continue;
        }
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(line);
    }
    flush_paragraph(&mut buf, &mut blocks);
    blocks
}

/// Build a fresh Docx that already has Heading1/2/3 styles registered in
/// styles.xml. Word will only show the paragraph's `pStyle` reference
/// ("Überschrift 1") if a matching style definition exists in the file —
/// without these `add_style` calls the heading lives on as a plain paragraph
/// in Word's UI even though we tagged it with the style id.
pub(super) fn new_docx_with_heading_styles() -> Docx {
    Docx::new()
        .add_style(
            Style::new("Heading1", StyleType::Paragraph)
                .name("heading 1")
                .bold()
                .size(40),
        )
        .add_style(
            Style::new("Heading2", StyleType::Paragraph)
                .name("heading 2")
                .bold()
                .size(32),
        )
        .add_style(
            Style::new("Heading3", StyleType::Paragraph)
                .name("heading 3")
                .bold()
                .size(28),
        )
}

pub(super) fn append_blocks_to_docx(mut docx: Docx, blocks: &[Block]) -> Docx {
    for b in blocks {
        docx = docx.add_paragraph(block_to_paragraph(b));
    }
    docx
}

fn block_to_paragraph(b: &Block) -> Paragraph {
    match b {
        Block::Heading(level, text) => {
            // We tag the paragraph with the matching Word style id so editors
            // that ship those styles (Word itself, mostly) pick them up — but
            // we ALSO format the run inline (bold + larger size) so the
            // heading looks right in viewers that ignore the style reference
            // because the matching definition isn't in our styles.xml.
            let style = match level {
                1 => "Heading1",
                2 => "Heading2",
                _ => "Heading3",
            };
            // Sizes are half-points: 40 = 20pt, 32 = 16pt, 28 = 14pt.
            let size = match level {
                1 => 40,
                2 => 32,
                _ => 28,
            };
            Paragraph::new()
                .style(style)
                .add_run(Run::new().bold().size(size).add_text(text))
        }
        Block::Paragraph(text) => {
            let mut para = Paragraph::new();
            for (i, line) in text.split('\n').enumerate() {
                if i > 0 {
                    para = para.add_run(Run::new().add_break(docx_rs::BreakType::TextWrapping));
                }
                para = para.add_run(Run::new().add_text(line));
            }
            para
        }
        // Real Word bullets need a numbering definition; for v1 we render
        // a plain "• " prefix which still looks like a bullet to readers.
        Block::Bullet(text) => Paragraph::new().add_run(Run::new().add_text(format!("• {text}"))),
    }
}

fn build_docx(blocks: &[Block]) -> Docx {
    append_blocks_to_docx(new_docx_with_heading_styles(), blocks)
}

pub(super) fn render_preview_text(blocks: &[Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        let line = match b {
            Block::Heading(1, t) => format!("# {t}"),
            Block::Heading(2, t) => format!("## {t}"),
            Block::Heading(_, t) => format!("### {t}"),
            Block::Paragraph(t) => t.clone(),
            Block::Bullet(t) => format!("- {t}"),
        };
        out.push_str(&line);
        out.push_str("\n\n");
        if out.len() > 600 {
            out.push('…');
            break;
        }
    }
    out.trim_end().to_string()
}

#[async_trait]
impl Tool for WriteDocxTool {
    fn name(&self) -> &'static str {
        "write_docx"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: "Create a new Word (.docx) file in the agent's folder, or overwrite an \
                 existing one. The content is a small Markdown-flavoured string: lines \
                 starting with `#`, `##`, or `###` become headings, lines starting with \
                 `- ` become bullets, blank lines separate paragraphs. The user is shown \
                 a preview and must approve before anything is written. WARNING: if the \
                 file already exists, it will be replaced — use `append_to_docx` instead \
                 to extend a document without losing existing content."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'report.docx' or 'reports/2026-04.docx'. Must end in .docx."
                    },
                    "content": {
                        "type": "string",
                        "description": "Document content. Use # / ## / ### for headings, '- ' prefix for bullets, blank lines between paragraphs."
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
        let creates_file =
            resolve_for_preview(&ctx.agent_folder, std::path::Path::new(&parsed.path))
                .map(|p| !p.is_file())
                .unwrap_or(true);
        Some(HitlPreview::WriteDocx {
            path: parsed.path,
            block_count: blocks.len(),
            preview_text,
            creates_file,
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
        let docx = build_docx(&blocks);

        let file = std::fs::File::create(&target)?;
        docx.build()
            .pack(file)
            .map_err(|e| CoreError::Llm(format!("docx pack failed: {e}")))?;

        Ok(ToolOutput::text(format!(
            "Wrote {} blocks to {} ({}).",
            blocks.len(),
            parsed.path,
            if target.exists() {
                "saved"
            } else {
                "saved (file system did not confirm)"
            }
        )))
    }
}
