# ProcessFox

**Lokale KI-Agenten für Einsteiger.**

ProcessFox ist eine plattformübergreifende Desktop-App (macOS, Windows, Linux), die kleinen Unternehmen, NGOs und Einzelnutzer:innen den Einstieg in die lokale Nutzung von KI-Sprachmodellen erleichtert. Statt eines komplexen Workflow-Builders setzt ProcessFox auf einfache, agentische Assistenten, die in einem vom Nutzer gewählten Ordner arbeiten und dort mit Dokumenten (DOCX, PDF, XLSX, CSV, MD, TXT) umgehen können.

Die App orientiert sich am Bedien-Paradigma von Obsidian: linke Sidebar mit Datei-Baum und Agenten-Dropdown, Chat als zentraler Interaktionsraum, klickbare Datei-Vorschau. Alle Daten bleiben standardmäßig lokal auf dem Rechner des Nutzers.

## Kern-Prinzipien

- **Lokal zuerst.** Lokale LLMs im GGUF-Format sind der Standard. Cloud-APIs (Anthropic, OpenAI, OpenRouter) sind optional hinterlegbar.
- **Agent statt Thread.** Die App kennt keine Chat-History-Sidebar. Alles lebt in benannten Agenten mit eigenem Ordner, Modell und Skill-Set.
- **Skills statt Workflows.** Fähigkeiten sind atomar und werden vom Agenten selbst ausgewählt — keine Prozessketten, die der Nutzer selbst bauen muss.
- **Einsteiger im Fokus.** Ein Einsteiger soll innerhalb von 5 Minuten nach Installation seine Dateien mit einem LLM bearbeiten können.
- **Regulatorisch vertretbar.** Strikte Ordner-Sandbox, kein Netzwerk-Zugriff für Skills in v1 außer den konfigurierten LLM-Endpunkten.

## Technologie

- Desktop: **Tauri v2**
- Frontend: **React + Vite + TypeScript**
- Backend: **Rust** (pure Rust, keine Python-Abhängigkeit)
- Lokale LLM-Runtime: **llama.cpp via `llama-cpp-2`** (Rust-Bindings mit nativem Tool-Calling über Chat-Templates)
- Distribution: **GitHub Releases** mit Auto-Updater via GitHub Actions

## Status

Phasen 1–4 sind abgeschlossen, Phase 5 (Polish & Onboarding) ist weit fortgeschritten. Siehe [CONCEPT.md](CONCEPT.md) für die vollständige Vision und die [Roadmap auf www.processfox.ai/docs](https://www.processfox.ai/docs/entwickler/roadmap/) für die Phasen.

## Dokumentation

Architektur, Skill-Beschreibungen und LLM-Kompatibilität leben in der Doku-Webseite unter [www.processfox.ai/docs](https://www.processfox.ai/docs/). Die Quell-Markdowns liegen im separaten Landing-Repo (lokal: `landing/src/content/docs/docs/`, im App-Repo gitignored).

## Schnellstart für Mitentwickler

### Build-Voraussetzungen

Die lokale GGUF-Runtime kompiliert llama.cpp aus C++ — entsprechend ein paar
einmalige Setup-Schritte:

**macOS** (Apple Silicon empfohlen):
- **Xcode + Command Line Tools** installiert
- **Metal-Toolchain**: `sudo xcodebuild -downloadComponent MetalToolchain`
- **Homebrew + cmake**: `brew install cmake`

**Linux**:
- `build-essential`, `cmake`, `pkg-config`

**Windows**:
- Visual Studio 2022 mit C++ Build-Tools
- cmake im PATH

### Dev-Server

```bash
npm install
npm run tauri dev
```

Erster Build kompiliert llama.cpp inklusive Metal-Kernels (~10 Min). Danach
ist der Cache warm und Iteration ist schnell.

Siehe [CLAUDE.md](CLAUDE.md) für Arbeits-Anweisungen, wenn du Claude Code zur Entwicklung nutzt.

## Lizenz

MIT — siehe [LICENSE](LICENSE).
