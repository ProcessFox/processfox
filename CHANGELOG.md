# Changelog

Alle nennenswerten Änderungen an ProcessFox werden hier festgehalten.

Format: [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).
Versionsschema: [Semantic Versioning](https://semver.org/lang/de/).

## [Unreleased]

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
