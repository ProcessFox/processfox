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
hitl:
  default: true
language: en
---

# Skill: Vorlage befüllen

## Purpose
Produce a filled `.docx` from a template that already lives in the agent's folder. Templates use double-brace placeholders such as `{{customer_name}}`, `{{quote_amount}}`, `{{deadline}}`. The template itself is never modified — the skill only writes a new file with the substitutions applied.

## When to Use
- User asks for a repetitive deliverable (offer, contract, status report, letter) and a template-style `.docx` exists in the agent folder.
- User explicitly references a "Vorlage" / "template" / "Muster".

## How to Use
1. If the user did not name the template explicitly, run `list_folder` and pick a `.docx` whose name suggests "template", "vorlage", "muster" or similar. If unclear, use `ask_user` to confirm.
2. **Always call `read_docx` on the template before `write_docx_from_template`.** Reading is the only way to see which placeholder keys actually exist (`{{customer}}` vs `{{customer_name}}`) and what context surrounds them. Inventing keys leads to leftover `{{…}}` tokens in the output.
3. Match the user's input to the placeholders found. For each missing field, call `ask_user` with a question that names the field. Don't guess.
4. Pick an output path that doesn't clash with the template (`offer-template.docx` → `offer-max-mustermann-2026-04-26.docx`). Default to a kebab-case name with the date so the user's folder stays scannable.
5. Call `write_docx_from_template` with `templatePath`, `outputPath`, and `replacements` as a flat key→value object. The HITL preview shows a `key | value` table plus any placeholders the template still has that you didn't fill.
6. After the write, confirm in one sentence which output file was created and from which template (cite both paths verbatim).

## HITL Behavior
Default: true. The user sees a preview of all key→value substitutions and a list of any unfilled placeholders before the new file is written. Approve writes it; reject leaves nothing behind.

## Example Interactions

### Example 1 — offer from a short brief
User: "Mach mir ein Angebot für Max Mustermann über 1500€ Beratung."
Plan:
- `list_folder` → finds `angebot-vorlage.docx`.
- `read_docx` → placeholders: `{{customer_name}}`, `{{amount}}`, `{{deadline}}`, `{{contact_email}}`.
- `ask_user` for missing `deadline` and `contact_email`.
- `write_docx_from_template` → `angebot-max-mustermann-2026-04-29.docx`.

### Example 2 — explicit template reference
User: "Nimm die NDA-Vorlage und fülle sie für die Firma Müller GmbH aus."
Plan:
- `read_docx` on `nda-vorlage.docx`.
- Fill what the user provided; `ask_user` for the rest.
- Write to `nda-mueller-gmbh-2026-04-29.docx`.

## Anti-Patterns
- Don't skip `read_docx` and try to guess placeholder names — broken placeholders survive into the output.
- Don't overwrite the template. The output path must differ.
- Don't silently invent values for missing fields.
- If `write_docx_from_template` reports unsubstituted placeholders due to the split-across-runs problem (Word splits placeholders internally when formatting changes mid-placeholder), explain to the user that the affected placeholders need to be re-typed in the template as plain text. Don't try to work around it by retrying with different keys.

## Notes for Maintainers
- Placeholder syntax: `{{snake_case_key}}`. Substitution is plain text replacement at the run level — no conditionals, no loops in v1.
- Implementation: `core/tool/tools/write_docx_from_template.rs`. Uses `docx-rs` to walk runs and detect split placeholders.
