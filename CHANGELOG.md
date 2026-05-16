# Changelog

Alle nennenswerten Änderungen an ProcessFox werden hier festgehalten.

Format: [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).
Versionsschema: [Semantic Versioning](https://semver.org/lang/de/).

## [Unreleased]

### Changed
- **Kontext-Dokumente werden im Chat-Input verwaltet.** Der Block im
  „Agenten bearbeiten"-Modal ist entfallen; stattdessen erscheint links neben
  dem Vorlage-Icon ein eigenes Buch-Icon (`BookOpen`), das ein Popover mit
  der Liste der angehängten Docs und „Dokument hinzufügen" öffnet. Mehrere
  Dokumente werden direkt im Picker oder durch wiederholtes Hinzufügen
  ergänzt; Einzelentfernen via X-Button im Popover.

### Improved
- **Auto-Re-Read von Kontext-Dokumenten:** Wenn ein angehängtes Dokument
  durch das History-Window-Trimming (max. 20 Turns) aus dem LLM-sichtbaren
  Verlauf gefallen ist — oder wenn der Skill `chat-context` deaktiviert ist
  und das Modell ältere Turns ohnehin nicht referenzieren soll — bekommt
  das LLM jetzt vor der Antwort einen kurzen Hinweis, die betroffenen Docs
  erneut zu lesen. Verhindert „Halluzinationen aus dem Gedächtnis" bei
  langen Konversationen und in stateless-artigen Modi.

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
