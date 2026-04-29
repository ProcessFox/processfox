# ProcessFox — Technische Architektur

Dieses Dokument skizziert die technische Architektur von ProcessFox v1.0. Es ergänzt [`../CONCEPT.md`](../CONCEPT.md) um die Implementierungs-Sicht.

## 1. Systemüberblick

```
┌─────────────────────────────────────────────────────────────┐
│                    Tauri v2 Application                     │
│                                                             │
│  ┌────────────────────────┐      ┌────────────────────┐     │
│  │   Frontend (React)     │◄────►│  Backend (Rust)    │     │
│  │   - UI (Obsidian-like) │      │  - Agents          │     │
│  │   - Chat-Renderer      │ IPC  │  - ReAct-Loop      │     │
│  │   - File-Tree/Preview  │      │  - Tool-Registry   │     │
│  │   - Settings-Modal     │      │  - Skill-Registry  │     │
│  └────────────────────────┘      │  - LLM-Runtime     │     │
│                                  │  - Sandbox         │     │
│                                  │  - Storage         │     │
│                                  └────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
                                             │
           ┌─────────────────────────────────┼──────────────────┐
           ▼                                 ▼                  ▼
   ┌───────────────┐               ┌────────────────┐    ┌─────────────┐
   │ Agent-Ordner  │               │ App-Support-   │    │ Cloud-LLM-  │
   │ (User-Files)  │               │ Ordner         │    │ Provider    │
   │ XLSX,DOCX,PDF │               │ agents/        │    │ (optional)  │
   │               │               │ skills/        │    │             │
   │               │               │ models/        │    │             │
   │               │               │ settings.json  │    │             │
   └───────────────┘               └────────────────┘    └─────────────┘
```

## 2. Datenfluss: ein Nutzer-Auftrag

```
User sendet Chat-Nachricht
      │
      ▼
Frontend ruft invoke("send_message", { agentId, message })
      │
      ▼
Backend: core::react_loop::run_loop(agent, message)
      │
      ├─► Lädt Agent, aktive Skills, Chat-Verlauf
      │
      ├─► Baut Prompt zusammen:
      │     [SystemPrompt] + [Skill-Descriptions] + [ChatVerlauf] + [UserMessage]
      │
      ├─► LLM-Provider.generate(...)
      │     ├─► Streamt TextDelta per event "chat/delta"
      │     └─► Bei ToolCall: Return-Event
      │
      ├─► Falls ToolCall:
      │     ├─► Tool-Registry: sucht Tool per Name
      │     ├─► Sandbox-Check (Pfad im Agent-Ordner?)
      │     ├─► Wenn schreibend & HITL: Event "hitl/request"
      │     │     └─► Frontend zeigt Inline-Diff-Karte, wartet auf User-Freigabe
      │     ├─► Tool::execute(...)
      │     ├─► Event "tool/status" mit Fortschritt
      │     └─► Ergebnis zurück in Loop
      │
      └─► Loop wiederholt, bis Finish oder Max-Iter
            │
            ▼
      Event "chat/finished"
            │
            ▼
      Frontend zeigt finale Antwort, persistiert Chat-Verlauf
```

## 3. Kern-Module (Rust)

### `core::agent`
Verwaltet Agent-Datensätze. CRUD auf `<app-support>/agents/<uuid>.json`. Lädt/speichert Chat-Verlauf als JSONL (append-only für Stabilität).

### `core::skill`
Scannt `src-tauri/skills_builtin/` und `<app-support>/skills/user/`. Parsed SKILL.md-Frontmatter via `gray_matter` + `serde_yaml`. Hält eine `SkillRegistry` im Speicher. Stellt Skills als System-Prompt-Fragment bereit (Name + Description + Tool-Liste).

### `core::tool`
Globale Tool-Registry. Trait-basiert: jedes Tool implementiert `trait Tool`. Stellt JSON-Schema für LLM-Function-Calling bereit. Dispatcher führt Tool-Calls aus, inklusive Sandbox-Check.

### `core::chat`
Orchestriert den Agent-Loop (`core::chat::run`). Führt Chat-Iterationen, dispatcht Tool-Calls, emittiert Events für Frontend, fährt HITL- und AskUser-Pipelines. Max-Iter-Sicherung (Default 12, in Agent-Config überschreibbar). Persistenz pro Agent in `core::chat::repo` (JSONL).

### `core::sandbox`
Zentrale Pfad-Validierung. Alle Tool-Input-Pfade laufen durch `ensure_in_agent_folder`. Verhindert Symlink-Ausbruch via `canonicalize`. Denylist für Spezialdateien.

### `core::storage`
Wissen über Plattform-spezifische App-Support-Pfade (`dirs` crate). Verwaltet `settings.json`, Modell-Katalog, Logs.

### `core::llm`
Abstraktion `trait LlmProvider`. Implementierungen:
- `LocalGgufProvider` — `llama-cpp-2`-Wrapper. Nutzt `apply_chat_template_oaicompat` + `streaming_state_oaicompat`, sodass Tool-Calling und Reasoning-Extraktion pro Modell vom llama.cpp-Parser geliefert werden.
- `AnthropicProvider` — Messages-API
- `OpenAiProvider` — Chat-Completions
- `OpenAiCompatProvider` — generischer OpenAI-kompatibler Endpoint (lokale Server etc.)
- `OpenRouterProvider` — OpenAI-kompatibel

Einheitliches Streaming-Event-Format (`LlmEvent`): `TextDelta`, `ReasoningDelta` (für Thinking-Channels), `ToolCall { id, name, arguments }`, `Finish { reason }`, `Error { code, message }`. `supports_tools()` markiert Provider, denen der ReAct-Loop Tools mitgeben darf.

**Lifecycle des lokalen Modells:** `LocalGgufProvider` hält genau ein Modell zur Zeit im RAM. Es bleibt zwischen Generations geladen, wird aber nach **10 Minuten ohne Aktivität** automatisch entladen — der RAM (mehrere GB plus KV-Cache) wird also nicht dauerhaft belegt, wenn die Nutzerin auf einen Cloud-Provider wechselt oder den Chat ruhen lässt. Ein Modellwechsel (anderer `filename`) triggert einen sofortigen Unload-und-Reload. Der nächste Prompt nach einem Idle-Unload kostet einmalig den Reload (~1–3 s SSD-Read bei einem 2-GB-Modell). Watcher-Implementierung in `core/llm/local_gguf.rs` (`ensure_idle_watcher`).

### `core::tool::tools::*`
Einzelne Tool-Implementierungen (alle in `src-tauri/src/core/tool/tools/`):
- `list_folder` — Listet Dateien/Ordner (rekursiv, mit Filter).
- `read_file` — Liest Text-Datei (mit Größen-Limit).
- `grep_in_files` — Textsuche in mehreren Dateien.
- `read_pdf` — Via `pdf-extract`. Optional Page-Range.
- `read_docx` — Via `docx-rs`, extrahiert strukturierten Text.
- `read_xlsx_range` — Via `calamine`, gibt Bereich als 2D-Array.
- `write_docx` — Via `docx-rs`, neue DOCX schreiben.
- `write_docx_from_template` — Lädt Template, ersetzt `{{placeholder}}`-Slots.
- `write_xlsx` — Via `umya-spreadsheet`, neue XLSX schreiben.
- `append_to_md` — Hängt Text an MD-/TXT-Datei an.
- `append_to_docx` — Hängt strukturierte Inhalte an bestehendes DOCX an.
- `rewrite_file` — Vollständiger Datei-Rewrite mit Diff-Preview.
- `update_xlsx_cell` — Aktualisiert eine oder mehrere Zellen in einer XLSX.
- `ask_user` — Erzeugt HITL-Event, wartet auf Antwort.

## 4. Kern-Module (Frontend)

### `views/Main.tsx`
Haupt-Layout. Dreispaltig (resizable): Sidebar, optional Preview, Chat.

### `views/Settings.tsx`
Settings-View mit Tabs: Modelle (`ModelsTab`), Cloud-APIs (`CloudApisTab`). Sprache/Theme/About sind in v1 noch nicht ausgebaut.

### `components/agent/AgentSwitcher.tsx`
Oben in Sidebar. Zeigt aktive Agent, Dropdown zum Wechsel, "Neuer Agent"-Eintrag.

### `components/agent/AgentEditorDialog.tsx`
Formular: Name, Icon, Ordner (File-Picker), System-Prompt (Textarea), Modell-Auswahl (Dropdown), Skills (Checkbox-Liste mit Icons).

### `components/agent/SkillIconRow.tsx`
Horizontal scrollbare Icon-Leiste unter dem Agent-Namen. Tooltip mit Skill-Name bei Hover.

### `components/filetree/FileTree.tsx`
Datei-Baum basiert auf `react-arborist`. Unterstützt Expand/Collapse, Icons nach Datei-Typ, Click-Handler für Preview.

### `components/preview/PreviewPane.tsx`
Router basierend auf Datei-Endung:
- `.md`, `.txt` → Markdown-Editor (CodeMirror 6, editierbar)
- `.pdf` → PDF-Viewer (via `pdfjs-dist`)
- `.docx` → DOCX-Preview (via `mammoth.js` zu HTML, readonly)
- `.xlsx` → XLSX-Preview (via SheetJS, tabellarisch)
- `.png`, `.jpg`, etc. → Bild-Viewer

### `components/chat/ChatPane.tsx`
Scrollender Chat. Rendert User- und Agent-Messages, Tool-Call-Chips, HITL-Karten, Streaming-Text.

### `components/chat/ChatInput.tsx`
Textarea + Senden-Button + Stop-Button, hängt unten am Chat.

### `components/chat/ToolCallChip.tsx`
Kleiner Status-Chip (z. B. "🔍 ordner durchsuchen..." mit Spinner, dann "✓ 12 Dateien gefunden").

### `components/chat/ReasoningChip.tsx`
Klappbare Anzeige für Modell-Reasoning (Thinking-Channels), die `LlmEvent::ReasoningDelta` produziert.

### `components/chat/HitlCard.tsx`
Inline-Freigabe-Karte. Zeigt Diff, Buttons "Freigeben", "Ablehnen", "Anpassen".

### `components/chat/AskUserCard.tsx`
Inline-Karte für `ask_user`-Tool-Calls. Pausiert den Loop, sammelt User-Antwort, schickt sie zurück in den Loop.

### `components/settings/`
`ModelsTab.tsx` (Modell-Katalog, Custom-URL, Download-Status), `CloudApisTab.tsx` (API-Key-Eingabe pro Provider).

> Skill-Editor-UI (`SkillEditor`) ist Phase-5-Roadmap und noch nicht implementiert.

## 5. Agent-Loop im Detail

Implementierung in `src-tauri/src/core/chat/run.rs`. Skizze des Ablaufs (vereinfacht):

```rust
pub async fn run(
    agent: &Agent,
    user_message: String,
    app: tauri::AppHandle,
    cancel: CancellationToken,
) -> CoreResult<()> {
    let mut messages = ChatRepo::load(agent)?;
    messages.push(Message::User(user_message));

    let skills = SkillRegistry::active_for(agent);
    let tools = ToolRegistry::schemas_for(&skills);
    let system_prompt = compose_system_prompt(agent, &skills);

    let provider = LlmRegistry::for_agent(agent);

    for _ in 0..agent.max_iterations.unwrap_or(12) {
        let (tx, mut rx) = mpsc::channel(32);
        let request = LlmRequest { system_prompt: &system_prompt, messages: &messages, tools: &tools };
        tokio::spawn(provider.generate(request, tx, cancel.clone()));

        let mut tool_calls = Vec::new();
        let mut text_buffer = String::new();

        while let Some(event) = rx.recv().await {
            match event {
                LlmEvent::TextDelta(d) => { app.emit("chat/delta", &d)?; text_buffer.push_str(&d); }
                LlmEvent::ReasoningDelta(d) => app.emit("chat/reasoning", &d)?,
                LlmEvent::ToolCall { id, name, arguments } => tool_calls.push((id, name, arguments)),
                LlmEvent::Finish { .. } if tool_calls.is_empty() => {
                    messages.push(Message::Assistant(text_buffer));
                    ChatRepo::persist(agent, &messages)?;
                    return Ok(());
                }
                LlmEvent::Finish { .. } => break,
                LlmEvent::Error { code, message } => return Err(CoreError::Llm { code, message }),
            }
        }

        for (id, name, args) in tool_calls {
            app.emit("tool/status", ToolStatus::running(&name))?;
            let tool = ToolRegistry::get(&name).ok_or(CoreError::UnknownTool)?;
            let ctx = ToolContext::new(agent, app.clone());
            // HITL: bei schreibenden Tools wird hier ein "hitl/request"-Event emittiert
            // und der Loop pausiert, bis der User entscheidet (siehe core::chat::run).
            let result = tool.execute(args, &ctx).await?;
            app.emit("tool/status", ToolStatus::done(&name))?;
            messages.push(Message::ToolResult { id, content: result });
        }
    }

    Err(CoreError::MaxIterationsReached)
}
```

## 6. Persistenz-Format

### `agents/<uuid>.json`
Siehe `CONCEPT.md` §7.

### `agents/<uuid>.chat.jsonl`
Append-only, eine Message pro Zeile:
```jsonl
{"role":"user","content":"Fasse mir die PDFs zusammen","timestamp":"2026-04-23T10:00:00Z"}
{"role":"assistant","content":"Ich schaue mir die Dateien an...","toolCalls":[{"id":"t1","name":"list_folder","args":{"path":"./"}}],"timestamp":"2026-04-23T10:00:02Z"}
{"role":"tool","toolCallId":"t1","content":"[...]"}
{"role":"assistant","content":"Ich habe 10 PDFs gefunden..."}
```

Vorteile JSONL: Append-only-sicher bei Crash, leicht tailbar für Debugging, einfach zu parsen im Frontend.

### `settings.json`
```json
{
  "language": "de",
  "theme": "system",
  "activeModelId": "google/gemma-4-e4b-gguf:Q4_K_M",
  "cloudProviders": {
    "anthropic": { "keyRef": "keychain://anthropic-key" },
    "openai": { "keyRef": "keychain://openai-key" }
  },
  "modelCatalogVersion": "2026-04-01"
}
```

## 7. LLM-Runtime: Entscheidung für `llama-cpp-2`

Die lokale GGUF-Runtime ist `llama-cpp-2` (Rust-Bindings auf llama.cpp). Der Weg dahin in Kürze:

1. **Phase 2** — Erst-Implementierung mit `mistral.rs`. Funktional, aber: GGUF-Loader hinkte llama.cpp upstream hinterher.
2. **Phase 3** — Migration auf `llama-cpp-2`, weil `mistral.rs` zum Migrationszeitpunkt Gemma 4 nicht laden konnte und Tool-Calling pro Modell-Familie selbst gepflegt werden müsste.

Warum `llama-cpp-2`:
- Schnelle Architektur-Coverage (neue Modelle wie Gemma 4 sind oft am Veröffentlichungstag in llama.cpp und damit in den Bindings nutzbar).
- Native Tool-Calling- und Reasoning-Extraktion via `apply_chat_template_oaicompat` + `streaming_state_oaicompat` — pro-Modell-Templates kommen aus llama.cpp.
- Robuster cross-platform Build, Metal/CUDA/Vulkan-Backends out-of-the-box.

`candle` ist bewusst nicht gewählt: zum Entscheidungszeitpunkt unvollständigere GGUF/Template-Unterstützung und kein vergleichbarer OpenAI-kompatibler Tool-Call-Parser.

## 8. Sandbox-Model (v1)

### Pfad-Sandbox
- Jeder schreibende oder lesende Tool-Input-Pfad wird gegen den Agent-Ordner validiert (`core::sandbox::ensure_in_agent_folder`).
- Relative Pfade werden gegen Agent-Ordner aufgelöst.
- Absolute Pfade außerhalb des Agent-Ordners werden abgelehnt.
- Symlinks werden via `canonicalize` aufgelöst, der kanonische Pfad muss im Agent-Ordner liegen.

### Ausführungs-Sandbox (Infrastruktur vorbereitet)
- Eingebaute Skills, die Scripts ausführen müssten, laufen in einem begrenzten Rust-Kontext ohne Netzwerk-Zugriff.
- Für v1: kein User-Script-Support. Die Infrastruktur (Capability-System) wird aufgebaut, aber nicht exponiert.

## 9. Fehler-Handling-Strategie

- **Recoverable Errors** (z. B. Datei nicht gefunden, ungültiger Pfad, LLM-Timeout): werden in den Chat als `Assistant-Nachricht` gespielt ("Ich konnte die Datei X nicht öffnen: ...") und der Loop läuft weiter oder endet sauber.
- **Unrecoverable Errors** (z. B. Modell nicht geladen, Agent-Config korrupt): werden als Toast im UI angezeigt, Loop wird abgebrochen.
- **Logs:** Alles Schwere geht nach `<app-support>/logs/processfox.log` mit Timestamp, Agent-ID, Fehler-Details. Frontend hat einen "Logs öffnen"-Button in den Settings.

## 10. Performance-Ziele

- App-Start (ohne Modell-Load): ≤ 3 Sekunden Cold-Start.
- Modell-Load (GGUF, warm cache): ≤ 30 Sekunden.
- Tool-Call ohne LLM-Zeit: ≤ 1 Sekunde.
- Datei-Baum für Agent-Ordner mit 1000 Dateien: ≤ 500 ms.
- Chat-Nachricht senden bis erstes Token: ≤ 2 Sekunden (lokal), ≤ 4 Sekunden (Cloud).

## 11. Erweiterbarkeits-Punkte für spätere Versionen

Explizit offen gelassen im Design, damit spätere Features ohne Umbau andocken können:
- **Web-Skills:** Tool-Trait erlaubt in Zukunft HTTP-Tools; Tool-Context kann Capability-Flags tragen.
- **User-Scripts:** Sandbox-Module ist auf Capability-basiertes Modell ausgelegt, kein grundsätzlicher Umbau nötig.
- **Multi-Agent-Kollaboration:** Agent-IDs in Tool-Calls erlauben später Verweise auf andere Agenten.
- **Skill-Marketplace:** Skill-Ordner-Struktur erlaubt simple Download-/Installations-Flows.
- **Internationalisierung:** UI-Strings sind in `src/lib/strings.ts` zentralisiert, leicht auf i18n-Lib migrierbar.
