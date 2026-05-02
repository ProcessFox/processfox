use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tauri::{Emitter, Manager};

use crate::core::chat::run::RunEvent;
use crate::core::error::{CoreError, CoreResult};
use crate::core::llm::TokenUsage;
use crate::core::tool::{
    DelegationSamplePrompt, HitlPreview, Tool, ToolContext, ToolOutput, ToolSchema,
};
use crate::state::AppState;

use super::write_docx::ensure_inside_sandbox;

const SAMPLE_PROMPT_COUNT: usize = 3;

#[derive(Debug, Default)]
pub struct DelegateIntoXlsxColumnTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    /// Sheet name. If absent, the first sheet in the workbook is targeted.
    #[serde(default)]
    sheet: Option<String>,
    /// Optional row filter. Only rows where `column == equals` (string
    /// comparison after `to_string`) are processed; the rest are left
    /// untouched.
    #[serde(default)]
    filter: Option<RowFilter>,
    /// Header names referenced by `task_template`. Used to validate the
    /// template before the run kicks off — if a referenced header is
    /// missing, the tool errors out instead of letting the LLM produce
    /// nonsense from an unsubstituted placeholder.
    source_columns: Vec<String>,
    /// Header name to write the generated text into. If the header is not
    /// present in row 1, the tool appends a new column with this name and
    /// fills it.
    target_column: String,
    /// Free-form prompt; `{{header_name}}` placeholders are replaced with
    /// each row's values before the worker runs.
    task_template: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RowFilter {
    column: String,
    equals: String,
}

#[async_trait]
impl Tool for DelegateIntoXlsxColumnTool {
    fn name(&self) -> &'static str {
        "delegate_into_xlsx_column"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: "Run a per-row LLM generation step on every (filtered) row of an Excel \
                 (.xlsx) workbook in the agent's folder, writing the output into one column. \
                 Use this for bulk tasks like 'write a one-line description for each product', \
                 'summarize each note', or 'translate each row's title' — anything where you \
                 would otherwise have to call write tools 30+ times in a loop. The first row \
                 of the sheet is treated as a header. Reference columns by their header name \
                 in `taskTemplate` with `{{header_name}}` placeholders. If `targetColumn` is \
                 not in the headers, a new column is appended; existing values in `targetColumn` \
                 will be overwritten. The user is shown one approval card before the batch \
                 runs (with row count, worker model, and a few sample rendered prompts) and \
                 can cancel mid-batch — already-written rows stay written."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the .xlsx file relative to the agent's folder."
                    },
                    "sheet": {
                        "type": "string",
                        "description": "Optional sheet name. Defaults to the first sheet."
                    },
                    "filter": {
                        "type": "object",
                        "description": "Optional row filter. Only rows where the named column equals the given string value are processed.",
                        "properties": {
                            "column": { "type": "string" },
                            "equals": { "type": "string" }
                        },
                        "required": ["column", "equals"],
                        "additionalProperties": false
                    },
                    "sourceColumns": {
                        "type": "array",
                        "description": "Header names referenced inside `taskTemplate`. Must match the header row exactly.",
                        "items": { "type": "string" },
                        "minItems": 1
                    },
                    "targetColumn": {
                        "type": "string",
                        "description": "Header name to write the generated text into. New header is created if it doesn't exist."
                    },
                    "taskTemplate": {
                        "type": "string",
                        "description": "Prompt template with `{{header}}` placeholders. Example: 'Write a 1-sentence description for product: {{name}} ({{category}}).'"
                    }
                },
                "required": ["path", "sourceColumns", "targetColumn", "taskTemplate"],
                "additionalProperties": false
            }),
        }
    }

    fn requires_approval(&self, input: &JsonValue, ctx: &ToolContext) -> Option<HitlPreview> {
        let parsed: Input = serde_json::from_value(input.clone()).ok()?;
        let resolved_path = ctx.agent_folder.join(&parsed.path);
        if !resolved_path.is_file() {
            return None;
        }

        let book = umya_spreadsheet::reader::xlsx::read(&resolved_path).ok()?;
        let worksheet = match parsed.sheet.as_deref() {
            Some(name) => book.get_sheet_by_name(name)?,
            None => book.get_sheet(&0)?,
        };
        let sheet_name = worksheet.get_name().to_string();
        let highest_col = worksheet.get_highest_column();
        let highest_row = worksheet.get_highest_row();
        let header_to_col = read_headers(worksheet, highest_col);
        let target_creates_column = !header_to_col.contains_key(&parsed.target_column);

        let mut affected: Vec<(u32, BTreeMap<String, String>)> = Vec::new();
        for row in 2..=highest_row {
            let values = read_row(worksheet, &header_to_col, row);
            if matches_filter(&values, parsed.filter.as_ref()) {
                affected.push((row, values));
            }
        }

        let app_state = ctx.app.state::<AppState>();
        let agent = app_state.agent_repo().get(&ctx.agent_id).ok()?;
        let runner = app_state.delegation_runner();
        let resolved_profile = runner.resolve_for(&agent).ok()?;
        let worker_label = format!(
            "{} · {}",
            resolved_profile.provider_id, resolved_profile.model_id
        );

        let sample_prompts: Vec<DelegationSamplePrompt> = affected
            .iter()
            .take(SAMPLE_PROMPT_COUNT)
            .map(|(row_idx, values)| DelegationSamplePrompt {
                row_label: format!("Zeile {row_idx}"),
                rendered_prompt: render_template(&parsed.task_template, values),
            })
            .collect();

        Some(HitlPreview::DelegateIntoXlsxColumn {
            path: parsed.path,
            sheet: sheet_name,
            target_column: parsed.target_column,
            target_creates_column,
            row_count: affected.len() as u32,
            worker_label,
            sample_prompts,
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
                "Datei {} existiert nicht.",
                parsed.path
            )));
        }

        let app_state = ctx.app.state::<AppState>();
        let agent = app_state.agent_repo().get(&ctx.agent_id)?;
        let runner = app_state.delegation_runner();
        let resolved_profile = runner.resolve_for(&agent)?;

        let mut book = umya_spreadsheet::reader::xlsx::read(&target)
            .map_err(|e| CoreError::Llm(format!("xlsx read failed: {e}")))?;

        // Inspect the workbook once to figure out headers, target column,
        // and which rows need work. We hold the immutable borrow only here.
        let (sheet_name, header_to_col, target_col, target_creates_column, targets) = {
            let worksheet = match parsed.sheet.as_deref() {
                Some(name) => book
                    .get_sheet_by_name(name)
                    .ok_or_else(|| CoreError::Llm(format!("Sheet '{name}' nicht gefunden")))?,
                None => book
                    .get_sheet(&0)
                    .ok_or_else(|| CoreError::Llm("Workbook hat keine Sheets".to_string()))?,
            };
            let highest_col = worksheet.get_highest_column();
            let highest_row = worksheet.get_highest_row();
            let header_to_col = read_headers(worksheet, highest_col);
            for src in &parsed.source_columns {
                if !header_to_col.contains_key(src) {
                    return Err(CoreError::Llm(format!(
                        "Spalte '{src}' fehlt im Header von {}.",
                        parsed.path
                    )));
                }
            }
            let (tcol, tnew) = match header_to_col.get(&parsed.target_column) {
                Some(&c) => (c, false),
                None => (highest_col + 1, true),
            };
            let mut targets: Vec<(u32, BTreeMap<String, String>)> = Vec::new();
            for row in 2..=highest_row {
                let values = read_row(worksheet, &header_to_col, row);
                if matches_filter(&values, parsed.filter.as_ref()) {
                    targets.push((row, values));
                }
            }
            (
                worksheet.get_name().to_string(),
                header_to_col,
                tcol,
                tnew,
                targets,
            )
        };
        let _ = header_to_col;

        let total = targets.len() as u32;
        if total == 0 {
            return Ok(ToolOutput::text(format!(
                "Keine Zeilen passen zum Filter — nichts zu tun in {}.",
                parsed.path
            )));
        }

        if target_creates_column {
            let worksheet = book.get_sheet_by_name_mut(&sheet_name).ok_or_else(|| {
                CoreError::Llm(format!("Sheet '{sheet_name}' nicht mehr auffindbar"))
            })?;
            worksheet
                .get_cell_mut((target_col, 1))
                .set_value(parsed.target_column.clone());
        }

        let _ = ctx.app.emit(
            &ctx.channel,
            RunEvent::DelegationStarted {
                tool_call_id: ctx.tool_call_id.clone(),
                total,
            },
        );

        let mut succeeded = 0u32;
        let mut failed = 0u32;
        let mut first_error: Option<String> = None;
        let mut cancelled_partway = false;
        let mut total_usage = TokenUsage::default();
        let mut usage_seen = false;
        let started_at = std::time::Instant::now();

        for (idx, (row, values)) in targets.iter().enumerate() {
            if ctx.cancel.is_cancelled() {
                cancelled_partway = true;
                break;
            }

            let item_index = (idx as u32) + 1;
            let item_label = format!("Zeile {row}");
            let prompt = render_template(&parsed.task_template, values);

            match runner
                .run_one(&resolved_profile, prompt, ctx.cancel.clone())
                .await
            {
                Ok((text, usage)) => {
                    if let Some(u) = usage {
                        total_usage.accumulate(&u);
                        usage_seen = true;
                    }
                    let trimmed = text.trim().to_string();
                    let worksheet = book.get_sheet_by_name_mut(&sheet_name).ok_or_else(|| {
                        CoreError::Llm(format!("Sheet '{sheet_name}' nicht mehr auffindbar"))
                    })?;
                    worksheet
                        .get_cell_mut((target_col, *row))
                        .set_value(trimmed);
                    succeeded += 1;
                    let _ = ctx.app.emit(
                        &ctx.channel,
                        RunEvent::DelegationItemDone {
                            tool_call_id: ctx.tool_call_id.clone(),
                            index: item_index,
                            total,
                            item_label,
                        },
                    );
                }
                Err(CoreError::Cancelled) => {
                    cancelled_partway = true;
                    break;
                }
                Err(e) => {
                    failed += 1;
                    let msg = e.to_string();
                    if first_error.is_none() {
                        first_error = Some(msg.clone());
                    }
                    let _ = ctx.app.emit(
                        &ctx.channel,
                        RunEvent::DelegationItemFailed {
                            tool_call_id: ctx.tool_call_id.clone(),
                            index: item_index,
                            total,
                            item_label,
                            error: msg,
                        },
                    );
                }
            }
        }

        let duration_ms = started_at.elapsed().as_millis() as u64;
        tracing::info!(
            provider = %resolved_profile.provider_id,
            model = %resolved_profile.model_id,
            path = %parsed.path,
            target_column = %parsed.target_column,
            items_total = total,
            items_succeeded = succeeded,
            items_failed = failed,
            cancelled = cancelled_partway,
            duration_ms,
            input_tokens = total_usage.input_tokens,
            output_tokens = total_usage.output_tokens,
            cached_input_tokens = total_usage.cached_input_tokens,
            cache_creation_input_tokens = total_usage.cache_creation_input_tokens,
            tokens_reported = usage_seen,
            "delegation fan-out usage"
        );

        // Always persist whatever progress was made — partial results stay
        // in the workbook even if the run was cancelled mid-batch.
        umya_spreadsheet::writer::xlsx::write(&book, &target)
            .map_err(|e| CoreError::Llm(format!("xlsx write failed: {e}")))?;

        let _ = ctx.app.emit(
            &ctx.channel,
            RunEvent::DelegationFinished {
                tool_call_id: ctx.tool_call_id.clone(),
                succeeded,
                failed,
            },
        );

        if cancelled_partway {
            return Err(CoreError::Cancelled);
        }

        let mut summary = format!(
            "Wrote {succeeded} of {total} row{} to column '{}' in {} ({}).",
            if total == 1 { "" } else { "s" },
            parsed.target_column,
            parsed.path,
            sheet_name,
        );
        if failed > 0 {
            if let Some(err) = first_error {
                summary.push_str(&format!(
                    " {failed} row{} failed (first error: {err})",
                    if failed == 1 { "" } else { "s" }
                ));
            }
        }
        Ok(ToolOutput::text(summary))
    }
}

fn read_headers(
    worksheet: &umya_spreadsheet::Worksheet,
    highest_col: u32,
) -> BTreeMap<String, u32> {
    let mut out: BTreeMap<String, u32> = BTreeMap::new();
    for col in 1..=highest_col {
        let value = worksheet
            .get_cell((col, 1))
            .map(|c| c.get_value().to_string())
            .unwrap_or_default();
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            out.insert(trimmed.to_string(), col);
        }
    }
    out
}

fn read_row(
    worksheet: &umya_spreadsheet::Worksheet,
    header_to_col: &BTreeMap<String, u32>,
    row: u32,
) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for (header, &col) in header_to_col {
        let v = worksheet
            .get_cell((col, row))
            .map(|c| c.get_value().to_string())
            .unwrap_or_default();
        out.insert(header.clone(), v);
    }
    out
}

fn matches_filter(row_values: &BTreeMap<String, String>, filter: Option<&RowFilter>) -> bool {
    match filter {
        Some(f) => row_values
            .get(&f.column)
            .map(|v| v == &f.equals)
            .unwrap_or(false),
        None => true,
    }
}

fn render_template(template: &str, row_values: &BTreeMap<String, String>) -> String {
    let mut out = template.to_string();
    for (key, val) in row_values {
        let placeholder = format!("{{{{{key}}}}}");
        out = out.replace(&placeholder, val);
    }
    out
}
