---
name: table-update
title: Tabelle ändern
description: Update single cells in an Excel (.xlsx) workbook in the agent's folder. Useful for fixing typos, recording new figures, or filling in missing values. Every change is shown cell-by-cell with before/after and the user must approve before anything is written.
icon: FilePen
tools:
  - list_folder
  - read_xlsx_range
  - update_xlsx_cell
  - ask_user
hitl:
  default: true
language: en
---

You can write into individual cells of a `.xlsx` workbook. Use this for narrow, targeted edits — fix a typo, replace a number, fill an empty cell. For sweeping rewrites or new sheets, ask the user to do it manually; this skill is for surgical updates.

Workflow — follow it in this order, every time:

1. If the target file or sheet is unclear, run `list_folder` to find the workbook and ask the user which sheet to edit.
2. **You MUST call `read_xlsx_range` on the relevant area before `update_xlsx_cell`.** This is non-negotiable. You need to see the actual contents to identify the right cells; without it you risk overwriting unrelated data because column letters and row numbers can drift between users' workbooks.
3. From the read result, locate the exact A1-style cell references the user wants changed. Repeat the read with a wider range if the first one didn't include the relevant rows.
4. Call `update_xlsx_cell` with `path`, optional `sheet`, and a `changes` list. The user will see a small table (cell | before | after) and must approve. Numbers like `"42"` and dates like `"2026-04-26"` are auto-detected by Excel's parser; pass them as plain strings.
5. After the update goes through, confirm in one sentence which cells changed and on which sheet (cite the file path verbatim). If the user rejects, ask what to change.

Anti-patterns — never do these:
- Calling `update_xlsx_cell` without a prior `read_xlsx_range` ("I'll just update B5" — no, you don't know what's in B5).
- Updating a long contiguous range cell by cell when the user described it as a single edit ("set B2:B10 to 0") — instead pass them as one batch in a single `update_xlsx_cell` call so the user approves once.
- Inventing cell references from the user's description without verifying against the workbook.
- **Refusing to add a new column or row because "you can only edit existing cells".** That is wrong. Excel cells beyond the current data range are simply empty — writing into `D1` when the current data only fills `A:C` adds a new "Kommentar" column with no schema change. `update_xlsx_cell` can target ANY A1 reference. Suggesting the user manually copy-paste, or proposing to recreate the workbook from scratch with `write_xlsx`, just because they asked for a new column is exactly the mistake to avoid.

Example — adding a new column "Kommentar" with one entry:

User: "Trag in der Zeile vom Spezialisten in einer neuen Spalte 'Kommentar' den Hinweis 'Bitte auf Missverständnisse prüfen' ein."

Agent calls (in order):
1. `read_xlsx_range({ path: "data.xlsx", range: "A1:C20" })` → sees columns A/B/C are filled, "Der Spezialist" is in row 9, no D column yet.
2. `update_xlsx_cell({ path: "data.xlsx", changes: [{ cell: "D1", value: "Kommentar" }, { cell: "D9", value: "Bitte auf Missverständnisse prüfen" }] })`

That's it — two cell updates, the new column appears. No `write_xlsx`, no recreating the file.

Example:

User: "In `budget.xlsx`, Zeile 'Marketing' auf 12000 setzen."

Agent calls (in order):
1. `read_xlsx_range({ path: "budget.xlsx", range: "A1:C20" })` → sees `Marketing` in row 7, column B holds the budget figure currently `10000`.
2. `update_xlsx_cell({ path: "budget.xlsx", sheet: "Budget", changes: [{ cell: "B7", value: "12000" }] })`

The HitlCard shows:

| Zelle | Vorher | Nachher |
|-------|--------|---------|
| B7    | 10000  | 12000   |

The user approves; the workbook is rewritten with that single change.
