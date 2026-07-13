# Changelog

Alle nennenswerten Änderungen an ProcessFox werden hier festgehalten.

Format: [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).
Versionsschema: [Semantic Versioning](https://semver.org/lang/de/).

## [Unreleased]

### Offen / Geplant
- **Zwischen-Reasoning in den nächsten Request zurückspeisen.** Aktuell wird
  das pro ReAct-Iteration extrahierte `reasoning` zwar persistiert, aber bei
  Tool-Call-Turns nicht in den Folge-Request mitgegeben — das Modell verliert
  zwischen den Schritten seine eigene Überlegung („warum lese ich das?").
  Geplant: Reasoning der Tool-Call-Iteration weiterreichen (lokal als Text;
  für Anthropic-Cloud als erhaltener signierter Thinking-Block plus
  Interleaved-Thinking-Beta-Header `interleaved-thinking-2025-05-14` in
  `anthropic.rs`). Vor Umsetzung: Provider-Serialisierung des `reasoning`
  verifizieren. Hoher Nutzen für die „weiter suchen oder antworten?"-
  Entscheidung, mittlerer Aufwand, Token-Kosten beachten.

### Bekannte Lücken — Code-Audit vom 2026-07-12

Ergebnis eines vollständigen Code-Durchgangs (Frontend, Chat-Pipeline,
LLM/Modelle, Tools/Skills, Agent-/Dateiverwaltung). Jeder Punkt ist so
beschrieben, dass er ohne weiteren Kontext umsetzbar ist. Referenzen als
`datei:zeile` (Stand dieses Commits).

**Priorität 1 — Korrektheit/Sicherheit:**

- **Sandbox-Seiteneffekt: `create_dir_all` läuft vor der Sandbox-Prüfung.**
  In `ensure_inside_sandbox` (`src-tauri/src/core/tool/tools/write_docx.rs:172-195`,
  `create_dir_all` bei Z. 181) und im Inline-Pendant in
  `src-tauri/src/core/tool/tools/append_to_md.rs:98-124` (Z. 108) wird das
  Parent-Verzeichnis angelegt, **bevor** geprüft wird, ob es im Agent-Ordner
  liegt. Übergibt das LLM einen absoluten Pfad, ersetzt `PathBuf::push` den
  ganzen Pfad (POSIX-Semantik) — es entstehen leere Verzeichnisse außerhalb
  der Sandbox (die Datei selbst wird nicht geschrieben). Verstößt gegen
  CLAUDE.md §2 Regel 4. Fix: erst Parent kanonisieren und gegen den
  Agent-Ordner prüfen, dann `create_dir_all`; dabei die duplizierte Logik
  (drei Varianten: `core/sandbox.rs::ensure_in_agent_folder`,
  `write_docx.rs::ensure_inside_sandbox`, Inline in `append_to_md.rs`) in
  `core/sandbox.rs` zentralisieren. Bestehende Tests in `sandbox.rs:31-113`
  grün halten, neue Fälle für nicht-existierende Zielpfade + absolute Pfade
  ergänzen.
- **HITL-Lücke beim Fan-out-Tool.** `requires_approval` von
  `delegate_into_xlsx_column`
  (`src-tauri/src/core/tool/tools/delegate_into_xlsx_column.rs:119-172`)
  gibt bei Fehlern während der Preview-Erzeugung (Workbook nicht lesbar,
  Profil nicht auflösbar — `?`/`.ok()?` bei Z. 122/146/148) `None` zurück.
  `None` bedeutet im Runner-Gate (`core/chat/run.rs:565-632`) „keine
  Freigabe nötig" — ein Massen-Lauf kann so ohne Bestätigungskarte starten,
  wenn die Preview scheitert, `execute` aber gelingt. Fix: bei
  Preview-Fehler eine minimale Fallback-Preview liefern (Tool-Name +
  Roh-Argumente) statt `None`; alternativ Signatur auf
  `Result<Option<HitlPreview>>` heben und Fehler ablehnen.

**Priorität 2 — fehlende Features (Backend teilweise fertig):**

- **Agent löschen hat kein UI.** Command `delete_agent`
  (`src-tauri/src/commands/agent.rs:46-50`) und Frontend-Binding
  `agentApi.delete` (`src/lib/tauri.ts:27`) existieren, werden aber nirgends
  aufgerufen. Fix: Lösch-Button mit Bestätigungsdialog im
  `AgentEditorDialog.tsx`; nach Löschen aktiven Agenten wechseln/leeren.
  Achtung: `AgentRepo::delete` (`core/agent.rs:301-308`) entfernt nur
  `agents/{id}.json` — der Chat-Verlauf `agents/{id}.chat.jsonl`
  (`core/chat/repo.rs:69-71`) bliebe als Waise liegen und muss mit gelöscht
  werden; ebenso ggf. der Watch auf den Agent-Ordner.
- **Chat-Verlauf lässt sich nicht löschen/zurücksetzen.** Kein Command, kein
  UI; `ChatRepo` kann nur `load`/`append` (`core/chat/repo.rs:73-106`).
  Fix: Command `clear_messages(agent_id)` (JSONL-Datei löschen oder
  truncaten), Binding in `tauri.ts`, Button im Chat-Header mit
  Bestätigung. Achtung: `FreshnessTracker::bootstrap_from_history`
  (`core/chat/freshness.rs:87-128`) liest die History beim ersten Run —
  nach dem Leeren den Tracker-State für den Agenten mit invalidieren.
- **User-Skills sind tote Infrastruktur.** `skills_user_dir()`
  (`core/storage.rs:34`) wird von `ensure_dirs()` angelegt (Z. 54), aber
  nie gelesen — es gibt nur `SkillRegistry::load_builtin()`
  (`core/skill/registry.rs:19`) und als einziges Command `list_skills`
  (`commands/skill.rs:8`). Entscheidung nötig: entweder `load_user()`
  implementieren (Scan + Frontmatter-Parsing wie builtin, Namenskollisionen
  definieren) oder Verzeichnis-Anlage entfernen, bis das Feature dran ist.
- **Custom-Provider verlangt API-Key auch für key-lose Endpunkte.**
  `send_message` prüft für jeden Provider außer `local` einen Key
  (`src-tauri/src/commands/chat.rs:28`) — ein lokaler Ollama-/vLLM-Server
  über den `custom`-Provider scheitert damit grundlos. Fix: `custom` vom
  Key-Gate ausnehmen (oder Dummy-Key erlauben); `CloudApisTab.tsx`
  entsprechend anpassen (Key-Feld für Custom optional machen).

**Priorität 3 — kleinere Lücken / Hygiene:**

- **Kontext-Dokumente werden nicht geprunt:** `prune_broken_attachments`
  (`src-tauri/src/core/watcher.rs:128-142`) räumt bei gelöschten Dateien nur
  `template_path` auf, nicht `context_paths` — tote Verweise bleiben am
  Agenten. Fix: `context_paths` in derselben Schleife mitprüfen.
- **HITL-Ablehnungsgrund ohne UI:** `reject_hitl` akzeptiert optionalen
  `reason` (`commands/chat.rs:64-75`), das UI ruft immer ohne auf
  (`src/App.tsx:377`). Fix: optionales Textfeld in `HitlCard.tsx`.
- **Per-Skill-/Per-Tool-HITL nur im Datenmodell:** `Agent.skill_settings` /
  `SkillHitl.per_tool` (`src/types/agent.ts:5-7`, `src/types/skill.ts:2`,
  Rust-Seite `core/agent.rs:90-93`) sind nirgends im UI konfigurierbar —
  nur der globale Schalter `hitlDisabled`. Entweder UI nachziehen oder
  Felder entfernen.
- **Kein Icon-Picker:** `Agent.icon` ist im `AgentEditorDialog.tsx` nicht
  änderbar (wird nur intern auf den Bestand zurückgesetzt).
- **Download-Resume fehlt:** `core/models/download.rs` lädt in `.partial`
  und löscht sie bei Abbruch/Fehler (Z. 137) — kein `Range`-Header, großer
  Download beginnt von vorn. Fix: `Range`-Request ab `.partial`-Größe,
  Server-Support (206) prüfen.
- **`write_docx`-Bullets sind kein echtes Word-Listenformat**, nur
  `"• "`-Textpräfix (`core/tool/tools/write_docx.rs:142-144`, dokumentiert
  „for v1"). Fix: echte Numbering-Definition via `docx-rs`.
- **Veralteter Doc-String in `read_file`:** „use dedicated tools for
  PDF/DOCX/XLSX instead (coming later)" (`core/tool/tools/read_file.rs:35`)
  — die Tools existieren längst; Schema-Beschreibung geht so ans LLM.
- **`ReasoningChip`-Labels hart deutsch codiert** („Denkt …"/„Gedanken",
  `src/components/chat/ReasoningChip.tsx:31`) statt über i18n.
- **Ungenutztes Binding:** `available_providers` (`src/lib/tauri.ts:139`)
  wird nirgends aufgerufen; Provider-Listen sind im UI hart codiert
  (`CloudApisTab.tsx`, `AgentEditorDialog.tsx`). Entweder nutzen oder
  Binding + Command entfernen.
- **Platzhalter `tool_calls_were_emitted`** liefert immer `false`
  (`core/llm/local_gguf.rs:605-607`, Aufrufer Z. 505) — die
  `FinishReason::ToolUse`-Erkennung hängt allein an den Events; bewusst so
  kommentiert, bei Umbau des Local-Providers auflösen.

**Test-Lücken:**

- Kein Integrationstest für den `react_loop` (Mock-`LlmProvider`, der
  Tool-Calls/HITL/AskUser durchspielt) — nur die Hilfsfunktionen sind
  getestet (`core/chat/run.rs:1200-1532`).
- `core/chat/repo.rs` (JSONL-Persistenz) ist komplett ungetestet
  (Append/Load-Roundtrip, korrupte Zeilen werden übersprungen).

## [0.2.0] — 2026-07-13

### Added
- **Workspace-Orientierung im System-Prompt.** Der Agent sieht pro
  User-Turn ein begrenztes, datiertes Inventar seines Ordners (`## Workspace`,
  eingerückter Baum mit Größe + Änderungsdatum). Damit beantwortet er Fragen
  über „das Projekt", ein Thema oder einen Zeitraum nicht mehr aus einer
  einzelnen schon im Kontext liegenden Datei, sondern sichtet erst die
  passenden Dokumente. Tiefe und Eintragszahl sind gedeckelt; Symlinks und
  Junk-Dateien bleiben außen vor.
- **Gründlichkeits-Regel im System-Prompt.** Eine immer vorhandene,
  modellunabhängige Zeile weist den Agenten an, bei Fragen zu Dateien,
  Thema, Projekt oder Zeitraum erst Belege zu sammeln (Workspace sichten,
  relevante Dokumente lesen), nicht aus einer einzelnen Datei zu antworten
  (außer benannt) und nach jedem Lesen zu prüfen, ob die Informationslage
  reicht — sonst weitersuchen statt vorschnell zu antworten.
- **Agent löschen.** Der Agent-Editor bietet im Bearbeiten-Modus einen
  Lösch-Button mit Bestätigungsdialog; das Backend räumt den zugehörigen
  Chat-Verlauf mit ab (`delete_agent` + `ChatRepo::delete`).
- **Unterhaltung zurücksetzen.** Neuer Radiergummi-Button neben dem
  Agent-Umschalter leert den persistierten Verlauf eines Agenten
  (Bestätigungsdialog, neuer Command `clear_chat_history`). Dateien und
  Agent-Konfiguration bleiben unberührt.
- **Standard-Provider/-Modell umschaltbar.** „Als Standard verwenden"-Aktionen
  auf allen Cloud-Provider-Karten und lokalen Modell-Karten; neue Liste
  „Weitere installierte Modelle" für Custom-URL-Downloads (vorher weder
  löschbar noch als Standard wählbar). Vorher war der Default nach dem
  ersten Setup faktisch eingefroren.
- **Icon-Picker im Agent-Editor.** 16 kuratierte Icons; bisher gab es das
  `icon`-Feld nur im Datenmodell, jeder Agent blieb „Bot".
- **Datums-Trenner im Chat** („Heute", „Gestern", Datum) zwischen Nachrichten
  verschiedener Tage.
- **Freundliche Fehlermeldungen im Chat.** Provider-Fehler werden in
  verständliche Kategorien übersetzt (API-Key, Rate-Limit, Modell nicht
  gefunden, Kontext zu lang, überlastet, Netzwerk) mit passender Aktion;
  Roh-Fehler bleibt hinter „Technische Details" erreichbar. Nutzer-Abbruch
  erscheint nicht mehr als Fehler.
- **Klartext-Tool-Labels.** Tool-Chips und HITL-Karte zeigen lokalisierte
  Labels („Datei lesen") statt roher Tool-Namen; der rohe Name bleibt als
  Tooltip.

### Changed
- **Enter sendet** (Shift+Enter = neue Zeile, IME-sicher) im Chat-Eingabefeld
  und in der Agent-Rückfrage-Karte; ⌘/Ctrl+Enter funktioniert weiterhin.
- **Auto-Scroll folgt nur noch am Ende.** Wer während des Streamings
  hochscrollt, wird nicht mehr ans Ende gezogen.
- **Sidebar-Kopfzeile entzerrt.** Agent-Bearbeiten ist jetzt ein Stift statt
  eines zweiten Zahnrads; die App-Einstellungen sitzen unten in der Sidebar.
- **Banner-Aktion passt zum Grund.** „Kein Agent" → Agent anlegen,
  „Modell fehlt" → Modelle-Tab, „API-Key fehlt" → Cloud-Tab (vorher immer
  Cloud-Tab).
- **Erweiterte Agent-Optionen eingeklappt.** „Schreiben ohne Rückfrage" und
  „Hintergrund-Worker" liegen hinter „Erweitert"; aktivierte HITL-Umgehung
  wird amber hervorgehoben und der Abschnitt öffnet dann automatisch.
- Diverse hartkodierte UI-Strings (Reasoning-Chip, Tool-Chip-Detailfelder,
  „Override", „Default"-Badge) laufen jetzt über i18n; HTML-Titel der App
  von „Tauri + React + Typescript" auf „ProcessFox" korrigiert; doppelte
  Deaktiviert-Meldung (Banner + Placeholder) entfernt.

## [0.1.1] — 2026-05-16

### Changed
- **Kontext-Dokumente werden im Chat-Input verwaltet.** Der Block im
  „Agenten bearbeiten"-Modal ist entfallen; stattdessen erscheint links neben
  dem Vorlage-Icon ein eigenes Buch-Icon (`BookOpen`), das ein Popover mit
  der Liste der angehängten Docs und „Dokument hinzufügen" öffnet. Mehrere
  Dokumente werden direkt im Picker oder durch wiederholtes Hinzufügen
  ergänzt; Einzelentfernen via X-Button im Popover.
- **Skill `chat-context` ist jetzt ein echter History-Toggle.** Bisher
  beeinflusste der Skill nur einen Hinweis im System-Prompt, der Verlauf
  wurde immer mitgeschickt. Ab v0.1.1 gilt: **Skill aus → kein Verlauf,
  nur die aktuelle User-Nachricht** an das LLM. Nützlich für stateless-Tasks
  (Übersetzung, einmalige Q&A) und konstante Token-Kosten in langen
  Sitzungen. Skill an → wie bisher die letzten 20 Turns. Beachten: Agenten,
  bei denen der Skill explizit aus war, verlieren mit diesem Update den
  bisherigen impliziten Verlauf; einfach den Skill aktivieren, wenn das
  unerwünscht ist.

### Improved
- **Auto-Re-Read von Kontext-Dokumenten:** Wenn ein angehängtes Dokument
  durch das History-Window-Trimming (max. 20 Turns) aus dem LLM-sichtbaren
  Verlauf gefallen ist, bekommt das LLM jetzt vor der Antwort einen kurzen
  Hinweis, die betroffenen Docs erneut zu lesen. Verhindert
  „Halluzinationen aus dem Gedächtnis" bei langen Konversationen.

### Fixed
- **macOS:** Bei unsignierten Builds wirft macOS „ProcessFox ist beschädigt"
  statt des erwarteten Gatekeeper-Dialogs („nicht identifizierter Entwickler").
  Ursache: Browser hängen ein `com.apple.quarantine`-Attribut an die DMG, und
  für unsignierte Binaries macht macOS daraus die „beschädigt"-Meldung. Der
  bisher in den Release-Notes empfohlene Weg über Systemeinstellungen →
  Datenschutz & Sicherheit → „Trotzdem öffnen" funktioniert dafür nicht.
  Workaround:
  ```
  xattr -dr com.apple.quarantine /Applications/ProcessFox.app
  ```
  Release-Notes ab v0.1.1 angepasst.

## [0.1.0] — 2026-05-16

Erste öffentliche Preview. Vollständige Release-Notes:
https://github.com/ProcessFox/processfox/releases/tag/v0.1.0
