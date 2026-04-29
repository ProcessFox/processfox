# ProcessFox — Roadmap

Dieses Dokument bricht den Weg zu v1.0 in sechs Phasen herunter. Jede Phase endet mit einem funktionsfähigen, testbaren Zwischenstand. Nach jeder Phase wird in `main` gemerged.

**Aktueller Stand (2026-04-29):** Phasen 1–4 abgeschlossen, Phase 5 weit fortgeschritten, Phase 6 noch nicht begonnen.

## Phase 1 — Gerüst (1–2 Wochen) ✅ abgeschlossen

**Ziel:** Die App startet, zeigt die UI-Struktur, aber kann noch nichts "Echtes".

### Arbeit
- [x] Tauri v2 Projekt initialisieren (`npm create tauri-app@latest` mit React + TS + Vite)
- [x] Basis-Layout: dreispaltig (Sidebar, optionaler Preview, Chat-Bereich) mit resizable Panels
- [x] Agent-Dropdown in Sidebar oben (statisch mit Mock-Daten)
- [x] Datei-Baum (`react-arborist`) mit Mock-Inhalt
- [x] Leerer Chat-Bereich mit Textarea unten und "Senden"-Button
- [x] Rust: Datenmodelle für `Agent`, `Message`, `Skill`, `ToolSchema`
- [x] Rust: CRUD-Commands für Agenten, Persistenz in `<app-support>/agents/`
- [x] Rust: `core::storage` mit plattform-spezifischen Pfaden
- [x] Settings-Modal-Shell mit Tabs (leer): "Modelle", "Cloud-APIs", "Sprache", "Über"
- [x] Agent-Editor als Modal: Name, Ordner (File-Picker via Tauri-Dialog), System-Prompt, leere Skill-Liste
- [x] Tailwind + shadcn/ui Setup mit Basis-Theme (Hell & Dunkel, System-Default)
- [x] GitHub Actions: "CI" Workflow (Rust + Frontend Build-Check, keine Releases)

### Akzeptanzkriterien
- App startet auf macOS, Windows, Linux (dev-mode mindestens auf macOS und einer zweiten Plattform getestet).
- Nutzer kann einen Agenten anlegen und sieht dessen Ordner-Inhalt im Baum.
- Datei-Klick im Baum zeigt Dateinamen im Mittelbereich (noch keine Preview).
- Chat-Textarea akzeptiert Eingaben, "Senden" zeigt die Nachricht als User-Bubble an.
- Settings-Modal öffnet und schließt sauber.

## Phase 2 — LLM-Anbindung (1–2 Wochen) ✅ abgeschlossen

**Ziel:** Chat funktioniert mit lokalem GGUF-Modell, noch ohne Skills.

> Hinweis: Erst-Implementierung mit `mistral.rs`. In Phase 3 auf `llama-cpp-2` migriert, weil `mistral.rs` Gemma 4 nicht laden konnte (siehe `docs/architecture.md` §7).

### Arbeit
- [x] Benchmark `candle` vs. `mistral.rs` mit Gemma 4 E4B und einem Referenz-Prompt — Ergebnisse in `benchmarks/`
- [x] Entscheidung und Implementierung `LocalGgufProvider` — Erst `mistral.rs`, in Phase 3 ersetzt durch `llama-cpp-2`
- [x] Trait `LlmProvider` mit einheitlichem Streaming-Event-Format
- [x] `AnthropicProvider`, `OpenAiProvider`, `OpenRouterProvider` (Cloud-optional) — plus zusätzlicher generischer `OpenAiCompatProvider`
- [x] API-Key-Storage via Tauri Stronghold oder `keyring` — implementiert via `keyring`
- [x] Modell-Download-Flow: HuggingFace-URL, GGUF-Validierung, Progress-Bar, Speicherung
- [x] Kurator-JSON `models/catalog.json` im Repo mit initial 3–5 empfohlenen Modellen — liegt unter `src-tauri/resources/catalog.json`
- [x] Settings-Tab "Modelle": Katalog-Dropdown + Custom-URL + Download + Liste geladener Modelle
- [x] First-Run-Detection: Settings-Modal öffnet sich automatisch beim ersten Start — gelöst über den Welcome-Flow (`views/Welcome.tsx`)
- [x] Hardware-Check (RAM-Detection, einfache VRAM-Heuristik) mit Modell-Vorschlag
- [x] Chat sendet Nachrichten an aktiven Provider, streamt Antworten in UI
- [x] Chat-Verlauf persistieren (`<uuid>.chat.jsonl`)

### Akzeptanzkriterien
- Nutzer kann im Settings-Modal ein GGUF-Modell herunterladen.
- "Hallo, wer bist du?" vom User wird vom Modell beantwortet, Antwort streamt live in den Chat.
- Chat-Verlauf wird persistiert und beim Neustart wiederhergestellt.
- Cloud-Provider (mindestens Anthropic) funktioniert alternativ, wenn API-Key hinterlegt.
- Modell-Wechsel im laufenden Betrieb funktioniert (Entladen + Neuladen).

## Phase 3 — Tool-System + lesende Skills (1–2 Wochen) ✅ abgeschlossen

**Ziel:** Agent kann Dateien lesen und Antworten darauf basieren.

### Arbeit
- [x] `trait Tool` + `ToolRegistry` + JSON-Schema-Export für LLM-Function-Calling
- [x] `core::sandbox::ensure_in_agent_folder` + Unit-Tests (Symlink-Escape, Path-Traversal)
- [x] Tools: `list_folder`, `read_file`, `grep_in_files`, `read_pdf`, `read_docx`, `read_xlsx_range`
- [x] Skill-Loader: scannt `skills_builtin/`, parst SKILL.md, baut `SkillRegistry`
- [x] Prompt-Composer: baut System-Prompt aus Skill-Descriptions + Agent-SystemPrompt
- [x] Skill: `folder-search` (siehe `docs/skills/folder-search.md`)
- [x] Skill: `document-read`
- [x] Skill: `table-read`
- [x] Skill: `chat-context`
- [ ] ~~Skill: `context-document-read`~~ — verworfen, Funktion deckt `chat-context` mit ab
- [x] ReAct-Loop-Implementierung mit Max-Iter-Sicherung
- [x] Tool-Call-Chips im Chat (Status: running, done, error)
- [x] Skill-Auswahl im Agent-Editor: Checkbox-Liste aller verfügbaren Skills
- [x] Skill-Icons unter Agent-Namen im UI
- [x] JSON-Cleanup-Layer für Tool-Call-Outputs kleiner Modelle

### Akzeptanzkriterien
- Nutzer erstellt Agenten mit Ordner "~/TestPdfs" und 5 PDFs.
- Aktiviert Skills "folder-search" und "document-read".
- Frage: "Welche Dokumente sprechen über Thema X?" führt zu sichtbaren Tool-Calls (`list_folder`, `read_pdf` ×N, eventuell `grep_in_files`) und liefert eine sinnvolle Antwort mit Datei-Referenzen.
- Datei-Preview im Chat per Klick auf referenzierte Datei öffnet sie in der mittleren Spalte.
- Sandbox-Verletzung (Versuch, außerhalb des Agent-Ordners zu lesen) wird als Fehler-Chip angezeigt, Loop bricht sauber ab.

## Phase 4 — Schreibende Skills + HITL (1–2 Wochen) ✅ abgeschlossen

**Ziel:** Agent kann Dateien erzeugen und ändern, mit Inline-Freigabe.

### Arbeit
- [x] Tools: `write_docx`, `write_docx_from_template`, `append_to_md`, `update_xlsx_cell`, `ask_user` — zusätzlich `write_xlsx`, `append_to_docx`, `rewrite_file`
- [ ] ~~Tool: `llm_extract_structured`~~ — verworfen, strukturierte Extraktion läuft direkt über die Tool-Call-Argumente des Modells
- [x] HITL-Mechanik: Tool kann eine Freigabe anfordern, ReAct-Loop pausiert
- [x] Frontend: `HitlCard`-Komponente mit Diff-Darstellung
  - [x] Datei-Erstellung: volle Inhalt-Vorschau
  - [x] Datei-Bearbeitung: Zeilen-Diff (grün/rot)
  - [x] XLSX-Update: Liste der geplanten Zellen-Änderungen
- [x] HITL-Flags in SKILL.md-Frontmatter umsetzen, pro-Agent-Override im Agent-Editor
- [x] Skill: `document-create-docx`
- [x] Skill: `document-edit`
- [x] Skill: `document-extend`
- [x] Skill: `table-update` — plus `table-create` als eigener Skill
- [x] Template-Handling: Nutzer kann in Agent-Ordner `.docx`-Templates ablegen, Skill findet sie und nutzt Platzhalter — eigener Skill `document-from-template`
- [x] Tauri-File-Watcher: Datei-Baum aktualisiert sich live bei Änderungen im Agent-Ordner

### Akzeptanzkriterien
- Referenz-Use-Case "E-Mail → Angebot" läuft: Nutzer paste-t E-Mail, Agent nutzt Template, füllt Felder, zeigt Preview, User gibt frei, DOCX wird geschrieben.
- Referenz-Use-Case "Excel-Lücken füllen": Agent identifiziert leere Zellen, schlägt Werte vor, zeigt Diff-Karte pro Zelle oder gebündelt.
- Ablehnung der HITL-Karte führt zu "ich habe nichts geändert"-Antwort des Agenten und Fortsetzung des Dialogs.
- Bei aktivierter "ohne Rückfrage"-Variante läuft die Aktion direkt durch, Ergebnis wird prominent bestätigt.

## Phase 5 — Polish & Onboarding (1 Woche) 🚧 in Arbeit

**Ziel:** Die App fühlt sich fertig an.

### Arbeit
- [x] First-Run-Flow: Willkommen → Modell-Download → erster Agent → Tutorial-Chips — `views/Welcome.tsx` (drei Schritte)
- [x] Starter-Chips im leeren Chat ("Probier mal: ...") — `lib/starterPrompts.ts`, gerendert in `ChatPane`
- [ ] Skill-Editor-UI für User-erstellte Skills (Formular, kein Markdown-Editor)
- [x] Tastatur-Shortcuts: Cmd/Ctrl+N (Neuer Agent), Cmd/Ctrl+, (Settings), Cmd/Ctrl+Enter (Senden)
- [x] Fehler-Toasts mit "Logs öffnen"-Button — sonner-Toasts; "Logs öffnen" via `commands/file.rs` (`reveal_logs`)
- [x] Onboarding-Banner: "Für bessere deutsche Qualität: Modell XY empfohlen" — Hardware-Empfehlung im Modelle-Tab
- [ ] Modell-Empfehlungs-Mitteilung, wenn aktives Modell veraltet
- [x] Drag-and-Drop von Dateien in den Chat (erzeugt einen Inline-Verweis, der den Agent auf die Datei fokussiert) — Window-weiter Drop-Listener in `App.tsx`
- [x] Copy-Button für Agent-Antworten — `ChatPane.tsx` (`navigator.clipboard.writeText`)
- [ ] Diverse Usability-Tests (subjektiv mit 2–3 Testpersonen durchspielen) — laufend (siehe Commits "Usability changes")

### Akzeptanzkriterien
- Erfolgs-Kriterium: ein Einsteiger kann ≤ 5 Minuten nach Installation (inkl. Download eines kleinen Modells) seine erste Frage beantwortet bekommen.
- Alle drei Referenz-Use-Cases sind mit Gemma 4 E4B lokal reproduzierbar.
- Kein Absturz in typischen Nutzungs-Pfaden; nicht-reproduzierbare Bugs sind dokumentiert.

## Phase 6 — Release (1 Woche) ⏳ noch nicht begonnen

**Ziel:** v1.0.0 auf GitHub Releases, Mac/Win/Linux installierbar.

> Aktuell läuft nur `ci.yml` (Build-Check). Version steht noch auf `0.1.0`.

### Arbeit
- [ ] GitHub Actions `release.yml`: Build-Matrix (macOS, Windows, Linux), Tauri-Bundler
- [ ] Release auf Tag-Push (`v*.*.*`) getriggert
- [ ] Tauri-Updater-Konfiguration (Public-Key im Repo, Signing-Key als GitHub Secret)
- [ ] Release-Notes-Template
- [ ] README mit Download-Links, Screenshots, Quickstart-Anleitung
- [ ] Bekannte Sicherheits-Warnungs-Hinweise dokumentieren (weil noch kein Code-Signing)
- [ ] Post-Release: Issue-Templates, CONTRIBUTING.md, Bug-Report-Template
- [ ] Beta-Tester anschreiben (3–5 Personen aus Netzwerk), Feedback-Kanal (Issues oder Discord)

### Akzeptanzkriterien
- Tag `v1.0.0` triggert Build, Release enthält Artefakte für alle drei Plattformen.
- Installation auf einem jungfräulichen Test-Rechner (VM) führt zur funktionsfähigen App.
- Auto-Updater findet den nächsten Release (getestet mit Point-Release `v1.0.1`).
- Mindestens ein externer Beta-Tester hat erfolgreich einen der drei Referenz-Use-Cases durchgespielt.

## Nach v1.0

Mögliche v1.1+ Themen — Priorität wird nach v1.0-Feedback entschieden:
- Code-Signing (Apple Developer, Windows-EV-Zertifikat)
- Web-Skills (HTTP-Fetch, Suchmaschinen-Integration)
- Skill-Marketplace (Public Index von Community-Skills)
- Englische UI
- Audio-Transkription via Whisper
- OCR auf gescannten PDFs
- Multi-Agenten-Kollaboration
- Auto-Komprimierung langer Chat-Verläufe
- Mobile/Tablet-Variante (iPadOS via Tauri Mobile später)
