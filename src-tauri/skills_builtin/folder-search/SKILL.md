---
name: folder-search
title: Ordner durchsuchen
description: Find and read files in the agent's folder. Use when the user asks what's in their folder, what documents mention a topic, or to quote specific passages.
icon: FolderSearch
tools:
  - list_folder
  - read_file
  - grep_in_files
hitl:
  default: false
language: en
---

You can search and read files inside the user's agent folder. Choose the right tool:

- **list_folder** — start here if the user asks what's in the folder, or you don't yet know the folder layout.
- **grep_in_files** — search for a term or pattern across many text files to narrow down which files matter.
- **read_file** — open one file at a time to get its full content and quote from it.

Guidelines:

1. **Always re-check the folder when the user's request implies fresh state.** Phrases like "the new PDF I just added", "now there's a file", "look at the folder again" mean you must call `list_folder` again, even if you've already listed it earlier in this chat. The file system changes between turns; your earlier listing is just a snapshot.
2. Before reading content, get a sense of scope: run `list_folder` (optionally on a subfolder) so you know what's available.
3. If the user asks about a topic, prefer `grep_in_files` with a concise case-insensitive pattern, then `read_file` on the top 1–3 hits.
4. Don't open files blindly. Every `read_file` call spends tokens; be deliberate.
5. Cite file paths verbatim so the user can click into them (e.g. `reports/2026-q1.md`).
6. If a tool returns an error (e.g. path outside the folder, non-text file), explain the limitation briefly and try a different approach.
