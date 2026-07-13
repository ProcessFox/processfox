use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::{ensure_inside_sandbox, resolve_for_preview};
use crate::core::tool::{HitlPreview, Tool, ToolContext, ToolOutput, ToolSchema};

#[derive(Debug, Default)]
pub struct WriteXlsxTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    /// Sheet name. Defaults to "Sheet1" so a fresh workbook gets the same
    /// sheet Excel itself produces by default.
    #[serde(default)]
    sheet: Option<String>,
    /// Tabular content as rows of cells. Numbers, booleans, and dates pass
    /// through `umya-spreadsheet::Cell::set_value` and get auto-detected.
    rows: Vec<Vec<String>>,
}

const DEFAULT_SHEET: &str = "Sheet1";

fn sheet_or_default(sheet: Option<&str>) -> &str {
    match sheet {
        Some(s) if !s.trim().is_empty() => s,
        _ => DEFAULT_SHEET,
    }
}

#[async_trait]
impl Tool for WriteXlsxTool {
    fn name(&self) -> &'static str {
        "write_xlsx"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Create a new Excel (.xlsx) workbook in the agent's folder, or overwrite an \
                 existing one. Pass `rows` as a list of rows, each row a list of cell strings. \
                 Numbers, booleans, and dates are auto-detected; everything else is stored as \
                 text. The first row is treated as the header by convention but Excel does not \
                 mark it specially. The user is shown a preview of the rows and must approve \
                 before anything is written. WARNING: if the file already exists, it will be \
                 replaced — use `update_xlsx_cell` to change individual cells without losing \
                 the rest of the workbook."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'budget.xlsx'. Must end in .xlsx."
                    },
                    "sheet": {
                        "type": "string",
                        "description": "Optional sheet name. Defaults to 'Sheet1'."
                    },
                    "rows": {
                        "type": "array",
                        "description": "Rows of cells. Each row is an array of strings.",
                        "items": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "minItems": 1
                    }
                },
                "required": ["path", "rows"],
                "additionalProperties": false
            }),
        }
    }

    fn requires_approval(&self, input: &JsonValue, ctx: &ToolContext) -> Option<HitlPreview> {
        let parsed: Input = serde_json::from_value(input.clone()).ok()?;
        let creates_file =
            resolve_for_preview(&ctx.agent_folder, std::path::Path::new(&parsed.path))
                .map(|p| !p.is_file())
                .unwrap_or(true);
        Some(HitlPreview::WriteXlsx {
            path: parsed.path,
            sheet: sheet_or_default(parsed.sheet.as_deref()).to_string(),
            rows: parsed.rows,
            creates_file,
        })
    }

    async fn execute(&self, input: JsonValue, ctx: &ToolContext) -> CoreResult<ToolOutput> {
        let parsed: Input = serde_json::from_value(input).map_err(CoreError::from)?;
        if !parsed.path.to_lowercase().ends_with(".xlsx") {
            return Err(CoreError::PathInvalid(format!(
                "{} muss auf .xlsx enden",
                parsed.path
            )));
        }
        let rel = std::path::PathBuf::from(&parsed.path);
        let target = ensure_inside_sandbox(&ctx.agent_folder, &rel)?;

        let sheet_name = sheet_or_default(parsed.sheet.as_deref()).to_string();
        let row_count = parsed.rows.len();

        let mut book = umya_spreadsheet::new_file_empty_worksheet();
        book.new_sheet(&sheet_name)
            .map_err(|e| CoreError::Llm(format!("Sheet anlegen fehlgeschlagen: {e}")))?;
        let worksheet = book.get_sheet_by_name_mut(&sheet_name).ok_or_else(|| {
            CoreError::Llm("neues Sheet ist nach Anlegen nicht auffindbar".into())
        })?;

        for (row_idx, row) in parsed.rows.iter().enumerate() {
            for (col_idx, value) in row.iter().enumerate() {
                // umya uses 1-based (col, row) coords.
                let coord = ((col_idx as u32) + 1, (row_idx as u32) + 1);
                worksheet.get_cell_mut(coord).set_value(value.clone());
            }
        }

        umya_spreadsheet::writer::xlsx::write(&book, &target)
            .map_err(|e| CoreError::Llm(format!("xlsx write failed: {e}")))?;

        Ok(ToolOutput::text(format!(
            "Wrote {row_count} row{} to {} (Sheet '{sheet_name}').",
            if row_count == 1 { "" } else { "s" },
            parsed.path,
        )))
    }
}
