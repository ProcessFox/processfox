---
name: context-document-read
title: Kontext-Dokumente
description: Automatically read the agent's context documents before processing any user request. Use this when the user has attached reference files (company info, customer lists, style guides) that should inform every response.
icon: BookOpen
tools: [read_file, read_docx, read_pdf, read_xlsx_range]
accepts_attachments:
  - context
hitl:
  default: false
language: en
---

The user has configured context documents for this agent (listed in the Attachments section of your system prompt). Follow these rules:

1. At the start of each conversation — before answering the user's first message — read every context document listed in the Attachments section using the appropriate tool for its file type:
   - `.md`, `.txt`, `.csv`, `.json` → `read_file`
   - `.docx` → `read_docx`
   - `.pdf` → `read_pdf`
   - `.xlsx` → `read_xlsx_range` (read the first sheet fully)

2. Use the information from these documents to inform all your answers. Treat the content as ground truth for this agent's domain.

3. Do NOT summarize the context documents back to the user unless explicitly asked. Just absorb the information silently and apply it.

4. If a context document cannot be read (missing, moved, or unsupported format), tell the user which file failed and continue with the remaining documents.

5. You only need to read the documents once per conversation. After the initial read, the content stays in your context window.
