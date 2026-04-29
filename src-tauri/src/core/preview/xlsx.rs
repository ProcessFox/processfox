//! XLSX preview — pull a single sheet's cells into a 2D string grid that
//! the UI can render as a plain table. Calamine handles the heavy lifting;
//! we just convert each cell value to a display string and clip the
//! sheet to a hard cap so a million-row workbook can't lock up the
//! renderer.

use std::path::Path;

use calamine::{open_workbook_auto, Data, Range, Reader};
use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};

/// Max cells we hand to the frontend per sheet. 1000×50 keeps a plain
/// HTML table snappy on commodity hardware while still showing enough
/// of a typical sheet to be useful.
pub const MAX_ROWS: usize = 1000;
pub const MAX_COLS: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxPreview {
    /// All sheet names in workbook order — used by the UI to render tabs.
    pub sheets: Vec<String>,
    /// Name of the sheet whose data is in `rows`.
    pub active_sheet: String,
    /// Cells as display strings, row-major. Trimmed to MAX_ROWS × MAX_COLS.
    pub rows: Vec<Vec<String>>,
    /// Total dimensions of the active sheet *before* trimming, so the UI
    /// can show "showing first 1000 of 8232 rows".
    pub total_rows: usize,
    pub total_cols: usize,
    pub truncated: bool,
}

pub fn xlsx_preview(path: &Path, requested_sheet: Option<&str>) -> CoreResult<XlsxPreview> {
    let mut workbook =
        open_workbook_auto(path).map_err(|e| CoreError::Llm(format!("XLSX nicht lesbar: {e}")))?;

    let sheet_names = workbook.sheet_names();
    if sheet_names.is_empty() {
        return Err(CoreError::Llm("Workbook hat keine Sheets.".to_string()));
    }

    let active_sheet: String = match requested_sheet {
        Some(s) if !s.trim().is_empty() => {
            if !sheet_names.iter().any(|n| n == s) {
                return Err(CoreError::Llm(format!(
                    "Sheet '{s}' nicht gefunden. Verfügbar: {}",
                    sheet_names.join(", ")
                )));
            }
            s.to_string()
        }
        _ => sheet_names[0].clone(),
    };

    let range = workbook
        .worksheet_range(&active_sheet)
        .map_err(|e| CoreError::Llm(format!("Sheet konnte nicht geladen werden: {e}")))?;

    let (total_rows, total_cols) = range.get_size();
    let rows = render_grid(&range);
    let truncated = total_rows > MAX_ROWS || total_cols > MAX_COLS;

    Ok(XlsxPreview {
        sheets: sheet_names,
        active_sheet,
        rows,
        total_rows,
        total_cols,
        truncated,
    })
}

fn render_grid(range: &Range<Data>) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    for (r, row) in range.rows().enumerate() {
        if r >= MAX_ROWS {
            break;
        }
        let mut cells: Vec<String> = Vec::with_capacity(row.len().min(MAX_COLS));
        for (c, cell) in row.iter().enumerate() {
            if c >= MAX_COLS {
                break;
            }
            cells.push(format_cell(cell));
        }
        out.push(cells);
    }
    out
}

/// Render a calamine Data into a human-friendly string. Numbers stay as
/// numbers (with integers shown without a trailing `.0`); errors get a
/// short tag; date/time values fall through to their stored form.
fn format_cell(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => format!("{}", dt.as_f64()),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#err:{e:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_without_trailing_zero() {
        assert_eq!(format_cell(&Data::Float(42.0)), "42");
        assert_eq!(format_cell(&Data::Float(2.5)), "2.5");
        assert_eq!(format_cell(&Data::Int(7)), "7");
    }

    #[test]
    fn formats_text_and_empty() {
        assert_eq!(format_cell(&Data::String("hi".into())), "hi");
        assert_eq!(format_cell(&Data::Empty), "");
    }
}
