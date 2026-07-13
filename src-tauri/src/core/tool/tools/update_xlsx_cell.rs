use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::{ensure_inside_sandbox, resolve_for_preview};
use crate::core::tool::{CellChange, HitlPreview, Tool, ToolContext, ToolOutput, ToolSchema};

#[derive(Debug, Default)]
pub struct UpdateXlsxCellTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    /// Sheet name. If absent, the first sheet in the workbook is targeted.
    #[serde(default)]
    sheet: Option<String>,
    changes: Vec<CellInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CellInput {
    /// A1-style cell reference, e.g. "B12".
    cell: String,
    /// New cell value. Numbers and booleans are auto-detected by
    /// `umya-spreadsheet::Cell::set_value`; everything else is stored as text.
    value: String,
}

/// Resolve the workbook + the requested sheet (or default sheet 0) and read
/// the current value of every cell named in `cells`. Returns Cell references
/// in the form `(cell_ref, current_value_as_string)`. Empty cells return an
/// empty string.
fn read_current_values(
    workbook_path: &std::path::Path,
    sheet: Option<&str>,
    cells: &[String],
) -> Result<(String, Vec<(String, String)>), String> {
    let book = umya_spreadsheet::reader::xlsx::read(workbook_path)
        .map_err(|e| format!("xlsx read failed: {e}"))?;
    let worksheet = match sheet {
        Some(name) => book
            .get_sheet_by_name(name)
            .ok_or_else(|| format!("Sheet '{name}' nicht gefunden"))?,
        None => book
            .get_sheet(&0)
            .ok_or_else(|| "Workbook hat keine Sheets".to_string())?,
    };
    let resolved_sheet_name = worksheet.get_name().to_string();
    let pairs = cells
        .iter()
        .map(|cell_ref| {
            let value = match worksheet.get_cell(cell_ref.as_str()) {
                Some(c) => c.get_value().to_string(),
                None => String::new(),
            };
            (cell_ref.clone(), value)
        })
        .collect();
    Ok((resolved_sheet_name, pairs))
}

#[async_trait]
impl Tool for UpdateXlsxCellTool {
    fn name(&self) -> &'static str {
        "update_xlsx_cell"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Update one or more cells in an existing Excel (.xlsx) workbook in the agent's \
                 folder. Pass a list of {cell, value} entries where `cell` is ANY A1-style \
                 reference (e.g. 'B12', 'D1') — including cells in columns or rows that are \
                 currently empty, which is exactly how you add a new column or row without \
                 recreating the workbook. `value` is the new content. Numbers, booleans, and \
                 dates are auto-detected; everything else stays as text. The user is shown a \
                 cell-by-cell before/after diff and must approve before anything is written. \
                 You MUST call `read_xlsx_range` first so you know the current contents — \
                 otherwise you risk writing into the wrong cell."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'budget.xlsx'."
                    },
                    "sheet": {
                        "type": "string",
                        "description": "Optional sheet name. Defaults to the first sheet."
                    },
                    "changes": {
                        "type": "array",
                        "description": "List of cell updates.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "cell": {
                                    "type": "string",
                                    "description": "A1-style cell reference, e.g. 'A1' or 'B12'."
                                },
                                "value": {
                                    "type": "string",
                                    "description": "New cell value."
                                }
                            },
                            "required": ["cell", "value"],
                            "additionalProperties": false
                        },
                        "minItems": 1
                    }
                },
                "required": ["path", "changes"],
                "additionalProperties": false
            }),
        }
    }

    fn requires_approval(&self, input: &JsonValue, ctx: &ToolContext) -> Option<HitlPreview> {
        let parsed: Input = serde_json::from_value(input.clone()).ok()?;
        if parsed.changes.is_empty() {
            return None;
        }
        let Some(resolved) =
            resolve_for_preview(&ctx.agent_folder, std::path::Path::new(&parsed.path))
        else {
            // Not resolvable inside the sandbox — missing, or outside the
            // agent folder entirely. The HITL preview can't show
            // before-values in that case either way. Surface a synthetic
            // preview so the user still sees what would happen; `execute`
            // enforces the real boundary/existence check and fails cleanly.
            let changes = parsed
                .changes
                .iter()
                .map(|c| CellChange {
                    cell: c.cell.clone(),
                    before: String::new(),
                    after: c.value.clone(),
                })
                .collect();
            return Some(HitlPreview::UpdateCells {
                path: parsed.path,
                sheet: parsed.sheet.unwrap_or_else(|| "(file missing)".into()),
                changes,
            });
        };

        let cell_refs: Vec<String> = parsed.changes.iter().map(|c| c.cell.clone()).collect();
        let (sheet_name, before_pairs) =
            match read_current_values(&resolved, parsed.sheet.as_deref(), &cell_refs) {
                Ok(v) => v,
                Err(_) => {
                    // Fall back to empty before-values so the preview still
                    // renders. execute() will fail with a clean error.
                    (
                        parsed.sheet.clone().unwrap_or_else(|| "?".into()),
                        cell_refs
                            .iter()
                            .map(|c| (c.clone(), String::new()))
                            .collect(),
                    )
                }
            };
        let changes: Vec<CellChange> = parsed
            .changes
            .iter()
            .zip(before_pairs)
            .map(|(input_change, (_cell, before))| CellChange {
                cell: input_change.cell.clone(),
                before,
                after: input_change.value.clone(),
            })
            .collect();
        Some(HitlPreview::UpdateCells {
            path: parsed.path,
            sheet: sheet_name,
            changes,
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
        if !target.is_file() {
            return Err(CoreError::PathInvalid(format!(
                "Datei {} existiert nicht — update_xlsx_cell legt keine neuen Workbooks an.",
                parsed.path
            )));
        }

        let mut book = umya_spreadsheet::reader::xlsx::read(&target)
            .map_err(|e| CoreError::Llm(format!("xlsx read failed: {e}")))?;

        let sheet_name = parsed.sheet.clone();
        let worksheet = match sheet_name.as_deref() {
            Some(name) => book
                .get_sheet_by_name_mut(name)
                .ok_or_else(|| CoreError::Llm(format!("Sheet '{name}' nicht gefunden")))?,
            None => book
                .get_sheet_mut(&0)
                .ok_or_else(|| CoreError::Llm("Workbook hat keine Sheets".to_string()))?,
        };

        for change in &parsed.changes {
            worksheet
                .get_cell_mut(change.cell.as_str())
                .set_value(change.value.clone());
        }

        umya_spreadsheet::writer::xlsx::write(&book, &target)
            .map_err(|e| CoreError::Llm(format!("xlsx write failed: {e}")))?;

        Ok(ToolOutput::text(format!(
            "Updated {} cell{} in {} ({}).",
            parsed.changes.len(),
            if parsed.changes.len() == 1 { "" } else { "s" },
            parsed.path,
            parsed.sheet.as_deref().unwrap_or("first sheet")
        )))
    }
}
