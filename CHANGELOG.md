# Changelog

Alle nennenswerten Änderungen an mcp-quiver. Das Format folgt
[Keep a Changelog](https://keepachangelog.com/de/1.1.0/), die Versionen
[Semantic Versioning](https://semver.org/lang/de/).

Pflegt `release-please` aus den Conventional Commits; von Hand kommt nur der
Eintrag zur ersten Version hierher. Der Tag ist `v<Version>` und muss zur
Version in `[workspace.package]` passen.

## 0.1.0 (2026-11-25)

### Neu

- **Server** `quiver-server`: Bibliothek auf Basis des `server.json`-Formats,
  Sätze mit Priorität, Gruppen (lokal und aus Entra ID), Geräteabgleich über
  SSE, Admin-API, Prüfspur, verschlüsselte Refresh-Token. Ein Container, ein
  Port, ein Volume; distroless, ohne Shell, mit Grenzen für Rumpf, Zeit,
  Verbindungen und Anmeldeversuche.
- **Verwaltung** (Vue): Überblick, Bibliothek mit Import, Sätze, Gruppen,
  Benutzer, Geräte und Prüfspur; Ersteinrichtung und Anmeldung über den Root
  oder Microsoft Entra ID; dieselben Tokens und Bausteine wie das
  Client-Fenster.
- **Client** `quiver`: Anmeldung des Geräts über Entra oder Einmal-Code,
  Abgleich des Soll-Zustands, stdio- und entfernte MCP-Tools, ein MCP-Endpunkt
  unter `127.0.0.1:8741/mcp`, Koppeln von pi, Claude Code, OpenCode, GitHub
  Copilot und Hermes, Tresor-Datei für Geheimnisse, Export und Import der
  Angaben, Protokoll lokal mit Maskierung.
- **Client-Fenster** (egui): Verbindung, Tools, Agenten und Protokoll; Tray und
  Autostart; Einrichten und Anmelden je Tool; mehrere Zugänge je Eintrag;
  Abschalten auf einem Gerät; drei Farbschemata, Deutsch und Englisch.
- **Microsoft Entra ID** (optional): Zertifikat (`private_key_jwt`) oder
  Geheimnis, Gruppen aus dem Token, Gruppennamen über Graph auf Wunsch,
  Zertifikatswechsel ohne Ausfallzeit.
- **Pakete** für macOS (arm64 und Intel), Windows (MSI und portables ZIP) und
  Linux (deb, rpm, AppImage, tar.gz), dazu `SHA256SUMS` und
  Herkunftsnachweis je Release; Signieren und Notarisieren, wenn die Geheimnisse
  gesetzt sind.
- **Release-Weg** über release-please: Tag und Bau im selben Workflow, weil ein
  von `GITHUB_TOKEN` erzeugter Tag keine weiteren Workflows auslöst; der Tag
  von Hand und der reine Testbau bleiben möglich.
