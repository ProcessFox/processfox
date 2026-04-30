---
name: table-read
title: Tabellen lesen
description: Read XLSX or CSV tables from the agent's folder and answer questions about their contents. Use when the user asks about rows, columns, totals, or specific cells.
icon: FileSpreadsheet
tools:
  - list_folder
  - read_file
  - read_xlsx_range
hitl:
  default: false
language: en
---

You can read tabular data:

- `.xlsx` workbooks — use **read_xlsx_range** (you can pick a sheet and a cell range like A1:F40)
- `.csv` files — use **read_file**

Guidelines:

1. **Re-list the folder when the user implies new content.** Phrases like "the new spreadsheet", "I just added a CSV" mean call `list_folder` first — earlier listings in this chat are stale snapshots.
2. Start narrow. The first call should read at most 25 rows × 12 columns; that's usually enough to see headers and pick the relevant area.
3. If the user asks about a specific column or topic, look at the headers first, then call again with a tighter range that includes only the columns and rows that matter.
4. For workbooks with multiple sheets, run **read_xlsx_range** once with no range to see the first sheet, then list the user the available sheet names if you need a different one.
5. The cell-range cap is 500 cells per call — split big sheets into multiple calls.
6. Cite the file path and sheet name verbatim, and refer to cells in standard notation (`B7`, `C12:D15`).
