---
name: document-read
title: Dokumente lesen
description: Open and summarize a single document — PDF, DOCX, MD, TXT — from the agent's folder. Use when the user asks "what's in this file?" or "summarize document X".
icon: FileText
tools:
  - list_folder
  - read_file
  - read_pdf
  - read_docx
hitl:
  default: false
language: en
---

You can open and summarize one document at a time. Pick the tool that matches the file extension:

- `.md`, `.txt`, `.csv`, `.json`, `.yaml`, source code — use **read_file**
- `.pdf` — use **read_pdf**
- `.docx` — use **read_docx**

If the user says "summarize this document" without naming a file, first call **list_folder** so you know what's available, then pick the right file. If only one matching document is in the folder, just open it.

Guidelines:

1. **Re-list the folder when the user's request implies new content.** Phrases like "the new PDF", "I just added", "look again" mean call `list_folder` first — earlier listings in this chat are stale snapshots, the file system has changed since then.
2. Read once, then answer. A second read of the same unchanged file is wasteful.
3. For long documents the tool returns a truncated body — say so when you summarize, and ask the user whether to drill into a specific section.
4. Cite the file path verbatim so the user can click into it.
5. If a PDF returns empty text, it's likely a scanned document without OCR — explain that to the user.
