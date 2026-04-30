---
name: table-create
title: Tabelle erstellen
description: Create a new Excel (.xlsx) workbook from a small structured input — e.g. a budget, a contact list, a quick comparison. Existing workbooks are not touched (use `table-update` for that). Every write asks the user for confirmation first.
icon: FilePlus
tools:
  - list_folder
  - write_xlsx
hitl:
  default: true
language: en
---

You can create a new `.xlsx` workbook in the agent's folder. Use this for one-shot deliverables: a budget, a comparison table, a small list the user wants to share or open in Excel.

Workflow — follow it in this order, every time:

1. If the target filename is unclear, run `list_folder` first so you don't overwrite an existing workbook by accident. Pick a sensibly-named `.xlsx` (e.g. `budget-2026-q3.xlsx`, `kontakte.xlsx`).
2. **Refuse to overwrite an existing `.xlsx`** unless the user explicitly says "overwrite" or "replace". Existing sheets, formatting, and formulas are lost when you call `write_xlsx`. If the user wants to change a single value, redirect them to the `table-update` workflow.
3. Build the table as `rows`: a list of rows, each row a list of strings. The first row should be the header (column names) by convention. All rows should have the same number of columns; pad shorter rows with `""`.
4. Numbers, booleans, and dates pass straight through — Excel auto-detects them. Use the user's locale where it makes sense (`"12.000,50"` for German Excel, `"12,000.50"` for English Excel) but when in doubt, use unambiguous plain numbers like `"12000.5"`.
5. Call `write_xlsx` with `path`, optional `sheet` (defaults to `"Sheet1"`), and `rows`. The user sees a preview of the table and approves before anything is written.
6. After the write goes through, confirm in one sentence what was created and where (cite the file path verbatim). If the user rejects, ask what to change.

Example:

User: "Lege ein einfaches Budget in `q3-budget.xlsx` an: Marketing 12000, Personal 50000, IT 15000."

Agent calls (in order):
1. `list_folder({ path: "" })` → bestätigt, dass `q3-budget.xlsx` noch nicht existiert.
2. `write_xlsx({ path: "q3-budget.xlsx", sheet: "Budget", rows: [["Position", "Betrag"], ["Marketing", "12000"], ["Personal", "50000"], ["IT", "15000"]] })`

The HitlCard shows the rows as a 2-column × 4-row table; the user approves; the workbook opens in Excel with `Position`/`Betrag` headers and three numeric rows.
