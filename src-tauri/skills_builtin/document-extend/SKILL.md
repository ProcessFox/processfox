---
name: document-extend
title: Dokument fortschreiben
description: Append new entries to a running Markdown, text, or Word document in the agent's folder — useful for journaling, logging decisions, meeting minutes, or building a document over multiple turns. Every write asks the user for confirmation first.
icon: FileSignature
tools:
  - list_folder
  - read_file
  - read_docx
  - append_to_md
  - append_to_docx
hitl:
  default: true
language: en
---

You can extend a running document (Markdown, plain text, or Word) by appending new content. Use this for:

- Journals, decision logs, change logs
- Long-running notes the user is building up across multiple sessions
- Meeting minutes, status updates, ongoing reports
- Any "add this entry to file X" request

Pick the right append tool by extension:
- `.md`, `.markdown`, `.txt`, `.text` → `append_to_md`
- `.docx` → `append_to_docx`
- Anything else → ask the user.

Workflow — follow it in this order, every time:

1. If the user implies a fresh state ("the new note", "I just made a journal"), call `list_folder` first — earlier listings in this chat may be stale.
2. If the target file is unclear, run `list_folder` and pick a sensibly-named file (e.g. `journal.md`, `meeting-2026-04.docx`).
3. **You MUST read the target file before appending, unless you are certain it does not exist yet.** Use `read_file` for text/markdown, `read_docx` for Word. This is non-negotiable: without reading, you cannot match the existing format and your entry will look out of place.
4. From the existing content, extract the pattern: heading style (`## YYYY-MM-DD`, `### Mon DD, YYYY`, plain bullet, custom prefix like `25.04.2026 -- …`), the language entries are written in, indentation, whether entries carry metadata. Your new entry MUST mirror that pattern.
5. If the file is new, default to a heading on its own line, followed by the content. For markdown use `## YYYY-MM-DD`; for docx use `## YYYY-MM-DD` too — `append_to_docx` will turn it into a Heading 2 paragraph. Use today's date (see the date in the system prompt) unless the user names a different one.
6. Call the appropriate append tool. The user sees a preview, including the last lines of the existing file, and approves before anything is written. Both tools insert a blank line separator automatically — do not add leading blank lines.
7. After the append goes through, confirm in one sentence what was added and where (cite the file path verbatim). If the user rejects, ask what to change.

Example A — Markdown (file exists with the pattern `25.04.2026 -- Heute habe ich gelernt, was HITL ist.`):

User: "Notiere im Journal, dass ich gestern gelernt habe, was ein LLM ist."

Agent calls (in order):
1. `read_file({ path: "journal.md" })` → sees entries use `DD.MM.YYYY -- <text>`.
2. `append_to_md({ path: "journal.md", content: "24.04.2026 -- Gestern habe ich gelernt, was ein LLM ist." })`

Example B — DOCX (file exists with H2 headings per meeting):

User: "Trag das heutige Meeting ein, Themen: Q3-Ziele, Hiring, Budget."

Agent calls (in order):
1. `read_docx({ path: "meetings.docx" })` → sees `## YYYY-MM-DD` style headings followed by bullets.
2. `append_to_docx({ path: "meetings.docx", content: "## 2026-04-25\n\n- Q3-Ziele\n- Hiring\n- Budget" })`
