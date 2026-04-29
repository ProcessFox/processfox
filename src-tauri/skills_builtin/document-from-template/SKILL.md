---
name: document-from-template
title: Vorlage befüllen
description: Generate a new Word (.docx) document from an existing template by filling its `{{placeholder}}` slots. Useful for offers, contracts, letters, anything where the structure stays the same and only specific values change. The original template is never modified; the user approves every fill before the new file is written.
icon: FileStack
tools:
  - list_folder
  - read_docx
  - write_docx_from_template
  - ask_user
accepts_attachments:
  - template
hitl:
  default: true
language: en
---

You can produce a filled `.docx` from a template that already lives in the agent's folder. Templates use double-brace placeholders such as `{{customer_name}}`, `{{quote_amount}}`, `{{deadline}}`. Use this for repetitive deliverables: offer letters, contracts, status reports.

Workflow — follow it in this order, every time:

1. **Pick the template.** If the user attached one (you'll see it noted under `## Attachments` in this prompt), use that path verbatim and skip discovery. Otherwise: if the user named the template explicitly, use that. Otherwise run `list_folder` and pick a `.docx` whose name suggests "template", "vorlage", "muster" or similar. If still unclear, use `ask_user` to confirm.
2. **You MUST call `read_docx` on the template before `write_docx_from_template`.** This is non-negotiable: only by reading do you see which placeholder keys actually exist (e.g. `{{customer}}` vs `{{customer_name}}`) and what context surrounds them. Inventing keys leads to leftover `{{…}}` tokens in the output.
3. Match the user's input to the placeholders you found. If the user only gave partial info — e.g. they said "Angebot für Max Mustermann über 1500€" but the template also has `{{deadline}}` and `{{contact_email}}` — call `ask_user` for each missing field separately, with a question that mentions the field name. Don't guess.
4. Pick an output path that doesn't clash with the template (`offer-template.docx` → `offer-max-mustermann-2026-04-26.docx`). Default to a kebab-case name with the date so the user's folder stays scannable.
5. Call `write_docx_from_template` with `templatePath`, `outputPath`, and `replacements` as a flat key→value object. The user sees a preview table (key | value) plus any placeholders the template still has that you didn't fill. Approve writes the new file; reject leaves nothing behind.
6. After the write, confirm in one sentence which output file was created and from which template (cite the paths verbatim).

If the tool reports that some placeholders were not substituted (split-across-runs problem), explain to the user that those placeholders need to be re-typed in the template as plain text — Word splits them internally when formatting changes mid-placeholder. Do NOT try to work around it by retrying with different keys.

Example:

User pastes the email:
> Hallo, bitte erstellt mir ein Angebot für Max Mustermann GmbH über 12.500 €,
> Lieferzeit Ende Mai, Kontakt max@mustermann.de.

Agent calls (in order):
1. `list_folder({ path: "" })` → sees `offer-template.docx`.
2. `read_docx({ path: "offer-template.docx" })` → reads the template, sees placeholders `{{customer}}`, `{{amount}}`, `{{deadline}}`, `{{contact_email}}`.
3. `write_docx_from_template({ templatePath: "offer-template.docx", outputPath: "angebot-max-mustermann-2026-04-26.docx", replacements: { customer: "Max Mustermann GmbH", amount: "12.500 €", deadline: "Ende Mai 2026", contact_email: "max@mustermann.de" } })`

The HitlCard shows the four substitutions; the user approves; a new docx is written. The template stays untouched.

Respond in the language the user used.
