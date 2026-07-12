# CLAUDE.md — Arbeits-Anweisungen für Claude Code

Dieses Dokument richtet sich an Claude Code (und an alle anderen LLM-gestützten Codier-Assistenten), die an ProcessFox mitarbeiten. Es fasst Projekt-Kontext, Tech-Stack, Code-Stil-Regeln und Architektur-Prinzipien so zusammen, dass Entscheidungen konsistent mit der Produkt-Vision bleiben.

**Pflicht-Lektüre vor jedem größeren Task:**
- [`CONCEPT.md`](CONCEPT.md) — vollständige Produkt-Vision und Architektur (bleibt im App-Repo, intern)
- Architektur, Roadmap, Skill-Dokus und LLM-Kompatibilität leben in einem separaten Doku-Repo (Astro-/Starlight-Projekt für `www.processfox.ai`), das **nicht** mehr im App-Repo liegt. Lies die öffentliche Version unter `https://www.processfox.ai/docs/`:
  - `https://www.processfox.ai/docs/entwickler/architektur/` — technische Architekturskizze
  - `https://www.processfox.ai/docs/entwickler/roadmap/` — aktuelle Phase
  - `https://www.processfox.ai/docs/entwickler/eigenen-skill-bauen/` — SKILL.md-Template + Konventionen
  - `https://www.processfox.ai/docs/modelle/kompatibilitaet/` — welche lokalen Modelle ProcessFox laden kann (Format, Architektur, Chat-Template, Tool-Calling). Konsultieren, bevor du Modelle in den Catalog aufnimmst oder Custom-URL-Empfehlungen formulierst.
  - `https://www.processfox.ai/docs/skills/<skill>/` — wenn du an einem konkreten Skill arbeitest
- Doku-Synchronisierung ist Owner-Aufgabe: Code-Änderungen, die das Verhalten der App ändern (neue Skills, geänderte Tools, Roadmap-Sprünge), bitte im PR-Body erwähnen, damit der Owner die Doku im separaten Repo nachzieht.

## 1. Projekt-Kurzprofil

- **Produkt:** ProcessFox — Desktop-App für lokale KI-Agenten, Zielgruppe Einsteiger (kleine Unternehmen, NGOs).
- **Framework:** Tauri v2.
- **Frontend:** React 19 + Vite + TypeScript + Tailwind + shadcn/ui.
- **Backend:** Rust (pure Rust, keine Python-Abhängigkeit).
- **LLM-Runtime:** `llama-cpp-2` (in Phase 3 von mistral.rs migriert, weil dessen GGUF-Loader Gemma 4 nicht kannte). Nutzt `apply_chat_template_oaicompat` + `streaming_state_oaicompat` für native Tool-Calling- und Reasoning-Extraktion. Cloud-Provider parallel via separate Implementierungen.
- **Distribution:** GitHub Releases (kostenlos, ohne Auto-Updater) + Apple App Store / Microsoft Store (Einmalzahlung, Auto-Update via Store). Kein eigener Tauri-Updater.
- **Lizenz:** Dual-Licensing — GPL v3 (Community Edition, GitHub) + proprietäre Store-Lizenz. Gesteuert über `PROCESSFOX_EDITION` Env-Variable (`community` | `store`), ausgewertet in `build.rs` als `cfg(edition_store)`. Laufzeit-Abfrage über `core::license::Edition::current()`.

## 2. Goldene Regeln

1. **Einsteiger-Fokus schlägt Feature-Fülle.** Wenn eine Entscheidung zwischen "mehr können" und "einfacher bedienen" steht, gewinnt immer einfacher. Bei Zweifeln: zurück zu `CONCEPT.md` §3 "Produkt-Prinzipien".
2. **Agent > Thread.** Es gibt keine Chat-History-Sidebar. Alles passiert in benannten Agenten. Wer eine Thread-UI vorschlägt, liegt falsch.
3. **Skills sind atomar.** Ein Skill tut eine Sache und kombiniert dafür Tools. Keine Meta-Skills, keine Workflow-Skills.
4. **Ordner-Sandbox ist nicht verhandelbar.** Jeder Tool-Call MUSS im Backend prüfen, dass der Pfad im Agent-Ordner liegt. Kein Verlass auf LLM-Disziplin.
5. **HITL ist Default für Schreibaktionen.** Ausnahme nur, wenn der Skill bewusst auf "ohne Rückfrage" konfiguriert ist.
6. **Keine Python-Abhängigkeit in v1.** Alles in Rust. Wenn du einen Python-Subprozess vorschlägst, stimmt etwas nicht.
7. **Lokal zuerst.** Cloud-APIs sind Optionen, nicht die Haupt-Codepfad.
8. **Kein User-Script in der Sandbox in v1.** Die Sandbox-Infrastruktur wird gebaut, aber nur für eingebaute Skills.

## 3. Code-Stil-Regeln

### Rust
- Rust 2021 Edition, `cargo fmt` vor jedem Commit, `cargo clippy -- -D warnings` muss grün sein.
- **Fehler-Handling:** `thiserror` für Library-Crates, `anyhow` nur in Tauri-Commands. Keine `unwrap()` in Production-Code — immer `?` oder sinnvolles Fallback.
- **Async:** `tokio` (Tauri bringt es mit). Blockierende Operationen (Datei-IO, LLM-Inferenz) immer in `spawn_blocking` oder eigenem Thread.
- **Module-Layout:** Ein Tauri-Command pro Feature-Datei, gruppiert unter `src-tauri/src/commands/`.
- **Sicherheit:** Jeder File-Path, der aus dem Frontend kommt, wird gegen den Agent-Ordner normalisiert und geprüft. Nutze eine zentrale Funktion `ensure_in_agent_folder(agent_id, path) -> Result<PathBuf>`.
- **Serialisierung:** `serde` mit expliziten Feldnamen (`#[serde(rename_all = "camelCase")]` zur TypeScript-Seite hin).

### TypeScript / React
- TypeScript strict-mode an.
- Funktionale Komponenten, Hooks, keine Klassen-Komponenten.
- Datenfluss: **Zustand möglichst im Rust-Backend.** Frontend holt per `invoke()` und cached lokal via `react-query` oder simplem State.
- **Keine State-Management-Library** wie Redux/Zustand in v1 nötig — Props + Context reichen bei unserer Größe.
- **Styling:** Tailwind Utility-Klassen, keine Inline-Styles, keine separaten CSS-Dateien außer `globals.css`.
- **Datei-Organisation:** `src/components/`, `src/views/`, `src/hooks/`, `src/lib/` (für Rust-Bridge-Wrapper), `src/types/` (für geteilte TS-Typen).
- **Kommentare:** Englisch im Code (Kommentare, Variablen-Namen). UI-Strings und Doku-Markdown auf Deutsch (siehe §8).

## 4. Verzeichnis-Layout (Ist-Stand)

```
processfox/
├── README.md
├── CONCEPT.md
├── CLAUDE.md                       # dieses Dokument
├── CHANGELOG.md
├── LICENSE                         # GPL v3 (Community Edition)
├── LICENSE-STORE                   # proprietäre Store-Lizenz
├── .gitignore
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.js
├── components.json                 # shadcn/ui-Config
├── index.html
├── src/                            # Frontend (React + TS)
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── agent/                  # AgentSwitcher, AgentEditorDialog
│   │   ├── chat/                   # ChatPane, ChatInput, HitlCard, AskUserCard,
│   │   │                           # ToolCallChip, ReasoningChip, MessageMarkdown
│   │   ├── filetree/               # FileTree (react-arborist)
│   │   ├── preview/                # Format-Viewer/-Editoren: PreviewPane, PreviewHeader,
│   │   │                           # DocxViewer, PptxViewer, XlsxViewer, PdfViewer,
│   │   │                           # ImageViewer, MarkdownEditor, TextEditor, UnsupportedViewer
│   │   ├── settings/               # ModelsTab, CloudApisTab
│   │   ├── theme-provider.tsx
│   │   └── ui/                     # shadcn-Bausteine
│   ├── views/
│   │   ├── Main.tsx
│   │   ├── Settings.tsx
│   │   └── Welcome.tsx
│   ├── hooks/                      # useAgentChat etc.
│   ├── lib/
│   │   ├── tauri.ts                # typed invoke() wrappers + event subs
│   │   ├── i18n.ts                 # i18next-Setup
│   │   ├── fileIcons.ts
│   │   ├── toolIcons.ts
│   │   ├── starterPrompts.ts
│   │   └── utils.ts
│   ├── locales/                    # i18next-Übersetzungen: de/en/es/fr/it/pl
│   └── types/                      # ChatMessage, Agent, Skill, …
├── src-tauri/                      # Backend (Rust)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs                    # liest PROCESSFOX_EDITION → cfg(edition_store)
│   ├── icons/                      # tatsächliche Bundle-Icons (macOS/Windows/Android/iOS)
│   ├── skills_builtin/             # eingebaute Skills (include_dir!-eingebunden)
│   │   ├── folder-search/
│   │   │   └── SKILL.md
│   │   ├── document-read/
│   │   ├── document-extend/
│   │   ├── document-create-docx/
│   │   ├── document-edit/
│   │   ├── document-from-template/
│   │   ├── table-read/
│   │   ├── table-update/
│   │   ├── table-create/
│   │   ├── chat-context/
│   │   └── context-document-read/
│   └── src/
│       ├── main.rs
│       ├── lib.rs                  # tauri::Builder, generate_handler!
│       ├── state.rs                # AppState (Clone, interne Arc<OnceLock<…>>)
│       ├── commands/               # Ein Tauri-Command-Modul pro Feature
│       │   ├── agent.rs
│       │   ├── chat.rs
│       │   ├── file.rs             # list/watch/unwatch agent folder
│       │   ├── mod.rs
│       │   ├── models.rs
│       │   ├── preview.rs          # docx/pptx/xlsx-Preview-Generierung
│       │   ├── secrets.rs
│       │   ├── settings.rs
│       │   └── skill.rs
│       └── core/
│           ├── agent.rs            # Agent-Datenmodell, Persistenz
│           ├── chat/               # ChatRunner, ReAct-Loop, ChatRepo
│           │   ├── run.rs          # ReAct-Orchestrierung + HITL/AskUser-Pipelines
│           │   ├── repo.rs         # JSONL-Persistenz pro Agent
│           │   └── freshness.rs    # FreshnessTracker / Staleness-Hint
│           ├── delegation/         # Stateless LLM-Inferenz für Fan-out-Tools (Background Worker)
│           ├── error.rs            # CoreError + CommandError
│           ├── hardware.rs         # RAM/VRAM-Erkennung
│           ├── license.rs          # Edition-Enum, Edition::current()
│           ├── llm/                # LlmProvider Trait + Implementierungen
│           │   ├── anthropic.rs
│           │   ├── openai.rs
│           │   ├── openai_compat.rs # geteilte HTTP-/Streaming-Basis für OpenAI-kompatible Provider
│           │   ├── openrouter.rs
│           │   ├── cortecs.rs
│           │   ├── custom_openai.rs # nutzerkonfigurierter OpenAI-kompatibler Endpoint (Ollama/vLLM)
│           │   ├── local_gguf.rs   # llama-cpp-2-Wrapper, Idle-Watcher
│           │   ├── json_cleanup.rs
│           │   └── registry.rs
│           ├── models/             # GGUF-Catalog, Download-Runner
│           ├── preview/            # docx/pptx/xlsx-Preview-Erzeugung
│           ├── sandbox.rs          # ensure_in_agent_folder, ensure_inside_sandbox
│           ├── secrets.rs          # keyring-API-Key-Storage
│           ├── settings.rs
│           ├── skill/              # SKILL.md-Parsing, SkillRegistry
│           ├── storage.rs          # AppPaths
│           ├── tool/
│           │   ├── mod.rs          # Tool Trait, HitlPreview, ToolContext
│           │   ├── registry.rs
│           │   └── tools/          # einzelne Tool-Implementierungen
│           │       ├── list_folder.rs / grep_in_files.rs / read_file.rs / read_skill.rs
│           │       ├── read_pdf.rs / read_docx.rs / read_xlsx_range.rs
│           │       ├── append_to_md.rs / append_to_docx.rs / rewrite_file.rs
│           │       ├── write_docx.rs / write_docx_from_template.rs
│           │       ├── write_xlsx.rs / update_xlsx_cell.rs
│           │       ├── delegate_into_xlsx_column.rs   # Fan-out: LLM-Worker pro Zeile
│           │       └── ask_user.rs
│           ├── workspace.rs        # Ordner-Baum fürs System-Prompt (siehe §5)
│           ├── types.rs
│           └── watcher.rs          # notify-debouncer-mini Folder-Watch
├── assets/                         # Nur Icon-Quelldatei (assets/source/) — Bundle-Icons liegen in src-tauri/icons/
├── benchmarks/                     # Historisches Artefakt (Phase 2c LLM-Runtime-Vergleich), kein aktiver Test-Pfad
├── public/                         # statische Assets fürs Vite-Frontend
└── .github/
    └── workflows/
        ├── ci.yml                  # fmt/clippy/cargo test + npm build, auf push/PR gegen main
        ├── release.yml             # Community Edition, workflow_dispatch, GitHub-Draft-Release
        └── release-store.yml       # Store Edition (Apple/Microsoft), workflow_dispatch
```

> **Doku-Repo ist separat.** Architektur, Roadmap, Skill-Dokus und LLM-Kompatibilität liegen in einem eigenen Astro-/Starlight-Repo (veröffentlicht unter `www.processfox.ai/docs/`) und werden vom Owner manuell mit der App synchron gehalten. `CONCEPT.md` bleibt als interne Vision im Repo-Root.

## 5. Wichtige Schnittstellen-Konventionen

### Tauri Commands (Rust → Frontend)

- Jeder Command, der Shared State braucht, nimmt `tauri::State<'_, AppState>` entgegen. `AppState` selbst ist `Clone` mit pro-Feld `Arc<OnceLock<…>>` für lazy-initialisierte Singletons (ChatRunner, DownloadRunner, FolderWatcher, DelegationRunner) — kein globaler Mutex außenrum. Ausnahme: rein zustandslose Commands (z. B. die Keyring-Zugriffe in `secrets.rs`, `get_app_info`) verzichten bewusst auf `AppState`.
- Fehler werden als `Result<T, CommandError>` zurückgegeben, wobei `CommandError` serialisierbar ist und einen `code`, `message`, und optional `details` enthält.
- Lange Operationen (Modell-Download, ReAct-Loop) laufen via Tauri-Events (`app.emit`), nicht als Return-Value.
- Command-Namen in `snake_case` in Rust, TypeScript-Seite wrappt zu `camelCase`.
- **Serde-Falle**: Bei getaggten Enums (wie `RunEvent`, `HitlPreview`) reicht `#[serde(rename_all = "camelCase")]` allein nicht — das renamed nur die Varianten-Tags, nicht die Felder *innerhalb* der Varianten. Immer `rename_all_fields = "camelCase"` zusätzlich setzen, sonst kommt das JSON mit snake_case-Feldnamen am Frontend an.

### Frontend-Bridge (`src/lib/tauri.ts`)

- Zentrale Typ-sichere Wrapper für alle Commands.
- Event-Listener für Live-Updates (Tool-Call-Status, Download-Progress) als Custom Hooks.

### LLM-Runtime-Abstraktion

- Trait `LlmProvider` mit async `generate(request, sink, cancel) -> CoreResult<()>`. Streamt `LlmEvent`s über einen `mpsc::Sender`, respektiert `CancellationToken`.
- Implementierungen: `LocalGgufProvider` (llama-cpp-2), `AnthropicProvider`, `OpenAiProvider`, `OpenRouterProvider`, `CortecsProvider`, `CustomOpenAiProvider` (nutzerkonfigurierter OpenAI-kompatibler Endpoint, z. B. Ollama/vLLM). `OpenAiProvider`, `OpenRouterProvider`, `CortecsProvider` und `CustomOpenAiProvider` teilen sich die HTTP-/Streaming-Basis `OpenAiCompat` (`core/llm/openai_compat.rs`) statt jeweils eigenen Boilerplate zu implementieren — nur `CustomOpenAiProvider` setzt `include_usage = false`, weil manche selbstgehosteten Backends den Parameter ablehnen.
- Einheitliches Event-Format: `TextDelta`, `ReasoningDelta` (für `<|channel>thought` u. ä.), `ToolCall`, `Usage(TokenUsage)`, `Finish { reason }`, `Error { code, message }`.
- `supports_tools()` markiert Provider, die `request.tools` verarbeiten können — der ReAct-Loop reicht Tools nur an Provider, die das bestätigen.
- **Lokales Modell-Lifecycle:** `LocalGgufProvider` hält ein Modell zwischen Generations geladen, entlädt es aber nach 10 min Idle automatisch (Watcher in `ensure_idle_watcher`). Wer den RAM-Bedarf debuggt oder zusätzliche Trigger zum Entladen einbaut (z. B. beim Wechsel auf Cloud-Provider), arbeitet hier — nicht den Watcher umgehen, sondern ergänzen.

### Token-Usage-Logging

- Provider emittieren genau einmal pro Generation ein `LlmEvent::Usage(TokenUsage)`, direkt vor dem terminalen `Finish`. `TokenUsage` führt `input_tokens`, `output_tokens`, `cached_input_tokens` und `cache_creation_input_tokens` (die letzten beiden `Option<u32>`).
- Welcher Provider was füllt:
  - **Anthropic:** alle vier Felder (Cache-Werte stehen im `message_start.usage`-Block, output_tokens kumulativ in `message_delta.usage`).
  - **OpenAI / OpenRouter / Cortecs / Custom (alle OpenAI-kompatibel):** teilen sich `OpenAiCompat` und füllen `input_tokens`, `output_tokens`, `cached_input_tokens` (aus `prompt_tokens_details.cached_tokens`). Voraussetzung: `OpenAiCompat::new(..., include_usage = true)` schickt `stream_options.include_usage`. `CustomOpenAiProvider` setzt das bewusst auf `false`, weil selbstgehostete Backends (Ollama/vLLM u. ä.) den Schlüssel teils ablehnen — für andere Compat-Server bleibt es grundsätzlich opt-in.
  - **Local GGUF:** exakte `input_tokens` (aus `tokens.len()`) und `output_tokens` (aus `n_cur - prompt_len`); Cache-Felder bleiben `None`, solange wir bei jedem Call einen frischen `LlamaContext` bauen.
- Aggregation passiert im `react_loop` (`core/chat/run.rs`): pro Iteration ein `tracing::debug!`-Eintrag, am Ende des Runs ein `tracing::info!("chat run usage", provider, model, iterations, input_tokens, output_tokens, cached_input_tokens, cache_creation_input_tokens)`. Logfile liegt unter `<app-support>/ProcessFox/logs/processfox.log.<datum>`.
- Wenn ein Provider keine Usage liefert (Compat-Backend ohne `include_usage`, abgebrochener Stream), bleibt das `Usage`-Event aus — der Runner darf nicht hängen, und der Run-Total-Log wird übersprungen statt mit Nullen geschrieben.

### Tool-Registry

- Tools sind in einer zentralen Registry registriert (`HashMap<&'static str, Arc<dyn Tool>>`).
- `trait Tool: Send + Sync + Debug { fn name(&self) -> &'static str; fn schema(&self) -> ToolSchema; async fn execute(&self, input, ctx: &ToolContext) -> CoreResult<ToolOutput>; fn requires_approval(&self, input: &JsonValue, ctx: &ToolContext) -> Option<HitlPreview> { None } }`. `requires_approval` ist der zentrale Haken für HITL (§2 Regel 5) — Standard `None` (kein Preview nötig), Schreib-Tools überschreiben es.
- `ToolContext` enthält Agent-ID, Agent-Ordner-Pfad, App-Handle für Events, plus `channel` (Event-Channel-Name), `tool_call_id` und `cancel: CancellationToken` für kooperatives Abbrechen von Fan-out-Tools.

### Skill-Loading

- Beim App-Start wird nur `src-tauri/skills_builtin/` geladen — via `include_dir!` zur Compile-Zeit eingebettet und von `SkillRegistry::load_builtin()` geparst (kein Runtime-Filesystem-Scan).
- Frontmatter wird via `serde_yaml` geparsed (manueller `---`-Split, kein `gray_matter` — die Crate ist keine Dependency).
- Geladen in ein `SkillRegistry`, von dort kann der Agent sie abrufen.
- **Bekannte Lücke:** `<app-support>/skills/user/` wird von `AppPaths::ensure_dirs()` angelegt, aber nirgends gescannt oder in die `SkillRegistry` gemerged — der Ordner ist vorbereitet, aber tot. Wer nutzerdefinierte SKILL.md-Dateien laden will, muss diesen Scan erst bauen; bis dahin nicht als vorhandenes Feature behandeln oder dokumentieren.

### Progressive Skill Disclosure

- **System-Prompt zeigt nur die Frontmatter** (Title + Description) jedes auf einem Agenten aktivierten Skills, nicht den Body. Gerendert von `skills_block` in `core/chat/run.rs` als kompakte Bullet-Liste unter `## Available skills`.
- **Body wird on demand geladen** über das `read_skill`-Tool (`core/tool/tools/read_skill.rs`). Das Tool nimmt `{ skillId }` und gibt den Body zurück; es wird automatisch in die Tool-Schemas aufgenommen, sobald ein Agent mindestens einen Skill aktiv hat (`collect_tool_schemas`).
- **Konsequenzen für SKILL.md-Autor:innen:**
  - Die `description` muss alleine ausreichen, damit der Agent entscheiden kann, ob er das Skill braucht. Action-oriented schreiben, mit konkreten Trigger-Phrasen ("when the user asks …", "use this for …"), nicht Marketing-Text.
  - Der Body muss als isolierter Text funktionieren, der mid-conversation als Tool-Result auftaucht. Keine impliziten Referenzen auf „den vorherigen Block im Prompt", außer auf die immer vorhandenen System-Prompt-Sektionen (`Today is …`, `## Attachments`).
  - Globale Verhaltensregeln (z. B. „Respond in the language the user used") gehören in den System-Prompt-Header (`compose_system_prompt`), nicht in jeden Skill-Body — sonst greifen sie erst, sobald das Skill geladen wurde.
- **Trade-off**, den man kennen muss: der erste Einsatz eines Skills kostet eine zusätzliche ReAct-Iteration (read_skill-Roundtrip). Schwächere lokale Modelle entscheiden manchmal schlecht, *ob* sie ein Skill brauchen — beim Auswählen der Default-Modelle für Local GGUF berücksichtigen.

### Workspace-Orientierung (System-Prompt)

- **Problem:** Bei Fragen über „das Projekt", ein Thema oder einen Zeitraum (z. B. „Was ist im April passiert?") beantwortet das Modell aus der einzelnen Datei, die zufällig schon im Kontext liegt (oft eine referenzierte `CLAUDE.md`/Kontext-Doku), weil es kein Inventar des Agent-Ordners hat und nicht weiß, *dass* weitere Dateien existieren. Verhaltens-Nudges allein greifen bei schwachen lokalen Modellen unzuverlässig — das Wissen muss vorliegen, nicht erfragt werden.
- **Lösung:** `core/workspace.rs` mit `build_workspace_tree(root) -> Option<String>`. `compose_system_prompt` (`core/chat/run.rs`) ruft über `workspace_block(agent)` einen `## Workspace`-Block ab und platziert ihn **nach** dem Agent-Prompt, **vor** `## Available skills` (Orientierung vor Skill-/Tool-Auswahl). Der Block enthält einen kurzen deutschen Verhaltens-Hinweis plus den eingerückten Datei-Baum.
- **Tree-Format:** eingerückter Baum, Ordner zuerst dann alphabetisch, Dateien als `📄 name  (size · YYYY-MM-DD)`. Datum = lokale mtime, Tagesauflösung — **bewusst nicht** das RFC-3339-UTC-Format der Read-Tool-Header (kein Parser hängt daran). Tiefe `MAX_DEPTH = 2` (Root + unmittelbare Kinder von Unterordnern; tiefere Ordner nur als Header ohne Inhalt). Cap `WORKSPACE_MAX_ENTRIES = 40`, danach Marker ohne exakte Restzahl (zweiter Full-Walk wäre die Zahl nicht wert). Symlinks werden komplett übersprungen (escape-/zyklensicher). Junk-Denylist wie `list_folder`.
- **Kosten/Frische:** einmal pro User-Turn gebaut (nicht pro ReAct-Iteration), bounded FS-Walk → vernachlässigbar; Snapshot bleibt pro Turn frisch und ergänzt den `FreshnessTracker`. Token-Kosten ~0,3–1 k pro Turn worst case, über die beiden Konstanten deckelbar.
- **Best-effort:** Jeder IO-Fehler (unlesbarer Unterordner, fehlende Metadata) wird übersprungen statt propagiert — Prompt-Komposition scheitert nie an einem FS-Hänger. Kein Ordner / leer / nur Junk → `None`, dann gar kein `## Workspace`-Header.
- **Bewusster Folge-Schritt (nicht in diesem Change):** Die Walk-Logik dupliziert `list_folder`s Denylist/`human_bytes`/Sortierung. Eine geteilte Primitive ist absichtlich separat gehalten, um den Diff fokussiert zu lassen.
- **Tests:** `core/workspace.rs::tests` deckt ab: kein/leerer/nur-Junk-Ordner → `None`, flache Liste mit Größe + Alpha-Sortierung, Dirs-vor-Files, Tiefe-2-Cap (Level-3-Inhalt ausgelassen), Entry-Cap-Marker, Symlink-Ordner nicht durchlaufen (`#[cfg(unix)]`). Wer die Logik anfasst, hält die 8 Tests grün.

### Datei-Frische / Staleness-Hint

- **Problem:** Der Agent liest `plan.md` per `read_file`, der Inhalt landet als `tool_result` in der History. Aus LLM-Sicht ist das die aktuelle Wahrheit über die Datei. Editiert die Nutzer:in die Datei zwischen den Turns parallel im Editor, antwortet das Modell aus dem Stand-Snapshot weiter — verwirrend und falsch.
- **Lösung:** `core/chat/freshness.rs` mit `FreshnessTracker` als Feld auf `ChatRunner`. Map `(agent_id, kanonischer_pfad) → mtime_zur_lese_zeit`.
- **Recording:** Nach jedem erfolgreichen Content-Read-Tool (`read_file`, `read_docx`, `read_pdf`, `read_xlsx_range` — siehe `is_content_read_tool`) ruft `react_loop` `freshness.record_read(agent.id, abs_path)` auf. Lese-Tools, die *keine* Snapshots etablieren (`list_folder`, `grep_in_files`, `read_skill`), werden bewusst ausgenommen.
- **Hint-Injection:** Vor jeder neuen LLM-Anfrage prüft `react_loop` die Stale-Liste. Wenn nicht leer, wird via `format_freshness_hint` eine einzeilige deutsche Notiz vorne an den letzten User-Turn gehängt — **nur in der In-Memory-`turns`-Liste**, **nicht** in der persistierten JSONL-User-Message. Token-Kost: 0 im Steady-State, ~30–60 Tokens wenn relevant. Format unterscheidet `Modified` (geändert) und `Removed` (gelöscht/umbenannt) für klare LLM-Reaktion.
- **Bewusste Granularitäts-Entscheidung:** Eine Änderung außerhalb des gelesenen Bereichs (z. B. `read_xlsx_range A1:C20`, dann Zelle E50 geändert) wird trotzdem als stale gemeldet. False positive ist günstiger als false negative — das LLM verschwendet einen `read_*`-Roundtrip statt mit veralteten Daten zu antworten. Wenn das in der Praxis als nervig auffällt, kann der Tracker später feiner werden (per-Range-Tracking).
- **mtime im Tool-Result-Header:** Jeder Content-Read-Tool-Output trägt im Header die Quell-mtime: `--- foo.md (1006 bytes, modified 2026-05-02T10:31:00Z) ---` (XLSX: `--- foo.xlsx · sheet='Sheet1' · A1:L25 · modified 2026-05-02T10:31:00Z ---`). Format ist RFC 3339 UTC mit Sekunden-Auflösung, **fix**. Erzeugt von `core/tool/mod.rs::mtime_suffix`, geparst von `core/chat/freshness.rs::parse_mtime_from_header`. Wer das Header-Format der Read-Tools ändert, muss den Parser im Auge behalten.
- **Bootstrap nach Restart:** `react_loop` ruft beim ersten Run pro Agent in diesem Prozess `freshness.bootstrap_from_history(agent.id, history, agent_folder)` auf. Die Methode walkt die persistierte Chat-History, findet alle alten Content-Read-Tool-Calls, parst die mtime aus dem Tool-Result-Header und füllt damit den Tracker — so überleben Stale-Erkennungen einen ProcessFox-Restart. Idempotent per HashSet-Guard. Bei Legacy-Logs ohne mtime im Header fällt der Bootstrap auf die *aktuelle* mtime zurück; Edits *während* der Downtime werden dann nicht erkannt, künftige Edits aber schon.
- **Tests:** `core/chat/freshness.rs::tests` deckt Record/No-Stale, Modification, Removal, Agent-Isolation, nonexistent-path-no-op, mtime-Parser (mit/ohne Timestamp, XLSX-Header-Variante, Garbage), Bootstrap mit precise mtime, Bootstrap-Fallback auf current mtime, Bootstrap-Idempotenz und Bootstrap-skip-non-content-read ab. Wer die Logik anfasst, muss die 13 Tests grün halten.

### Chat-History-Trimming (Boundary-Pflege)

- Der ReAct-Loop schickt bei jedem Request maximal die letzten `HISTORY_WINDOW = 20` Turns. Das stumpfe `drain` reicht **nicht** — es kann mitten in einem `assistant(tool_use) → user(tool_result)`-Paar landen und einen Orphan-`tool_result` erzeugen. Anthropic lehnt das mit `400 invalid_request_error` ab („Each `tool_result` block must have a corresponding `tool_use` block in the previous message"); OpenAI ist genauso strikt.
- Pflicht-Helper: **`trim_history(turns, window)`** in `core/chat/run.rs`. Nach dem Drain läuft eine Heal-Schleife, die führende Turns droppt, die kein sauberer User-Start sind (Tool-Result-Turn, Assistant-Turn, Assistant mit Tool-Calls), bis der erste Turn ein echter `user`-Turn mit Content ist. Ergebnis darf kürzer als `window` sein, niemals länger.
- **Wer den Trim umbaut, muss die Tests in `core/chat/run.rs::tests` grün halten** — dort liegen Cases für Orphan-Tool-Result, führenden Assistant, Orphan-Kette und intaktes Paar an der Grenze.
- Wenn das Window in Tool-lastigen Sessions konsistent unter ~16 Turns sackt, lohnt ein klügerer Trim: rückwärts vom Ende einen sauberen Boundary suchen, der genau `window` Turns ergibt. Erst messen (Token-Logs zeigen die effektive History-Größe nicht direkt — ggf. extra DEBUG-Log einbauen), dann optimieren.
- **Anwendungspunkt:** `trim_history`/`select_initial_turns` läuft nur **einmal pro User-Turn**, am Anfang von `react_loop` — nicht vor jedem einzelnen LLM-Call innerhalb eines ReAct-Zyklus. Bei vielen Tool-Iterationen in einer einzigen User-Anfrage (bis `MAX_REACT_ITERATIONS = 12`) wächst die tatsächlich gesendete Turn-Zahl innerhalb dieses Zyklus über `HISTORY_WINDOW` hinaus, weil neue Assistant-/Tool-Result-Turns ungetrimmt angehängt werden. Kein Bug, aber beim Tunen von `HISTORY_WINDOW`/`MAX_REACT_ITERATIONS` relevant.

## 6. Sicherheits-Pattern

```rust
// Pseudo-Code — in jedem File-Tool anzuwenden:
pub async fn execute(input: ToolInput, ctx: ToolContext) -> Result<ToolOutput> {
    let requested_path = PathBuf::from(&input.path);
    let safe_path = ensure_in_agent_folder(&ctx.agent_folder, &requested_path)?;
    // ... weitermachen mit safe_path
}

fn ensure_in_agent_folder(agent_folder: &Path, requested: &Path) -> Result<PathBuf> {
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        agent_folder.join(requested)
    };
    let canonical = absolute.canonicalize().map_err(|_| Error::PathInvalid)?;
    if !canonical.starts_with(agent_folder.canonicalize()?) {
        return Err(Error::PathOutsideAgentFolder);
    }
    Ok(canonical)
}
```

Zusätzlich: Symlink-Escape-Prävention durch `canonicalize`; Denylist für spezielle Dateien (`.DS_Store`, `Thumbs.db` ignorieren aber nicht manipulieren); maximale Dateigröße-Limits für Lese-Tools.

**Zweite Variante für Schreib-Tools:** `ensure_in_agent_folder` verlangt, dass der Pfad bereits existiert (Standardfall für Lese-Tools). Schreib-Tools, deren Zielpfad noch nicht existiert (`write_docx.rs`, `write_xlsx.rs`, `update_xlsx_cell.rs`, `rewrite_file.rs`, `append_to_docx.rs`, `write_docx_from_template.rs`, `delegate_into_xlsx_column.rs`), nutzen stattdessen `ensure_inside_sandbox` (definiert in `core/tool/tools/write_docx.rs`), das fehlende Elternverzeichnisse per `create_dir_all` anlegt, bevor kanonisiert wird. Gleiches Sicherheitsprinzip, andere Existenz-Annahme — beide Funktionen im Auge behalten, wenn du an der Sandbox-Grenze arbeitest.

## 7. Test-Strategie

- **Rust (Ist-Stand):** `cargo test` läuft, aber die Abdeckung liegt fast ausschließlich in Core-Logik-Modulen (`sandbox.rs`, `workspace.rs`, `chat/freshness.rs`, `chat/run.rs`, `skill/registry.rs`, `llm/json_cleanup.rs`, `preview/*.rs`) — nicht "pro Tool": die einzelnen Dateien unter `core/tool/tools/` haben (bis auf einen Test in `read_xlsx_range.rs`) keine Unit-Tests. Es gibt **keine** Integration-Tests für den ReAct-Loop mit Mock-LLM — kein Mock-`LlmProvider` existiert im Repo. Wer das aufbaut: neuer Test-Ordner + Fake-Provider, der `LlmProvider` implementiert.
- **Frontend (Ist-Stand):** Kein `vitest`, keine `*.test.ts(x)`-Dateien, kein Test-Script in `package.json` — Frontend-Tests existieren aktuell nicht. Storybook weiterhin nicht aufgesetzt (bleibt für spätere Phasen optional).
- **Build-Gates:** laufen aktuell über CI (`.github/workflows/ci.yml`, getriggert auf `push`/`pull_request` gegen `main`), nicht als lokaler Pre-Commit-Hook — es gibt keine Husky-/Git-Hook-Konfiguration im Repo. CI führt `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` (**ohne** `--no-default-features`) und in einem separaten Frontend-Job `tsc && npm run build` aus. Vor jedem Commit lokal dieselben Befehle laufen zu lassen bleibt trotzdem beste Praxis, auch wenn nichts sie technisch erzwingt. Smoke-Run im `tauri dev`-Fenster für jede HITL-fähige Änderung.
- **E2E:** Playwright/WebDriver-basierte Tests sind für Phase 5/6 vorgesehen, aktuell noch nicht aufgesetzt — in der Zwischenzeit reicht der manuelle Smoke-Run.
- **Tool-Calling mit echten Modellen:** Eigenes Test-Script ist nicht aufgebaut; statt dessen läuft die Validierung pro Skill manuell beim Smoke-Run nach jeder Etappe. (`benchmarks/` ist ein historisches Artefakt aus Phase 2c für Runtime-Vergleiche, kein Tool-Calling-Test.)

## 8. Sprach-Konvention

- **Code:** Englisch (Variablen, Funktionsnamen, Kommentare, Git-Commit-Messages).
- **UI-Strings:** i18next ist bereits im Einsatz (`src/lib/i18n.ts`, `src/locales/{de,en,es,fr,it,pl}.ts`, `useTranslation`/`t(...)` in ~20 Komponenten). Deutsch bleibt die primäre/Referenz-Sprache, aber es ist **nicht** mehr nur "fest verdrahtet mit Retrofit-Potenzial" — die i18n-Library ist produktiv. Neue UI-Strings gehören in `src/locales/*.ts`, nicht hartkodiert in Komponenten.
- **Dokumentation im Repo:** Deutsch (CONCEPT.md, dieses Dokument). Die öffentliche Doku (separates Repo, `www.processfox.ai/docs/`) ist ebenfalls deutsch — der Owner ist deutschsprachig und Beta-Tester:innen ebenfalls.
- **SKILL.md-Bodies:** Englisch. Standard-Hinweis im Prompt: "Respond in the user's language."

## 9. Commit- und PR-Konventionen

- Conventional Commits: `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`, `build:`.
- PR-Beschreibungen enthalten: Was ändert sich, warum, welche Tests laufen.
- Feature-Branches: `feat/<phase>-<kurz>` (z. B. `feat/3-folder-search-skill`).
- Stable-Branch: `main`. Alles geht über PR. Direct-Push auf `main` ist verboten.

## 10. Wenn du unsicher bist

- **Architektur-Frage:** Lies zuerst `CONCEPT.md` §4 (Taxonomie) und §6 (Verhalten). Wenn es immer noch unklar ist, markiere die Stelle mit `// TODO(decision):` und frage den Owner explizit.
- **UX-Frage:** Schau zu Obsidian und Claude Cowork als Referenz. Bei echter Unsicherheit: Minimal-Variante implementieren und Feedback einholen, statt lange Diskussion.
- **Performance-Frage:** Erst messen, dann optimieren. Profile mit `cargo flamegraph` oder Chrome DevTools, bevor du umbaust.
- **Fehlende Abhängigkeit:** Neue Crates/NPM-Pakete vor dem Hinzufügen begründen (Issue / PR-Description). Wir halten die Abhängigkeiten bewusst schlank.

## 11. Was NICHT zu tun ist

- Keine eigene State-Library einführen, solange Context + `useState` ausreichen.
- Keine Mikroservice-Architektur oder externe Services.
- Keine KI-generierten Skills in v1 (kein "Agent schreibt seinen eigenen Skill"). Aktuell existiert noch **gar keine** Skill-Erstellungs-UI — Skills entstehen nur als manuell angelegte `SKILL.md`-Dateien unter `skills_builtin/`; im Frontend lassen sich Skills pro Agent nur an-/abwählen (`AgentEditorDialog`). Falls eine Erstellungs-UI gebaut wird, darf sie ausschließlich formularbasierte Anlage erlauben.
- Keine impliziten Berechtigungen — jede Datei-Operation ist explizit gesandboxt.
- Keine Chat-History-Sidebar. Ernsthaft.
- Keine Einführung einer Skript-Sprache für User in v1.

## 12. Release-Prozess (Kurz)

Laufende Änderungen werden in [`CHANGELOG.md`](CHANGELOG.md) unter `## [Unreleased]` mitgeschrieben — beim Release wird dieser Block einfach in den neuen Versionsabschnitt umbenannt und dient direkt als Release-Notes.

1. Alle Akzeptanzkriterien der aktuellen Phase (Roadmap unter `https://www.processfox.ai/docs/entwickler/roadmap/`) sind erfüllt.
2. Version in `package.json`, `src-tauri/tauri.conf.json` **und** `src-tauri/Cargo.toml` bumpen — alle drei müssen übereinstimmen, sonst meldet `get_app_info` (liest `CARGO_PKG_VERSION`) eine falsche Version an die UI.
3. In `CHANGELOG.md` den `## [Unreleased]`-Block in `## [<version>] — <YYYY-MM-DD>` umbenennen und einen neuen leeren `## [Unreleased]`-Block oben einfügen.
4. Schritte 2–3 auf `main` mergen.
5. Auf GitHub → Actions → "Release" → **"Run workflow"** klicken (`workflow_dispatch`).
6. GitHub Actions baut auf drei Plattformen (macOS ARM, Linux x64, Windows x64). Die `tauri-action` erstellt automatisch den Tag `v<VERSION>` und ein **Draft-Release** mit den Build-Artefakten.
7. Draft-Release auf GitHub öffnen, den frisch umbenannten CHANGELOG-Block als Release-Notes übernehmen (ggf. um Download-Hinweise und macOS-Quarantäne-Workaround ergänzen), dann **"Publish release"** klicken.
8. Für Store-Versionen: `.github/workflows/release-store.yml` (ebenfalls `workflow_dispatch`) baut die Store Edition (`PROCESSFOX_EDITION=store`, `LICENSE-STORE`) für macOS und Windows und lädt unsignierte Artefakte hoch — Code-Signing für Apple/Microsoft ist als TODO im Workflow markiert und muss vor der Store-Einreichung ergänzt werden. Die fertigen Artefakte dann über die jeweiligen Store-Portale einreichen (Apple App Store Connect, Microsoft Partner Center). Auto-Updates laufen über den Store-Mechanismus — kein eigener Updater nötig.

---

Wenn du dieses Dokument liest und etwas unvollständig oder widersprüchlich findest: bitte melde es und aktualisiere es im selben PR, in dem du die neue Arbeit hinzufügst. Dieses Dokument lebt mit dem Projekt.
