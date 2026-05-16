# Changelog

Alle nennenswerten Änderungen an ProcessFox werden hier festgehalten.

Format: [Keep a Changelog](https://keepachangelog.com/de/1.1.0/).
Versionsschema: [Semantic Versioning](https://semver.org/lang/de/).

## [Unreleased]

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
