# 32 – Produktseite: quiver als fertiges Produkt auf GitHub

Vorbild (nur lesen, nicht kopieren): die Dateien README, CONTRIBUTING, SECURITY
und release-please aus den Projekten sprechwerk, wledambilight und crosspoint.
quiver soll genauso auftreten. Arbeitsordner ist dieses Repository auf dem
Branch `feat/release`. Das parallele Arbeitsverzeichnis des Vorgänger-Repos
wird nicht angefasst.

## 1. README

`README.md` (englisch) und `README.de.md` (deutsch), inhaltlich gleich:

- Logo rechts (`assets/marke`), Badges (Prüfen-Workflow, Release, Lizenz,
  Docker Hub `smotherer/mcp-quiver-server`),
- ein Satz, was es ist, zwei Bilder (hell/dunkel),
- „Was es kann", „Für wen", „So sieht es aus" (Client und Verwaltung),
  Schaubild,
- „Installation" je Plattform (Client aus Releases, Server als Docker-Image mit
  Compose),
- „Erste Schritte" (Server einrichten, Entra verbinden mit Verweis auf
  `docs/entra.md`, Gruppe/Satz zuweisen, Client verbinden, Agent anbinden),
- „Sicherheit und Daten", „Betrieb", „Fehlersuche",
- „Stand heute" (ehrlich: was geht, was nicht),
- „Mitentwickeln", Lizenz.

Bisherige ausführliche Entwickler-Inhalte wandern nach `docs/` (Verweise
prüfen, nichts verlieren). Nichts versprechen, was der Code nicht kann: jede
Aussage im Code oder in `docs/` nachsehen.

## 2. Bilder

Vorhandene aus `docs/bilder` verwenden. **Vorher jedes verwendete Bild
ansehen:** keine echten Namen, E-Mail-Adressen, Firmennamen, Hostnamen,
IP-Adressen oder Kundennamen. Was solche Daten zeigt, nicht verwenden und in
der Abschlussliste nennen (nicht löschen).

## 3. Projektdateien

- `CHANGELOG.md` (Anfang), `CONTRIBUTING.md`, `SECURITY.md` (Meldeweg: GitHub
  Private Vulnerability Reporting, keine E-Mail),
- Issue-Vorlagen und PR-Vorlage unter `.github/`,
- `release-please-config.json` und `.release-please-manifest.json` passend zur
  Cargo-Workspace-Version.

Wie in sprechwerk müssen Tag und Bau im selben Workflow passieren, weil
`GITHUB_TOKEN`-Tags keine weiteren Workflows auslösen. `release.yml` dafür
passend erweitern, ohne den Weg „Tag von Hand" und den reinen Testbau zu
verlieren. Rechte minimal.

## 4. Keine internen Daten

Keine internen Daten (Firmen, Kunden, IPs, interne Pfade, Personen) in einer
öffentlichen Datei; auch bestehende README- und `docs/`-Stellen, die angefasst
werden, daraufhin prüfen. Diese Aufgabe selbst liegt als
`docs/aufgaben/32-produktseite.md` im Repo, ohne private Pfade.

## 5. Vorschläge (nur Text, nicht ausführen)

Repo-Beschreibung, Homepage, Topics und Eintrag für das Organisationsprofil
`patimweb-labs`.

## Prüfen

- alle relativen Links und Bilder existieren (kleines Skript),
- `actionlint` (Docker-Image `rhysd/actionlint:1.7.7` lokal),
- Markdown rendert plausibel.

**Nicht committen, nicht pushen, kein Tag, keine GitHub-Einstellungen ändern.**
