---
name: document-create-docx
title: Word-Dokument erstellen
description: Create a new Microsoft Word (.docx) file from a structured text input — e.g. a meeting summary, an offer letter, a one-pager. Existing files are not touched (use `document-extend` or `document-edit` for that). Every write asks the user for confirmation first.
icon: FilePlus
tools:
  - list_folder
  - read_file
  - write_docx
hitl:
  default: true
language: en
---

You can create a new `.docx` document in the agent's folder. Use this for one-shot deliverables: offers, summaries, notes-to-share, simple reports.

Workflow — follow it in this order, every time:

1. If the target filename is unclear, run `list_folder` first so you don't overwrite an existing document by accident. Pick a sensibly-named `.docx` (e.g. `offer-2026-04-25.docx`, `meeting-minutes.docx`).
2. **Refuse to overwrite an existing `.docx`** unless the user explicitly says "overwrite" or "replace". Existing Word formatting is lost when you call `write_docx`. If the user wants to extend a document, redirect them to the `document-extend` workflow.
3. Compose the new content using this small Markdown-flavoured syntax:
   - `# Title` → top-level heading
   - `## Section` → section heading
   - `### Subsection` → subsection heading
   - `- Bullet text` → bulleted line
   - blank line between paragraphs
   That is the entire syntax `write_docx` understands; do not use `**bold**`, links, or tables — they are passed through as literal text.
4. Call `write_docx` with the path and the content. The user sees a structural preview and approves before anything is written.
5. After the write goes through, confirm in one sentence what was created and where (cite the file path verbatim). If the user rejects, ask what to change.

Example:

User: "Erstelle mir bitte eine kurze Notiz für das Meeting morgen — Titel 'Quartalsplanung', drei Stichpunkte: Budget-Review, Personal, Q3-Ziele."

Agent calls (in order):
1. `list_folder({ path: "" })` → bestätigt, dass `quartalsplanung.docx` noch nicht existiert.
2. `write_docx({ path: "quartalsplanung.docx", content: "# Quartalsplanung\n\n- Budget-Review\n- Personal\n- Q3-Ziele" })`
