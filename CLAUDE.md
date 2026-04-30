# CLAUDE.md — Arbeits-Anweisungen für Claude Code

Dieses Dokument richtet sich an Claude Code (und an alle anderen LLM-gestützten Codier-Assistenten), die an ProcessFox mitarbeiten. Es fasst Projekt-Kontext, Tech-Stack, Code-Stil-Regeln und Architektur-Prinzipien so zusammen, dass Entscheidungen konsistent mit der Produkt-Vision bleiben.

**Pflicht-Lektüre vor jedem größeren Task:**
- [`CONCEPT.md`](CONCEPT.md) — vollständige Produkt-Vision und Architektur (bleibt im App-Repo, intern)
- Architektur, Roadmap, Skill-Dokus und LLM-Kompatibilität liegen im **Landing-Repo** unter `landing/src/content/docs/docs/`. Das `landing/`-Verzeichnis ist im App-Repo gitignored und wird separat synchronisiert — wenn es lokal vorhanden ist, lies direkt von dort:
  - `landing/src/content/docs/docs/entwickler/architektur.md` — technische Architekturskizze
  - `landing/src/content/docs/docs/entwickler/roadmap.md` — aktuelle Phase
  - `landing/src/content/docs/docs/entwickler/eigenen-skill-bauen.md` — SKILL.md-Template + Konventionen
  - `landing/src/content/docs/docs/modelle/kompatibilitaet.md` — welche lokalen Modelle ProcessFox laden kann (Format, Architektur, Chat-Template, Tool-Calling). Konsultieren, bevor du Modelle in den Catalog aufnimmst oder Custom-URL-Empfehlungen formulierst.
  - `landing/src/content/docs/docs/skills/<skill>.md` — wenn du an einem konkreten Skill arbeitest
- Wenn `landing/` nicht lokal liegt, ist die gleiche Doku öffentlich unter `https://www.processfox.ai/docs/` erreichbar.

## 1. Projekt-Kurzprofil

- **Produkt:** ProcessFox — Desktop-App für lokale KI-Agenten, Zielgruppe Einsteiger (kleine Unternehmen, NGOs).
- **Framework:** Tauri v2.
- **Frontend:** React 19 + Vite + TypeScript + Tailwind + shadcn/ui.
- **Backend:** Rust (pure Rust, keine Python-Abhängigkeit).
- **LLM-Runtime:** `llama-cpp-2` (in Phase 3 von mistral.rs migriert, weil dessen GGUF-Loader Gemma 4 nicht kannte). Nutzt `apply_chat_template_oaicompat` + `streaming_state_oaicompat` für native Tool-Calling- und Reasoning-Extraktion. Cloud-Provider parallel via separate Implementierungen.
- **Distribution:** GitHub Releases, Auto-Updater via Tauri Updater, GitHub Actions.

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
├── LICENSE
├── .gitignore
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.js
├── index.html
├── src/                            # Frontend (React + TS)
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── agent/                  # AgentSwitcher, AgentEditorDialog, SkillIconRow
│   │   ├── chat/                   # ChatPane, ChatInput, HitlCard, AskUserCard,
│   │   │                           # ToolCallChip, ReasoningChip
│   │   ├── filetree/               # FileTree (react-arborist)
│   │   ├── preview/                # PreviewPane für Datei-Vorschau
│   │   ├── settings/               # ModelsTab, CloudApisTab
│   │   ├── theme-provider.tsx
│   │   └── ui/                     # shadcn-Bausteine
│   ├── views/
│   │   ├── Main.tsx
│   │   └── Settings.tsx
│   ├── hooks/                      # useAgentChat etc.
│   ├── lib/
│   │   └── tauri.ts                # typed invoke() wrappers + event subs
│   └── types/                      # ChatMessage, Agent, Skill, …
├── src-tauri/                      # Backend (Rust)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── skills_builtin/             # eingebaute Skills (include_dir!-eingebunden)
│   │   ├── folder-search/
│   │   │   └── SKILL.md
│   │   ├── document-extend/
│   │   ├── document-create-docx/
│   │   ├── document-edit/
│   │   ├── document-from-template/
│   │   ├── table-read/
│   │   ├── table-update/
│   │   ├── table-create/
│   │   └── chat-context/
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
│       │   ├── secrets.rs
│       │   ├── settings.rs
│       │   └── skill.rs
│       └── core/
│           ├── agent.rs            # Agent-Datenmodell, Persistenz
│           ├── chat/               # ChatRunner, ReAct-Loop, ChatRepo
│           │   ├── run.rs          # ReAct-Orchestrierung + HITL/AskUser-Pipelines
│           │   └── repo.rs         # JSONL-Persistenz pro Agent
│           ├── error.rs            # CoreError + CommandError
│           ├── hardware.rs         # RAM/VRAM-Erkennung
│           ├── llm/                # LlmProvider Trait + Implementierungen
│           │   ├── anthropic.rs
│           │   ├── openai.rs
│           │   ├── openai_compat.rs
│           │   ├── openrouter.rs
│           │   ├── local_gguf.rs   # llama-cpp-2-Wrapper, Idle-Watcher
│           │   ├── json_cleanup.rs
│           │   └── registry.rs
│           ├── models/             # GGUF-Catalog, Download-Runner
│           ├── sandbox.rs          # ensure_in_agent_folder
│           ├── secrets.rs          # keyring-API-Key-Storage
│           ├── settings.rs
│           ├── skill/              # SKILL.md-Parsing, SkillRegistry
│           ├── storage.rs          # AppPaths
│           ├── tool/
│           │   ├── mod.rs          # Tool Trait, HitlPreview, ToolContext
│           │   ├── registry.rs
│           │   └── tools/          # einzelne Tool-Implementierungen
│           │       ├── list_folder.rs / grep_in_files.rs / read_file.rs
│           │       ├── read_pdf.rs / read_docx.rs / read_xlsx_range.rs
│           │       ├── append_to_md.rs / append_to_docx.rs / rewrite_file.rs
│           │       ├── write_docx.rs / write_docx_from_template.rs
│           │       ├── write_xlsx.rs / update_xlsx_cell.rs
│           │       └── ask_user.rs
│           ├── types.rs
│           └── watcher.rs          # notify-debouncer-mini Folder-Watch
├── assets/                         # Marketing-Assets, App-Icons (nicht im Bundle)
├── benchmarks/                     # Mess-Skripte für LLM-Runtime-Vergleiche
├── public/                         # statische Assets fürs Vite-Frontend
├── landing/                        # gitignored, separat synchronisiert
│                                   # — Astro-/Starlight-Projekt für www.processfox.ai
│                                   # — enthält Architektur-, Roadmap-, Skill- und Modell-Doku
└── .github/
    └── workflows/
        └── release.yml
```

> **`docs/` existiert nicht mehr im App-Repo.** Architektur, Roadmap, Skill-Dokus und LLM-Kompatibilität sind in das `landing/`-Verzeichnis (Astro-/Starlight-Doku) gewandert; `LLM_COMPATIBILITY.md` ebenfalls. `CONCEPT.md` bleibt als interne Vision im Repo-Root.

## 5. Wichtige Schnittstellen-Konventionen

### Tauri Commands (Rust → Frontend)

- Jeder Command nimmt `tauri::State<'_, AppState>` entgegen. `AppState` selbst ist `Clone` mit pro-Feld `Arc<OnceLock<…>>` für lazy-initialisierte Singletons (ChatRunner, DownloadRunner, FolderWatcher) — kein globaler Mutex außenrum.
- Fehler werden als `Result<T, CommandError>` zurückgegeben, wobei `CommandError` serialisierbar ist und einen `code`, `message`, und optional `details` enthält.
- Lange Operationen (Modell-Download, ReAct-Loop) laufen via Tauri-Events (`app.emit`), nicht als Return-Value.
- Command-Namen in `snake_case` in Rust, TypeScript-Seite wrappt zu `camelCase`.
- **Serde-Falle**: Bei getaggten Enums (wie `RunEvent`, `HitlPreview`) reicht `#[serde(rename_all = "camelCase")]` allein nicht — das renamed nur die Varianten-Tags, nicht die Felder *innerhalb* der Varianten. Immer `rename_all_fields = "camelCase"` zusätzlich setzen, sonst kommt das JSON mit snake_case-Feldnamen am Frontend an.

### Frontend-Bridge (`src/lib/tauri.ts`)

- Zentrale Typ-sichere Wrapper für alle Commands.
- Event-Listener für Live-Updates (Tool-Call-Status, Download-Progress) als Custom Hooks.

### LLM-Runtime-Abstraktion

- Trait `LlmProvider` mit async `generate(request, sink, cancel) -> CoreResult<()>`. Streamt `LlmEvent`s über einen `mpsc::Sender`, respektiert `CancellationToken`.
- Implementierungen: `LocalGgufProvider` (llama-cpp-2), `AnthropicProvider`, `OpenAiProvider`, `OpenRouterProvider`.
- Einheitliches Event-Format: `TextDelta`, `ReasoningDelta` (für `<|channel>thought` u. ä.), `ToolCall`, `Usage(TokenUsage)`, `Finish { reason }`, `Error { code, message }`.
- `supports_tools()` markiert Provider, die `request.tools` verarbeiten können — der ReAct-Loop reicht Tools nur an Provider, die das bestätigen.
- **Lokales Modell-Lifecycle:** `LocalGgufProvider` hält ein Modell zwischen Generations geladen, entlädt es aber nach 10 min Idle automatisch (Watcher in `ensure_idle_watcher`). Wer den RAM-Bedarf debuggt oder zusätzliche Trigger zum Entladen einbaut (z. B. beim Wechsel auf Cloud-Provider), arbeitet hier — nicht den Watcher umgehen, sondern ergänzen.

### Token-Usage-Logging

- Provider emittieren genau einmal pro Generation ein `LlmEvent::Usage(TokenUsage)`, direkt vor dem terminalen `Finish`. `TokenUsage` führt `input_tokens`, `output_tokens`, `cached_input_tokens` und `cache_creation_input_tokens` (die letzten beiden `Option<u32>`).
- Welcher Provider was füllt:
  - **Anthropic:** alle vier Felder (Cache-Werte stehen im `message_start.usage`-Block, output_tokens kumulativ in `message_delta.usage`).
  - **OpenAI / OpenRouter:** `input_tokens`, `output_tokens`, `cached_input_tokens` (aus `prompt_tokens_details.cached_tokens`). Voraussetzung: `OpenAiCompat::new(..., include_usage = true)` schickt `stream_options.include_usage`. Für selbstgehostete Compat-Server bleibt das opt-in, weil ältere Backends den Schlüssel ablehnen.
  - **Local GGUF:** exakte `input_tokens` (aus `tokens.len()`) und `output_tokens` (aus `n_cur - prompt_len`); Cache-Felder bleiben `None`, solange wir bei jedem Call einen frischen `LlamaContext` bauen.
- Aggregation passiert im `react_loop` (`core/chat/run.rs`): pro Iteration ein `tracing::debug!`-Eintrag, am Ende des Runs ein `tracing::info!("chat run usage", provider, model, iterations, input_tokens, output_tokens, cached_input_tokens, cache_creation_input_tokens)`. Logfile liegt unter `<app-support>/ProcessFox/logs/processfox.log.<datum>`.
- Wenn ein Provider keine Usage liefert (Compat-Backend ohne `include_usage`, abgebrochener Stream), bleibt das `Usage`-Event aus — der Runner darf nicht hängen, und der Run-Total-Log wird übersprungen statt mit Nullen geschrieben.

### Tool-Registry

- Tools sind in einer zentralen Registry registriert (`HashMap<String, Arc<dyn Tool>>`).
- `trait Tool { fn name() -> &str; fn schema() -> JsonSchema; async fn execute(input, context) -> Result<Output>; }`.
- `context` enthält Agent-ID, Agent-Ordner-Pfad, App-Handle für Events.

### Skill-Loading

- Beim App-Start werden `src-tauri/skills_builtin/` und `<app-support>/skills/user/` gescannt.
- Frontmatter wird via `serde_yaml` oder `gray_matter` geparsed.
- Geladen in ein `SkillRegistry`, von dort kann der Agent sie abrufen.

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

## 7. Test-Strategie

- **Rust:** `cargo test` für Unit-Tests pro Tool. Integration-Tests für ReAct-Loop mit Mock-LLM.
- **Frontend:** `vitest` für Hooks und Utility-Funktionen. Storybook optional für Komponenten in späteren Phasen.
- **Build-Gates vor jedem Commit:** `cargo fmt`, `cargo clippy --all-targets --no-default-features -- -D warnings`, `npm run build`. Smoke-Run im `tauri dev`-Fenster für jede HITL-fähige Änderung.
- **E2E:** Playwright/WebDriver-basierte Tests sind für Phase 5/6 vorgesehen, aktuell noch nicht aufgesetzt — in der Zwischenzeit reicht der manuelle Smoke-Run.
- **Tool-Calling mit echten Modellen:** Eigenes Test-Script ist nicht aufgebaut; statt dessen läuft die Validierung pro Skill manuell beim Smoke-Run nach jeder Etappe.

## 8. Sprach-Konvention

- **Code:** Englisch (Variablen, Funktionsnamen, Kommentare, Git-Commit-Messages).
- **UI-Strings:** Deutsch (in v1 fest verdrahtet, keine i18n-Library nötig — aber so strukturiert, dass i18n später nachrüstbar ist; z. B. ein `src/lib/strings.ts` mit Keys).
- **Dokumentation im Repo:** Deutsch (CONCEPT.md, dieses Dokument). Die öffentliche Doku im `landing/`-Repo ist ebenfalls deutsch — der Owner ist deutschsprachig und Beta-Tester:innen ebenfalls.
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
- Keine KI-generierten Skills in v1 (Skill-Erstellungs-UI erlaubt nur formularbasierte Anlage, kein "Agent schreibt seinen eigenen Skill").
- Keine impliziten Berechtigungen — jede Datei-Operation ist explizit gesandboxt.
- Keine Chat-History-Sidebar. Ernsthaft.
- Keine Einführung einer Skript-Sprache für User in v1.

## 12. Release-Prozess (Kurz)

1. Alle Akzeptanzkriterien der aktuellen Phase (Roadmap unter `landing/src/content/docs/docs/entwickler/roadmap.md` bzw. `https://www.processfox.ai/docs/entwickler/roadmap/`) sind erfüllt.
2. Version in `package.json` und `src-tauri/tauri.conf.json` bumpen.
3. Tag setzen (`git tag v0.x.y`) und pushen.
4. GitHub Actions `release.yml` baut Mac / Windows / Linux und pusht Artefakte an GitHub Release.
5. Release-Notes verfassen (was ist neu, was ist bekannt fehlerhaft).
6. Auto-Updater holt sich die neue Version bei den Nutzer:innen.

---

Wenn du dieses Dokument liest und etwas unvollständig oder widersprüchlich findest: bitte melde es und aktualisiere es im selben PR, in dem du die neue Arbeit hinzufügst. Dieses Dokument lebt mit dem Projekt.
