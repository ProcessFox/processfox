# ProcessFox — Konzept v1.0

Stand: April 2026
Zielgruppe dieses Dokuments: Projektinhaber, Entwicklerinnen, Beta-Tester:innen, und Claude Code als Codier-Assistent.

## 1. Vision & Elevator Pitch

**Lokale KI-Agenten für Einsteiger.**

ProcessFox ermöglicht nicht-technischen Personen (insbesondere Geschäftsführer:innen kleiner Unternehmen und Verantwortlichen in NGOs) den einfachen und datenschutzfreundlichen Einsatz lokaler KI-Sprachmodelle für alltägliche Dokumentenarbeit. Die App nimmt dem Nutzer die Komplexität von Prozessketten ab und bietet stattdessen agentische Assistenten, die selbstständig aus einer kuratierten Menge an Fähigkeiten auswählen.

## 2. Zielgruppe

### Persona: Christian, Geschäftsführer eines kleinen Unternehmens

- Arbeitet täglich mit DOCX- und XLSX-Dokumenten.
- Hat ChatGPT ausprobiert, traut sich aber aus Datenschutz- und Compliance-Gründen nicht, sensible Unternehmensdaten in Cloud-Dienste zu geben.
- Muss regulatorische Vorgaben (DSGVO, branchenspezifische Anforderungen) einhalten.
- Möchte wiederkehrende Aufgaben teilweise automatisieren, ohne Programmieren zu lernen.
- Will keine 20 Tools bedienen — ein einziges App-Fenster, das "funktioniert", ist ideal.

### Referenz-Anwendungsfälle

1. **Ausschreibungs-Analyse.** 10 PDFs mit komplizierten Vorgaben liegen in einem Ordner. ProcessFox durchsucht sie und erstellt daraus ein strukturiertes Memo.
2. **Mitgliederliste pflegen.** In einer XLSX-Datei sind Zellen unvollständig. ProcessFox identifiziert Lücken und macht Vorschläge zur Ergänzung — der Nutzer bestätigt jede Änderung per Inline-Diff.
3. **Angebot aus E-Mail.** Der Nutzer fügt eine Kunden-E-Mail ein, ProcessFox erstellt daraus auf Basis einer DOCX-Vorlage ein fertiges Angebot.

## 3. Produkt-Prinzipien

1. **Lokal zuerst.** Lokale Modelle sind Default. Cloud ist optional, nicht erwartet.
2. **Agent statt Thread.** Keine Chat-History-Sidebar. Agenten sind die persistenten Einheiten.
3. **Skills statt Workflows.** Der Nutzer baut keine Prozesse, er wählt Fähigkeiten aus.
4. **Ordner statt Datenbank.** Arbeit findet in realen Dateien im Dateisystem statt, nicht in proprietären Datenstrukturen.
5. **Transparenz vor Autonomie.** Der Agent zeigt Live-Status seiner Tool-Calls; schreibende Aktionen laufen standardmäßig durch eine menschliche Freigabe (Inline-Diff).
6. **Einfach vor vollständig.** Wenn ein Feature v1 komplexer macht als nötig, fliegt es raus.

## 4. Taxonomie: Tool, Skill, Agent

ProcessFox verwendet eine konsistente dreistufige Taxonomie:

### Tool
Kleinste ausführbare Einheit. Deterministisch, atomar, in Rust implementiert, per Function Calling vom LLM aufrufbar. Beispiele:
- `list_folder`, `read_file`, `grep_in_files`
- `read_xlsx_cell`, `read_xlsx_range`, `update_xlsx_cell`
- `write_docx`, `append_to_md`, `read_docx`
- `llm_extract_structured`
- `ask_user` (für HITL)

### Skill
Kuratierte Gruppe von Tools + Anleitung in einer `SKILL.md`-Datei (Format orientiert sich an Claude Code Skills). Ein Skill hat Frontmatter mit Name, Beschreibung, Trigger, Tool-Liste und HITL-Flags, plus Markdown-Prompt mit Anleitung für das Modell. Skills können Scripts und Templates referenzieren.

### Agent
Benannte Einheit mit Persönlichkeit und Arbeitsumgebung. Besteht aus: Name, Avatar-Icon, System-Prompt, zugewiesener Ordner, gewähltes Modell (lokal oder Cloud), Liste aktivierter Skills. Der Nutzer erstellt und pflegt Agenten über eine UI.

**Sichtbarkeit im UI:** Unter dem Agenten-Namen erscheinen kleine Icons für jeden aktiven Skill, damit der Nutzer auf einen Blick sieht, wozu der Agent fähig ist.

## 5. UI-Modell

### Hauptfenster (Obsidian-Vorbild)

```
┌───────────────────────────────────────────────────────────────┐
│ [Agent-Dropdown: ▾ Angebots-Assistent]                        │
│ [📋 📝 📊 💾] (aktive Skill-Icons)                             │
├──────────────┬────────────────────┬───────────────────────────┤
│              │                    │                           │
│ Datei-Baum   │ Datei-Preview /    │ Chat mit Agent            │
│ des Agent-   │ Editor (wenn       │                           │
│ Ordners      │ Datei geklickt)    │                           │
│              │                    │                           │
│ 📄 offer.md  │ # Angebot          │ User: Erstelle ein        │
│ 📊 list.xlsx │ ...                │ Angebot aus dieser Mail   │
│ 📁 pdfs/     │                    │                           │
│              │                    │ Agent: 🔧 read_email ...  │
│              │                    │ Agent: 🔧 write_docx ...  │
│              │                    │ [Inline-Diff-Karte]       │
│              │                    │ [Freigeben] [Ablehnen]    │
└──────────────┴────────────────────┴───────────────────────────┘
```

- **Links:** Agenten-Dropdown oben (Vault-Analog bei Obsidian). Darunter der Datei-Baum des Agent-Ordners. Unten rechts ein kleines Zahnrad-Icon für Agent-Einstellungen.
- **Mitte (optional):** Datei-Preview bzw. Editor erscheint erst, wenn der Nutzer eine Datei im Baum anklickt. TXT und MD editierbar; PDF, Bild, DOCX, XLSX als Vorschau. Panel-Handling per Drag-Handle.
- **Rechts:** Chat mit dem aktiven Agenten. Endlose Konversation pro Agent (Chat-Historie wird persistiert). Status-Chips unter Agent-Nachrichten zeigen Tool-Calls live. Inline-Diff-Karten erscheinen bei Freigabe-Anfragen.

### Dialoge und Modale

- **Settings-Modal:** Modelle, Cloud-APIs, Sprache, Theme, About. Öffnet sich zwingend beim allerersten Start mit Fokus auf "Modell herunterladen".
- **Agent-Editor:** Modal (oder Side-Panel) zum Bearbeiten von Name, Ordner, System-Prompt, Modell, aktive Skills.
- **Skill-Editor:** Modal (oder eigene View) zum Erstellen eigener Skills via Formular (nicht via Markdown-Editor in v1).

### Leer-Zustände

- **Kein Ordner gewählt:** Linke Sidebar zeigt großen Call-to-Action "Ordner wählen", Chat ist ausgegraut/deaktiviert.
- **Kein Modell geladen:** Chat ist deaktiviert, Banner oben "Lade ein Modell in den Einstellungen".
- **Neuer Agent, leerer Chat:** Kleine Starter-Chips im Chat ("Probier mal: Fasse mir alle PDFs zusammen").

## 6. Agentisches Verhalten

### Ausführungs-Modell: ReAct-Loop

1. Nutzer sendet Nachricht.
2. LLM erhält: System-Prompt + aktive Skill-Beschreibungen + Chat-Verlauf + User-Message.
3. LLM entscheidet: direkte Antwort, oder Tool-Call?
4. Wenn Tool-Call: Backend führt ihn aus, Resultat geht an LLM zurück.
5. Schritte 3–4 wiederholen sich bis zur finalen Antwort oder bis Max-Iterationen (konfigurierbar, Default 12) erreicht sind.
6. Status-Chips werden pro Tool-Call live im Chat angezeigt.

### Human-in-the-Loop

- Das HITL-Verhalten ist **skill-definiert**, nicht global.
- Lesende Skills brauchen nie Freigabe.
- Schreibende Skills haben zwei Varianten oder ein Flag: "mit Rückfrage" (zeigt Inline-Diff vor Ausführung) und "ohne Rückfrage" (führt direkt aus, aber zeigt Ergebnis prominent).
- Beispiel: "Tabelle aktualisieren (mit Rückfrage)" ist Default. Ein erfahrener Nutzer kann pro Agent auf die "ohne"-Variante umschalten.

### Freigabe-Darstellung

- **Inline-Diff-Karte** im Chat. Zeigt bei Datei-Änderungen den konkreten Unterschied (vorher/nachher).
- Buttons: **Freigeben**, **Ablehnen**, **Anpassen** (öffnet Editor).
- Bei Neuen Dateien: Vorschau des vollen Inhalts.
- Bei XLSX-Änderungen: Liste der geplanten Zell-Änderungen (Zeile/Spalte, alter Wert → neuer Wert).

### Berechtigungen

- **Dateisystem:** Agent darf ausschließlich im konfigurierten Agent-Ordner lesen/schreiben. Außerhalb: strikt verboten. Versuche müssen im Backend geblockt werden, nicht erst im LLM-Prompt.
- **Netzwerk:** Skills dürfen in v1 keine Web-Requests machen. Einzige Ausnahme: der konfigurierte LLM-Endpunkt (lokal oder Cloud).
- **Code-Ausführung:** In v1 dürfen keine User-Scripts laufen. Für eingebaute Skills, die Scripts benötigen, wird eine Sandbox vorbereitet (Whitelist-Modell-Sandbox in Rust), aber noch nicht für Dritte geöffnet.

### Lange laufende Aufgaben

- Tool-Calls mit Fortschritts-Anzeige als Status-Chips im Chat.
- Nutzer kann in andere Agenten wechseln, während ein Agent arbeitet.
- Nutzer kann die App verlassen (Hintergrund-Fertigstellung je nach OS unterstützt — für Tauri via System-Tray oder Notification).

## 7. Datenmodell

### Agenten-Ordner-Struktur (User-Seite)

```
~/MeinAgentenOrdner/
├── (Nutzerdateien — XLSX, DOCX, PDF, MD, TXT, ...)
└── (Gedächtnis-Dokumente liegen wo der Nutzer sie ablegt,
    z. B. im Root, als ausgewählte MD-Datei)
```

**Wichtig:** ProcessFox legt keine versteckten Metadaten-Ordner im Agent-Ordner an. Der Ordner gehört dem Nutzer.

### App-Support-Ordner (ProcessFox-Seite)

Alle App-Metadaten liegen in einem zentralen Anwendungsordner, plattformspezifisch:
- macOS: `~/Library/Application Support/ProcessFox/`
- Windows: `%APPDATA%/ProcessFox/`
- Linux: `~/.config/ProcessFox/` (XDG)

```
ProcessFox/
├── agents/
│   ├── <uuid>.json                 # Agent-Definition
│   └── <uuid>.chat.jsonl           # Chat-Verlauf des Agenten
├── skills/
│   ├── builtin/                    # mit App ausgeliefert, readonly
│   │   ├── folder-search/
│   │   │   ├── SKILL.md
│   │   │   └── (Scripts, Templates, ...)
│   │   └── ...
│   └── user/                       # User-erstellte Skills
│       └── ...
├── models/
│   ├── catalog.json                # kuratierte HF-Liste, wird pro App-Version aktualisiert
│   └── downloads/                  # heruntergeladene GGUF-Dateien
├── settings.json                   # globale App-Einstellungen
└── logs/
    └── processfox.log
```

### Agent-Schema (`<uuid>.json`)

```json
{
  "id": "uuid-v4",
  "name": "Angebots-Assistent",
  "icon": "📝",
  "folder": "/Users/.../MeinAgentenOrdner",
  "systemPrompt": "Du bist ein Assistent für ...",
  "model": {
    "type": "local",
    "id": "google/gemma-4-e4b-gguf:Q4_K_M"
  },
  "skills": [
    "folder-search",
    "document-read",
    "document-create-docx",
    "document-extend",
    "chat-context"
  ],
  "skillSettings": {
    "document-create-docx": { "hitl": true }
  },
  "createdAt": "2026-04-23T10:00:00Z",
  "updatedAt": "2026-04-23T11:30:00Z"
}
```

### SKILL.md Frontmatter-Schema

```yaml
---
name: folder-search
title: Ordner durchsuchen
description: Durchsucht Dateien im Agent-Ordner nach Inhalt und gibt relevante Stellen zurück.
icon: 🔍
tools:
  - list_folder
  - read_file
  - grep_in_files
  - llm_extract_structured
hitl:
  default: false
  per_tool: {}
language: en
---
```

Der Body der SKILL.md enthält die englische Anleitung für das Modell, typischerweise mit der Anweisung "Antworte in der Sprache des Nutzers".

## 8. Skill-Inventar v1 (9 Skills)

Volle Skill-Definitionen liegen in der öffentlichen Doku unter [www.processfox.ai/docs/skills](https://www.processfox.ai/docs/skills/) (Quellen im separaten Doku-Repo). Kurzübersicht:

### Lesende Skills

1. **folder-search** — Ordner durchsuchen (list + grep + extract)
2. **document-read** — Einzelnes Dokument lesen und zusammenfassen (PDF, DOCX, MD, TXT)
3. **table-read** — XLSX/CSV lesen und abfragen

### Schreibende Skills (mit konfigurierbarem HITL)

4. **document-create-docx** — Neues DOCX-Dokument erstellen (optional Template-basiert)
5. **document-edit** — Bestehendes DOCX/MD/TXT bearbeiten (Ersatz oder Teil-Update)
6. **document-extend** — An bestehendes Dokument anhängen (Append, für Log/Gedächtnis)
7. **table-update** — XLSX/CSV-Zellen aktualisieren

### Kontext-Skills

8. **chat-context** — Früheren Gesprächsverlauf des aktuellen Chats im Kontext berücksichtigen
9. **context-document-read** — Definierte Kontext-Dokumente (z. B. Unternehmens-Infos, Kunden-Liste) automatisch vor Auftragsbearbeitung lesen

## 9. Modell-Strategie

### Lokal-first, Cloud optional

- **Default:** Lokales GGUF-Modell, ein aktives Modell zur Zeit.
- **Cloud-API:** Anthropic, OpenAI, OpenRouter — konfigurierbar pro Agent, API-Keys werden sicher im OS-Keychain gespeichert (Tauri `@tauri-apps/plugin-stronghold` oder Rust-`keyring`).

### Modell-Katalog

- In der App ausgeliefert als kuratierte JSON-Liste (`models/catalog.json`).
- Pro App-Release aktualisiert.
- Bei Katalog-Update: stilles Einblenden. Wenn das aktuell gewählte Modell nicht mehr empfohlen wird, erhält der Nutzer eine dezente Mitteilung.

### Download-Flow

- Nutzer wählt aus Liste oder gibt eigene HuggingFace-URL ein.
- Validierung: URL zeigt auf GGUF-Datei.
- Download mit Fortschrittsbalken im Settings-Modal.
- Speicherung unter `models/downloads/<modell>.gguf`.

### Hardware-Check beim First-Run

- Detektiert RAM (und VRAM, sofern möglich).
- Schlägt passendes Modell vor:
  - ≤ 8 GB RAM: Gemma 4 E2B oder kleiner
  - 12–16 GB RAM: Gemma 4 E4B
  - 32 GB+ RAM: Gemma 4 26B MoE

### JSON-Cleanup-Layer

Da Gemma 4 E2B/E4B Tool-Calling-Output nicht immer sauber im JSON-Format liefert, hat ProcessFox einen Post-Processing-Layer:
- **Constrained Decoding** im Runtime, sofern möglich (Grammar-Sampling).
- **Regex-/Parser-basiertes Cleanup** als Fallback: Extraktion von JSON-Blöcken aus Fließtext, Reparatur typischer Syntaxfehler (fehlende Kommas, nicht-geschlossene Strings), JSON-Schema-Validierung mit Retry.

## 10. Tech-Stack

| Bereich | Technologie |
|---|---|
| Desktop-Framework | Tauri v2 |
| Frontend | React 18+ / Vite / TypeScript |
| UI-Styling | Tailwind CSS + shadcn/ui |
| Datei-Baum | `react-arborist` oder `@minoru/react-dnd-treeview` |
| Markdown-Editor | CodeMirror 6 |
| PDF-Preview | `pdfjs-dist` |
| DOCX-Preview | `mammoth.js` (Browser-seitig zu HTML) |
| XLSX-Preview | SheetJS (`xlsx`) |
| Backend | Rust (Tauri-Plugins + eigene Commands) |
| LLM-Runtime | Rust-nativ (candle oder mistral.rs — Entscheidung in Phase 2) |
| XLSX lesen | `calamine` |
| XLSX schreiben | `rust_xlsxwriter` |
| DOCX | `docx-rs` / `docx-rust` (Template-Platzhalter-Ansatz bevorzugt) |
| PDF | `pdfium-render` |
| CSV | `csv` |
| Sichere Speicherung | Tauri Stronghold oder Rust `keyring` |
| Distribution | GitHub Releases (manuell) + Apple App Store / Microsoft Store (Einmalzahlung) |
| CI/CD | GitHub Actions |

## 11. Sprache & Lokalisierung

- **UI-Sprache v1:** Deutsch (fix, nicht wechselbar).
- **Skill-Descriptions:** Englisch (kleinere Modelle folgen englischen Instruktionen zuverlässiger).
- **Agent-Antworten:** Englische Skills enthalten den Standard-Hinweis "Antworte in der Sprache, die der Nutzer verwendet hat".
- **Onboarding-Hinweis:** Bei Modell-Auswahl erscheint der Hinweis "Für bessere deutsche Antwortqualität empfehlen wir Gemma 4 E4B oder größer".

## 12. Distribution & Monetarisierung

- **Plattformen:** macOS (Universal: Apple Silicon + Intel), Windows x64, Linux (AppImage und .deb).
- **Release-Kanal:** nur Stable.
- **Lizenz:** GNU GPL v3 — erlaubt explizit den Verkauf von Kopien.
- **Zwei Vertriebswege:**

| Kanal | Updates | Preis |
|---|---|---|
| **GitHub Releases** (`.dmg`, `.msi`, `.AppImage`) | Manuell — kein „Neue Version verfügbar"-Banner, kein Auto-Updater | Kostenlos |
| **Apple App Store / Microsoft Store** | Auto-Update via Store-Mechanismus | Einmalzahlung |

- **Kein eigener Tauri-Updater.** Die App enthält keinen Update-Check und keine Update-Benachrichtigung. Store-Versionen nutzen den nativen Store-Update-Mechanismus.
- **Code-Signing:** Für Store-Einreichung erforderlich (Apple Developer Program, Windows-EV-Zertifikat). GitHub-Downloads sind in v1.0 unsigniert (User akzeptieren Sicherheits-Warnung; dokumentieren im README).
- **GPL-Kompatibilität:** Quellcode bleibt öffentlich auf GitHub. Jeder darf forken und selbst bauen — der Store-Preis ist eine Convenience-Gebühr für automatische Updates und verifizierte Distribution, kein Zugangs-Gate zum Code.

## 13. MVP-Scope v1.0

### Drin

- Tauri-App für macOS / Windows / Linux.
- Obsidian-artiges Layout mit Agenten-Dropdown, Datei-Baum, Datei-Preview, Chat.
- Agenten-Modell (mehrere Agenten, pro Agent ein Ordner, Modell, Skills).
- 9 vorinstallierte Skills (siehe Abschnitt 8).
- ReAct-Loop mit skill-definiertem HITL, Inline-Diff-Karten, Live-Status-Chips.
- Lokale LLM-Runtime (Rust, GGUF).
- Kuratierter HF-Modell-Katalog + eigener Link.
- Cloud-API-Hinterlegung optional (Anthropic, OpenAI, OpenRouter).
- Sandbox für eingebaute Skill-Scripts (nicht für User-Scripts).
- Einstellungen-Modal (Modelle, APIs, Theme, About).
- Sprache: Deutsch.

### Nicht drin (Roadmap für spätere Versionen)

- Workflow-Builder, eigene Datenbank.
- E-Mail-Postfach-Anbindung.
- Audio / Whisper / Transkription.
- Web-Skills, externe APIs außer LLM-Anbietern.
- Skill-Marketplace / Community-Sharing.
- Multi-Agenten-Kollaboration.
- Auto-Komprimierung langer Chats.
- Weitere Sprachen außer Deutsch.
- User-Scripts in der Sandbox.
- Eigene Skills via UI-Formular erstellbar (kommt in späterer Version).
- Code-Signing (kommt in v1.1).
- OCR auf gescannten PDFs.

## 14. Erfolgskriterien

- **Primär:** Ein Einsteiger kann innerhalb von 5 Minuten nach Installation seine Dateien mit einem LLM bearbeiten.
- **Sekundär:** Die 3 Referenz-Use-Cases (PDFs → Memo, Excel-Lücken, E-Mail → Angebot) funktionieren mit Gemma 4 E4B lokal in akzeptabler Qualität.
- **Technisch:** App-Start in ≤ 3 Sekunden, Modell-Ladezeit ≤ 30 Sekunden bei warm cache, Tool-Call-Latenz ≤ 1 Sekunde (ohne LLM-Zeit).

## 15. Offene Entscheidungen

Diese Punkte sind bewusst offen gelassen und werden im Verlauf der Entwicklung entschieden:

- **Rust-Runtime final:** `candle` (HuggingFace-native, ggml/gguf-Support wächst) oder `mistral.rs` (sehr gute Tool-Calling-Unterstützung, bessere GGUF-Kompatibilität). Im ersten Prototypen beides kurz testen.
- **Tool-Calling-Robustheit mit Gemma 4 E2B/E4B:** Muss real gemessen werden. Falls untragbar, Fallback auf E4B als Mindest-Modell.
- **DOCX-Ausgabequalität:** Falls `docx-rs` an Grenzen stößt, Evaluation von Template-basierter Strategie mit `minijinja` + XML-Patching.
- **Code-Signing-Kosten:** Apple Developer (~99 $/Jahr), Windows-EV-Zertifikat (200–400 $/Jahr) — einzuplanen vor v1.1.

## 16. Referenzen & Inspiration

- **Obsidian** — Datei-zentriertes Arbeiten, Vault-Konzept, UI-Paradigma.
- **Claude Cowork** — lokaler Ordner, Skill-basiertes Arbeiten, Human-in-the-Loop-Modell.
- **Claude Code** — SKILL.md-Format mit Frontmatter, Tool-Aufruf-Muster, Permission-Modell.
- **OpenClaw** — agentisches Verhalten im lokalen Kontext.
