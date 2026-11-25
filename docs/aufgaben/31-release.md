# 31 – Release-Pipeline für den Client (macOS, Windows, Linux)

Ein Tag `vX.Y.Z` baut den Client `quiver` (Crate `quiver-fenster`) für alle Plattformen und legt ein
GitHub-Release an. Das Server-Abbild macht weiter `abbild.yml`.

## Ablauf (`.github/workflows/release.yml`)

- Auslöser: Tag `v*`. Zusätzlich `workflow_dispatch` und Pull Requests, die Pipeline-Dateien ändern:
  dann nur bauen und als Artefakt ablegen, **kein** Release.
- Vorab prüfen: Tag = `workspace.package.version`, sonst Abbruch. `cargo … --locked`.
- Baut nativ auf dem jeweiligen Läufer (kein Cross-Compile):

| Plattform | Läufer | Pakete |
|---|---|---|
| macOS arm64 | macos-latest | `.dmg` mit `quiver.app` |
| macOS x86_64 | macos (Intel-Läufer) | `.dmg` mit `quiver.app` |
| Windows x86_64 | windows-latest | `.msi` und portables `.zip` |
| Linux x86_64 | ubuntu-22.04 (alte glibc) | `.deb`, `.rpm`, `AppImage`, `.tar.gz` |
| Linux aarch64 | ubuntu-22.04-arm | `.deb`, `.rpm`, `.tar.gz` (AppImage, wenn einfach) |

- Danach ein Job: `SHA256SUMS`, GitHub-Release mit allen Dateien, Build-Herkunft per
  `actions/attest-build-provenance`. Nur dieser Job hat `contents: write`.
- `permissions: {}` als Standard, Rechte je Job so klein wie möglich, `persist-credentials: false`,
  keine `pull_request_target`, keine Nutzereingaben direkt in `run` (immer über `env`).
- Submodule mit auschecken (`submodules: recursive`).

## Signieren (optional, ohne Geheimnisse im Repo)

- macOS: sind `MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PASSWORD`, `MACOS_SIGNING_IDENTITY`, `APPLE_ID`,
  `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` gesetzt, wird signiert und notarisiert, sonst unsigniert
  (ad hoc) gebaut und das steht im Release-Text. Bei Pull Requests nie auf Geheimnisse zugreifen.
- Windows: unsigniert, im Release-Text vermerkt.
- Schlüsselbund und Zertifikat werden am Ende gelöscht, auch bei Fehlern.

## Pakete

- Symbole aus `assets/marke` (vorhandene Dateien, kein neues Logo): `.icns`, `.ico`, PNG.
- macOS: `Info.plist` mit Bundle-Kennung, Version aus Cargo, Mindestversion, `LSUIElement` so wie es
  der Client heute verlangt (Tray; Fenster per Dock sichtbar – nachsehen, wie der Client sich verhält).
- Linux: `.desktop`-Datei, Symbole, AppStream-Metadaten; Laufzeitabhängigkeiten (GTK/AppIndicator für
  Tray) richtig angeben, so wie der Client sie braucht.
- Windows: MSI ohne Adminrechte installierbar (per Benutzer), Startmenü, Deinstallation.
- Autostart bleibt Sache des Clients.

## Skripte und Doku

- Paket-Skripte unter `verteilung/`, lokal ausführbar; macOS lokal bauen und prüfen.
- `docs/release.md` und `docs/release.de.md`: wie man ein Release macht (Version heben, Tag), welche
  Secrets, was unsigniert bedeutet (Gatekeeper/SmartScreen), wie man Prüfsummen und Herkunft prüft.
- README: Abschnitt Installation je Plattform.

## Prüfen

`actionlint` (Docker-Image `rhysd/actionlint:1.7.7` ist lokal vorhanden), `bash -n`, PowerShell-Syntax
so weit möglich, macOS-Paket lokal bauen und starten lassen **ohne** das echte Profil zu berühren
(`HOME`/Datenordner auf einen Temp-Ordner, Port frei wählen), danach beenden. `cargo fmt/clippy/test`
nur, wenn Rust-Code geändert wurde.

**Nicht committen, nicht pushen, kein Tag, kein Release.**
