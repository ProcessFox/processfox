use async_trait::async_trait;
use calamine::{open_workbook_auto, Data, Reader};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::path::PathBuf;

use crate::core::error::{CoreError, CoreResult};
use crate::core::sandbox::ensure_in_agent_folder;
use crate::core::tool::{Tool, ToolContext, ToolOutput, ToolSchema};

/// Hard cap to keep the LLM context predictable. ~500 cells is enough to
/// understand a sheet's structure; if the user needs more, they call us
/// again with a tighter range.
const MAX_CELLS: u32 = 500;

#[derive(Debug, Default)]
pub struct ReadXlsxRangeTool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    path: String,
    #[serde(default)]
    sheet: Option<String>,
    /// Top-left cell of the range to read, e.g. "A1". Defaults to "A1".
    #[serde(default)]
    start: Option<String>,
    /// Bottom-right cell, e.g. "F40". Defaults to a 25-row, 12-column window.
    #[serde(default)]
    end: Option<String>,
}

#[async_trait]
impl Tool for ReadXlsxRangeTool {
    fn name(&self) -> &'static str {
        "read_xlsx_range"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description:
                "Read a rectangular range of cells from an .xlsx workbook in the agent's folder. \
                 Returns the values as CSV (one row per line). When 'sheet' is omitted the \
                 first sheet is used; when 'start'/'end' are omitted the top-left 25×12 window \
                 is read. Maximum 500 cells per call — call again with a tighter range for more."
                    .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the agent's folder, e.g. 'members.xlsx'."
                    },
                    "sheet": {
                        "type": "string",
                        "description": "Name of the worksheet. Omit to use the first sheet."
                    },
                    "start": {
                        "type": "string",
                        "description": "Top-left cell of the range, e.g. 'A1'. Defaults to 'A1'."
                    },
                    "end": {
                        "type": "string",
                        "description": "Bottom-right cell, e.g. 'F40'. Defaults to a 25×12 window."
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

        let target_owned = target.clone();
        let parsed_owned = parsed.clone();
        let result =
            tokio::task::spawn_blocking(move || extract_range(&target_owned, &parsed_owned))
                .await
                .map_err(|e| CoreError::Llm(format!("XLSX-Lesen abgebrochen: {e}")))??;

        Ok(ToolOutput::text(result))
    }
}

impl Clone for Input {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            sheet: self.sheet.clone(),
            start: self.start.clone(),
            end: self.end.clone(),
        }
    }
}

fn extract_range(path: &std::path::Path, params: &Input) -> CoreResult<String> {
    let mut workbook =
        open_workbook_auto(path).map_err(|e| CoreError::Llm(format!("XLSX nicht lesbar: {e}")))?;
    let sheet_names = workbook.sheet_names();
    if sheet_names.is_empty() {
        return Err(CoreError::Llm("Workbook hat keine Sheets.".to_string()));
    }
    let sheet_name = match params.sheet.as_ref() {
        Some(s) if !s.trim().is_empty() => {
            if !sheet_names.iter().any(|n| n == s) {
                return Err(CoreError::Llm(format!(
                    "Sheet '{s}' nicht gefunden. Verfügbar: {}",
                    sheet_names.join(", ")
                )));
            }
            s.clone()
        }
        _ => sheet_names[0].clone(),
    };

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| CoreError::Llm(format!("Sheet konnte nicht geladen werden: {e}")))?;

    let (start_row, start_col) = parse_cell(params.start.as_deref().unwrap_or("A1"))?;
    let (end_row, end_col) = match params.end.as_deref() {
        Some(s) if !s.trim().is_empty() => parse_cell(s)?,
        _ => (start_row + 24, start_col + 11),
    };
    if end_row < start_row || end_col < start_col {
        return Err(CoreError::Llm("end-Zelle liegt vor start.".to_string()));
    }

    let cell_count = (end_row - start_row + 1) * (end_col - start_col + 1);
    if cell_count > MAX_CELLS {
        return Err(CoreError::Llm(format!(
            "Range enthält {cell_count} Zellen (cap {MAX_CELLS}). Bitte Range einschränken."
        )));
    }

    let mtime = crate::core::tool::mtime_suffix(path);
    // mtime_suffix yields ", modified <ts>" — flip to " · modified <ts>" to
    // match the xlsx header's middot separator style.
    let mtime_segment = if mtime.is_empty() {
        String::new()
    } else {
        format!(" ·{}", mtime.trim_start_matches(','))
    };
    let mut out = format!(
        "--- {} · sheet='{}' · {}:{}{} ---\n",
        params.path,
        sheet_name,
        params.start.as_deref().unwrap_or("A1"),
        params
            .end
            .as_deref()
            .unwrap_or(&col_letter(end_col).to_string()),
        mtime_segment
    );
    for row in start_row..=end_row {
        let mut line = String::new();
        for col in start_col..=end_col {
            if col > start_col {
                line.push(',');
            }
            let cell = range.get_value((row, col));
            line.push_str(&format_cell(cell));
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

fn format_cell(cell: Option<&Data>) -> String {
    match cell {
        None | Some(Data::Empty) => String::new(),
        Some(Data::String(s)) => csv_escape(s),
        Some(Data::Float(f)) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Some(Data::Int(i)) => i.to_string(),
        Some(Data::Bool(b)) => b.to_string(),
        Some(Data::DateTime(dt)) => format!("{}", dt.as_f64()),
        Some(Data::DateTimeIso(s)) => csv_escape(s),
        Some(Data::DurationIso(s)) => csv_escape(s),
        Some(Data::Error(e)) => format!("#err:{e:?}"),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Parse "A1", "B7", "AA12" into (row_index, col_index), 0-based.
fn parse_cell(spec: &str) -> CoreResult<(u32, u32)> {
    let mut col = 0u32;
    let mut row_start = 0;
    for (i, c) in spec.chars().enumerate() {
        if c.is_ascii_alphabetic() {
            col = col * 26 + (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
            row_start = i + 1;
        } else {
            break;
        }
    }
    if col == 0 {
        return Err(CoreError::Llm(format!("Ungültige Zellen-Adresse: {spec}")));
    }
    let row: u32 = spec[row_start..]
        .parse()
        .map_err(|_| CoreError::Llm(format!("Ungültige Zellen-Adresse: {spec}")))?;
    if row == 0 {
        return Err(CoreError::Llm(format!(
            "Zeilennummern beginnen bei 1: {spec}"
        )));
    }
    Ok((row - 1, col - 1))
}

fn col_letter(col: u32) -> String {
    let mut col = col + 1;
    let mut s = String::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        s.insert(0, (b'A' + rem as u8) as char);
        col = (col - 1) / 26;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cell_works() {
        assert_eq!(parse_cell("A1").unwrap(), (0, 0));
        assert_eq!(parse_cell("B2").unwrap(), (1, 1));
        assert_eq!(parse_cell("AA10").unwrap(), (9, 26));
    }
}
