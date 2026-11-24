# 27 – quiver-cli: Client für Docker (Entwurf)

Nach 22 und 24. Für autonome Agenten. Noch offen, was unten unter „Entscheiden“ steht.

## Grundsatz

Entscheidung des Nutzers (2026-11-24): **`quiver-cli` ist rein für Docker gedacht** und wird nicht dort installiert, wo der Fenster-Client läuft. Der Fenster-Client bleibt unverändert.

| | Fenster-Client | quiver-cli |
|---|---|---|
| Für | Menschen am Arbeitsplatz | autonome Agenten |
| Ausgeliefert als | App je Plattform | **nur Docker-Image** |
| Einrichtung | Fenster | geführt über die Konsole des Containers |
| Konto | Entra-Benutzer | Dienstkonto |
| Laufzeit | Tray, Autostart | Container (Neustart durch Docker/Compose/Kubernetes) |

- Neues Crate `crates/konsole`, Programm `quiver-cli`, ohne Grafikbibliotheken, auf dem Kern `crates/client`. Keine Pakete, kein Installer, kein Dienst, kein Autostart.
- Außerhalb des Images startet es nicht (Kennzeichen im Image; für die Entwicklung ein ausdrücklicher Schalter in Debug-Builds). Das ist Schutz vor Versehen, keine Sicherheitsgrenze.
- Eigenes Profil und eigener Tresor im Volume, **eigenes Gerät** am Server je Container. Zwei Container mit demselben Volume: der zweite startet nicht.

## Geführte Einrichtung in der Konsole

`docker run -it --rm -v quiver:/profil … einrichten` (de/en), danach läuft derselbe Container ohne Rückfragen:

1. Server-Adresse, Erreichbarkeit und Zertifikat prüfen
2. Dienstkonto einbinden (Einmal-Code aus der Verwaltung)
3. Tools zeigen; je Tool nutzen oder aus
4. Angaben abfragen, mit Titel, Hilfe, Art und Prüfung wie im Fenster; Passwörter verdeckt; Zugangslisten
5. Testen und Ergebnis zeigen; Agent koppeln (Token als Datei)

Jeder Schritt ist wiederholbar, auch im laufenden Container (`docker exec -it … quiver-cli angaben jira`): `tools`, `angaben`, `zugang`, `agent`, `status [--json]`, `protokoll`, `aus|an`, `einspielen`. Interaktiv als Oberfläche im Terminal (Tastatur, schmal, ohne Farbe lesbar). Ein Agent kann nichts davon aufrufen: kein MCP-Weg dorthin, und der Agent läuft in einem **anderen** Container ohne Zugriff auf Volume und Docker-Socket.

## Ohne Rückfragen (Docker, Automatisierung)

- `quiver.toml`: Server, Tools an/aus, offene Werte, Zugänge. **Keine Geheimnisse**; stehen dort welche → Start scheitert.
- Geheimnisse nur als Datei (`/run/secrets/<tool>__<feld>`, `<tool>__<zugang>__<feld>`), nie als Umgebungsvariable oder Argument.
- Oder die verschlüsselte Sicherung aus dem Fenster (Aufgabe 15) plus Passwort-Datei: am Rechner einrichten, im Container einspielen.
- Schlüssel des Tresors als Docker-Secret (`QUIVER_TRESOR_SCHLUESSEL_DATEI`), damit er nicht neben den Daten im Volume liegt.
- `quiver-cli pruefen`: sagt je Tool genau, welche Datei oder welcher Wert fehlt. Exit-Codes, `/gesund` nur auf Loopback.

## Docker

- Image mit `quiver-cli` **und** allen eigenen Tools, linux/amd64 und arm64, ohne Root, Dateisystem nur lesbar, Volume `/profil`, keine Fähigkeiten, SBOM.
- Beispiel-`compose.yaml` (quiver-cli und ein Agent) und Kubernetes-Beispiel; ein Test spielt die Anleitung in frischen Containern durch.
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
