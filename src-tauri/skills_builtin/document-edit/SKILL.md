---
name: document-edit
title: Dokument bearbeiten
description: Make edits to a Markdown or plain-text file in the agent's folder — fix a typo, restructure a paragraph, replace a section. The user sees a line-by-line diff before anything is written. Works on `.md`, `.markdown`, `.txt`, `.text` only; use `document-create-docx` or `document-extend` for `.docx`.
icon: FilePen
tools:
  - list_folder
  - read_file
  - rewrite_file
  - ask_user
hitl:
  default: true
language: en
---

You can edit Markdown or plain-text files in the agent's folder. The mechanism is "rewrite the entire file" — but the user only sees the line-level diff, so trivial edits stay trivial-looking.

Workflow — follow it in this order, every time:

1. If the target file is unclear, run `list_folder` and pick the right one.
2. **You MUST call `read_file` on the target file before `rewrite_file`.** This is non-negotiable. You need the exact current contents to make a minimal, targeted edit; without it you would have to invent the surrounding text and the diff would look enormous.
3. Apply the user's requested change in your head: take the file content from step 2, make only the change requested, leave everything else byte-for-byte identical (including blank lines, trailing whitespace, heading hierarchy).
4. Call `rewrite_file` with the path and the **complete new content** of the file. Do not send a diff or a snippet — `rewrite_file` always replaces the entire file. The HitlCard will show the user a line-by-line diff so they can verify the change is minimal.
5. After the rewrite goes through, confirm in one sentence what changed and where (cite the file path verbatim). If the user rejects, the file is untouched — ask what to change.

Anti-patterns — never do these:
- Sending only the changed lines as `content` (would delete the rest of the file).
- Re-formatting the whole file "for consistency" while making the requested edit (the diff will look huge and the user will reject).
- Calling `rewrite_file` without first calling `read_file`.

Example:

User: "Im Journal das Wort „HITL" durch „Human-in-the-Loop" ersetzen, sonst nichts ändern."

Agent calls (in order):
1. `read_file({ path: "journal.md" })` → returns the full current content.
2. `rewrite_file({ path: "journal.md", content: <full content with HITL → Human-in-the-Loop> })`

The HitlCard shows a diff of two lines (one removed, one added). The user approves; the rest of the file is untouched.
