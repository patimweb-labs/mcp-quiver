# 27 – Client ohne Fenster: Konsole und Docker (Entwurf)

Nach 22 und 24. Für autonome Agenten und Server. Noch offen, was unten unter „Entscheiden“ steht.

## Aufbau

- Neues Crate `crates/konsole`, Programm `quiver-cli`: derselbe Kern (`crates/client`, heute schon ohne egui), keine Grafikbibliotheken. `--headless` im Fenster-Programm bleibt als Verweis darauf.
- Gleiches Profil und gleicher Tresor wie das Fenster; die Ein-Instanz-Sperre gilt über beide. Läuft schon ein Client, sprechen die Befehle mit ihm, statt einen zweiten zu starten.

## Geführte Einrichtung

`quiver-cli einrichten` führt im Terminal (de/en, auch über SSH und `docker run -it`):

1. Server-Adresse, Erreichbarkeit prüfen
2. Anmelden: Adresse zeigen, im Browser auf **irgendeinem** Gerät bestätigen (eigener kurzer Code am quiver-Server, nur dort eingebbar, 10 min, gedrosselt; Bestätigungsseite nennt Gerät und Ort) – oder Dienstkonto (siehe unten)
3. Tools zeigen; je Tool: nutzen oder am Gerät aus
4. Angaben abfragen, mit Titel, Hilfe, Art und Prüfung wie im Fenster; Passwörter verdeckt; Zugangslisten anlegen
5. Testen und Ergebnis zeigen; Agent koppeln

Jeder Schritt ist wiederholbar und einzeln aufrufbar: `anmelden`, `tools`, `angaben <tool>`, `zugang <tool>`, `agent koppeln`, `status [--json]`, `protokoll`, `aus|an <tool>`, `export|import`. Ein Agent kann nichts davon aufrufen (nur am Terminal, nie über MCP).

## Ohne Rückfragen (Docker, Automatisierung)

- `quiver.toml`: Server, Tools an/aus, offene Werte, Zugänge. **Keine Geheimnisse**; stehen dort welche → Start scheitert.
- Geheimnisse nur als Datei (`/run/secrets/<tool>__<feld>`, `<tool>__<zugang>__<feld>`), nie als Umgebungsvariable oder Argument.
- Oder die verschlüsselte Sicherung aus dem Fenster (Aufgabe 15) plus Passwort-Datei: am Rechner einrichten, im Container einspielen.
- Schlüssel des Tresors als Docker-Secret (`QUIVER_TRESOR_SCHLUESSEL_DATEI`), damit er nicht neben den Daten im Volume liegt.
- `quiver-cli pruefen`: sagt je Tool genau, welche Datei oder welcher Wert fehlt. Exit-Codes, `/gesund` nur auf Loopback.

## Docker

- Image mit `quiver-cli` **und** allen eigenen Tools, linux/amd64 und arm64, ohne Root, Dateisystem nur lesbar, Volume `/profil`, keine Fähigkeiten, SBOM.
- Agenten erreichen den MCP-Endpunkt als **Sidecar im selben Netz-Namensraum** (`network_mode: service:agent`), er bleibt auf Loopback. Auf anderen Adressen nur mit TLS und ausdrücklichem Schalter.
- Agent-Token als Secret-Datei, die der Client beim Koppeln schreibt (0600).

## Dienstkonten (autonome Agenten sind keine Menschen)

- Der Admin legt ein **Dienstkonto** an: Name, Verantwortlicher (Entra-Benutzer), quiver-Gruppen, Ablauf (Pflicht, höchstens 1 Jahr). Scheidet der Verantwortliche aus oder wird gesperrt, ruht das Dienstkonto.
- Einbinden über einen Einmal-Code **nur für dieses Dienstkonto** (60 min, gedrosselt). Das ist die einzige Ausnahme von „keine Geräte-Codes mit Entra“ (ADR 0002 ergänzen).
- Je Dienstkonto am Server: erlaubte Tools, **Schreiben Standard aus**, Grenzen je Stunde, Abschalten sofort. Eigene Kennzeichnung in Geräten, Protokoll und Prüfspur.
- Tools mit persönlicher Browser-Anmeldung (`microsoft-365`, `zep`) gibt es für Dienstkonten nicht. Wer das braucht, richtet eine eigene Dienst-Anbindung ein.

## Voraussetzung: Programme je Plattform

Einträge tragen heute einen festen Pfad eines Rechners. Das geht weder im Container noch bei Kollegen. Eigene Tools nennen künftig nur den Namen (`quiver-jira`); der Client nimmt sie aus seinem Programmordner (signierte Pakete je Plattform: Prüfsumme und Signatur vor dem Start), nie aus `PATH`. Eigene Aufgabe 28, sie ist auch Voraussetzung für die Verteilung an Kollegen.

## Entscheiden

1. Dienstkonten wie beschrieben?
2. Wo laufen die autonomen Agenten (eigener Server, Azure, Kundensystem)?
3. Welche Tools brauchen sie zuerst, und welche sollen schreiben dürfen?
