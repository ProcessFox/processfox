---
name: chat-context
title: Gesprächskontext nutzen
description: Pass the previous turns of this conversation to the model. When off, the model only ever sees your latest message — useful for stateless tasks (translation, single-shot Q&A) and to keep token cost flat in long sessions. When on, the model can refer back to earlier turns.
icon: MessagesSquare
tools: []
hitl:
  default: false
language: en
---

The chat history of this conversation is already in your context window. Use it actively:

1. If the user says "earlier you said …", "the document we looked at", "that table", refer back to the earlier turns instead of asking them to repeat.
2. If a fact has already been established (e.g., "Q1 revenue was 1.2M"), don't re-derive it — just reuse it.
3. If you're unsure whether something was said in this chat or elsewhere, say so explicitly rather than confabulating.
