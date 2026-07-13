use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;
use std::io::{Read, Write};

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::{ensure_in_agent_folder, ensure_inside_sandbox, resolve_for_preview};
use crate::core::tool::{
    HitlPreview, TemplateReplacement, Tool, ToolContext, ToolOutput, ToolSchema,
};

#[derive(Debug, Default)]
pub struct WriteDocxFromTemplateTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    template_path: String,
    output_path: String,
    /// Map of placeholder key → replacement string. Keys are looked up in
    /// the template as `{{key}}`. Numeric/date values must be pre-formatted.
    replacements: BTreeMap<String, String>,
}

const PLACEHOLDER_OPEN: &str = "{{";
const PLACEHOLDER_CLOSE: &str = "}}";

/// XML-encode a replacement value so e.g. customer names containing `<` or
/// `&` don't break the docx. Only the four entities Word actually checks.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Apply all `{{key}} -> value` replacements to one xml string.
fn apply_replacements(xml: &str, replacements: &BTreeMap<String, String>) -> String {
    let mut out = xml.to_string();
    for (key, value) in replacements {
        let token = format!("{PLACEHOLDER_OPEN}{key}{PLACEHOLDER_CLOSE}");
        out = out.replace(&token, &xml_escape(value));
    }
    out
}

/// Find any unresolved `{{…}}` tokens left in the document after substitution
/// — usually a sign that the placeholder is split across multiple Word runs
/// or that the LLM passed the wrong key. Returns up to 5 distinct keys.
fn find_unresolved(xml: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find(PLACEHOLDER_OPEN) {
        let after_open = &rest[open + PLACEHOLDER_OPEN.len()..];
        if let Some(close) = after_open.find(PLACEHOLDER_CLOSE) {
            let key = after_open[..close].trim().to_string();
            if !key.is_empty() && !found.contains(&key) {
                found.push(key);
                if found.len() >= 5 {
                    break;
                }
            }
            rest = &after_open[close + PLACEHOLDER_CLOSE.len()..];
        } else {
            break;
        }
    }
    found
}

/// Read the template's document.xml so the tool can preview the actual set
/// of placeholders used by the file (HITL surface) and detect unknown keys
/// the LLM passed. Returns the placeholders found, deduped, in order.
fn collect_template_placeholders(template_path: &std::path::Path) -> Vec<String> {
    let Ok(file) = std::fs::File::open(template_path) else {
        return Vec::new();
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return Vec::new();
    };
    let Ok(mut entry) = zip.by_name("word/document.xml") else {
        return Vec::new();
    };
    let mut xml = String::new();
    if entry.read_to_string(&mut xml).is_err() {
        return Vec::new();
    }
    find_unresolved(&xml)
}

#[async_trait]
impl Tool for WriteDocxFromTemplateTool {
    fn name(&self) -> &'static str {
        "write_docx_from_template"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: "Generate a new Word (.docx) file by filling placeholders in an existing \
                 template .docx in the agent's folder. Placeholders use double-brace syntax \
                 like `{{customer_name}}`. Pass `replacements` as a flat key→value map; the \
                 tool replaces every occurrence and writes the result to `output_path`. The \
                 original template is never modified. The user is shown a preview with the \
                 substitution table and must approve before anything is written. \n\
                 LIMITATION: placeholders must be entered into the template without changing \
                 formatting mid-placeholder — Word otherwise splits a placeholder across \
                 internal runs and the substitution misses it. The tool reports unresolved \
                 placeholders with their keys so you can ask the user to fix the template."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "templatePath": {
                        "type": "string",
                        "description": "Path to the template .docx in the agent's folder."
                    },
                    "outputPath": {
                        "type": "string",
                        "description": "Path the filled .docx should be written to. Must end in .docx."
                    },
                    "replacements": {
                        "type": "object",
                        "description": "Flat key/value map: `{{key}}` in the template becomes the value.",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["templatePath", "outputPath", "replacements"],
                "additionalProperties": false
            }),
        }
    }

    fn requires_approval(&self, input: &JsonValue, ctx: &ToolContext) -> Option<HitlPreview> {
        let parsed: Input = serde_json::from_value(input.clone()).ok()?;
        let template_resolved = resolve_for_preview(
            &ctx.agent_folder,
            std::path::Path::new(&parsed.template_path),
        );
        let output_resolved =
            resolve_for_preview(&ctx.agent_folder, std::path::Path::new(&parsed.output_path));
        let creates_file = output_resolved.map(|p| !p.is_file()).unwrap_or(true);
        let template_placeholders = template_resolved
            .as_deref()
            .map(collect_template_placeholders)
            .unwrap_or_default();
        let replacements = parsed
            .replacements
            .iter()
            .map(|(k, v)| TemplateReplacement {
                key: k.clone(),
                value: v.clone(),
            })
            .collect();
        Some(HitlPreview::WriteDocxFromTemplate {
            template_path: parsed.template_path,
            output_path: parsed.output_path,
            replacements,
            template_placeholders,
            creates_file,
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> CoreResult<ToolOutput> {
        let parsed: Input = serde_json::from_value(input).map_err(CoreError::from)?;
        if !parsed.output_path.to_lowercase().ends_with(".docx") {
            return Err(CoreError::PathInvalid(format!(
                "{} muss auf .docx enden",
                parsed.output_path
            )));
        }
        let template = ensure_in_agent_folder(
            &ctx.agent_folder,
            &std::path::PathBuf::from(&parsed.template_path),
        )?;
        if !template.is_file() {
            return Err(CoreError::PathInvalid(format!(
                "Template {} existiert nicht",
                parsed.template_path
            )));
        }
        let output = ensure_inside_sandbox(
            &ctx.agent_folder,
            &std::path::PathBuf::from(&parsed.output_path),
        )?;

        let replacements = parsed.replacements;

        // Iterate every entry in the template ZIP. Run replacements on any
        // .xml file under `word/` (catches document.xml + header*.xml +
        // footer*.xml + endnotes etc.). Other entries (images, _rels) are
        // copied verbatim.
        let mut input_zip =
            zip::ZipArchive::new(std::fs::File::open(&template).map_err(CoreError::from)?)
                .map_err(|e| CoreError::Llm(format!("Template kein gültiges ZIP: {e}")))?;
        let output_file = std::fs::File::create(&output).map_err(CoreError::from)?;
        let mut zip_writer = zip::ZipWriter::new(output_file);
        let zip_options: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut unresolved: Vec<String> = Vec::new();

        for i in 0..input_zip.len() {
            let mut entry = input_zip
                .by_index(i)
                .map_err(|e| CoreError::Llm(format!("zip entry: {e}")))?;
            let name = entry.name().to_string();
            let is_xml_in_word = name.starts_with("word/") && name.to_lowercase().ends_with(".xml");

            if is_xml_in_word {
                let mut xml = String::new();
                entry
                    .read_to_string(&mut xml)
                    .map_err(|e| CoreError::Llm(format!("xml read {name}: {e}")))?;
                let replaced = apply_replacements(&xml, &replacements);
                let leftover = find_unresolved(&replaced);
                for k in leftover {
                    if !unresolved.contains(&k) {
                        unresolved.push(k);
                    }
                }
                zip_writer
                    .start_file(&name, zip_options)
                    .map_err(|e| CoreError::Llm(format!("zip start {name}: {e}")))?;
                zip_writer
                    .write_all(replaced.as_bytes())
                    .map_err(CoreError::from)?;
            } else {
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf).map_err(CoreError::from)?;
                zip_writer
                    .start_file(&name, zip_options)
                    .map_err(|e| CoreError::Llm(format!("zip start {name}: {e}")))?;
                zip_writer.write_all(&buf).map_err(CoreError::from)?;
            }
        }
        zip_writer
            .finish()
            .map_err(|e| CoreError::Llm(format!("zip finish: {e}")))?;

        if !unresolved.is_empty() {
            // The output file was written; we still flag the issue so the
            // LLM can ask the user to fix the template. Showing up to 5
            // keys keeps the message short.
            return Ok(ToolOutput::text(format!(
                "Wrote {} but {} placeholder(s) were not substituted: {}. \
                 They are likely split across formatting runs in the template — \
                 ask the user to retype them as plain text and try again.",
                parsed.output_path,
                unresolved.len(),
                unresolved.join(", ")
            )));
        }

        Ok(ToolOutput::text(format!(
            "Wrote {} from template {} ({} replacement(s) applied).",
            parsed.output_path,
            parsed.template_path,
            replacements.len()
        )))
    }
}
