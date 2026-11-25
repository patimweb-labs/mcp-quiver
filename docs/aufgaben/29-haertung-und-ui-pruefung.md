# 29 – Härtung und Prüfung der Oberfläche (Phase nach Entra)

Wunsch, 2026-11-24. Beginnt, wenn Entra (20–26, 22) im Alltag läuft. **Vor** der Verteilung an Kollegen. In dieser Phase keine neuen Funktionen. Befunde zuerst sammeln, nach Schwere ordnen, mit dem Nutzer durchgehen, dann beheben; jeder Fix mit Test.

## A. Härtung

1. **Bedrohungsmodell** schreiben: böser Agent, böses Tool, Schadtext in Mails/Tickets/Dateien (Prompt-Injection), anderer Benutzer am Rechner, Angreifer im Netz, gestohlenes Gerät, abtrünniger Admin, ausgeschiedener Kollege.
2. **Server:** Rechte-Matrix gegen jede Route, Sitzungen, CSRF, Drossel, Kopfzeilen, Eingabegrenzen, hängende und riesige Antworten, Last, Ausfall von Entra, Uhr falsch, Platte voll, Absturz mitten im Schreiben, Sicherung und Wiederherstellung, Migration zurück.
3. **Client:** MCP-Endpunkt (andere Prozesse und Webseiten: Origin, DNS-Rebinding), Tresor und Rechte, Geheimnisse in Log, Protokoll, Absturzbericht, Export; Prozesse enden sauber; tagelanger Betrieb (Speicher, offene Dateien); Schlafen, Netzwechsel, VPN. Automatischer Neustart abgestürzter Tools.
4. **Tools:** je Tool Schreibschutz, Begrenzungen (`nur`), Pfad-Ausbruch, SSRF, Weiterleitungen, große Antworten, Geheimnisse in Fehlern. DevOps-Attrappe gegen die offizielle API abgleichen. Nacharbeiten, was noch nie gegen echte Dienste lief (Jira, Confluence, Zephyr, DevOps, E-Mail), zuerst lesend.
5. **Lieferkette:** `cargo deny`/`audit`, `npm audit`, SBOM, signierte Pakete (28), Image ohne Root, feste Versionen; Fuzzing der Leser (XML, JSON, Excalidraw, draw.io, Export).
6. **Unabhängige Prüfung:** nicht durch den, der es gebaut hat. Anschließend eigener Angriffstest auf einem Testserver.
7. **Betrieb:** TLS und öffentliche Adresse, Ablauf des Zertifikats, Überwachung, Notfallanleitung (Root, alle Geräte sperren, Schlüssel tauschen), Datenschutz (was steht wie lange im Protokoll).

## Security-Durchgang (Pflicht, eigener Bericht)

Ausdrücklicher Wunsch des Nutzers. Gemeinsam durchgehen, Punkt für Punkt: Wie ist es geschützt, wo steht es im Code, welcher Test belegt es, was bleibt als Risiko? Ohne Nachweis gilt ein Punkt als offen.

| Bereich | Fragen |
|---|---|
| Anmeldung | Entra, Root, Ersteinrichtung, Sitzungen, Geräte-Token, Entzug: Wie schnell wirkt er wirklich? |
| Rechte | Rollen, Gruppen, Dienstkonten: Kann sich jemand selbst erhöhen? Sieht jemand Fremdes? |
| Geheimnisse | Tresor, Server-Schlüssel, Zertifikat, Agent-Token, Secrets in Docker: Wo liegen sie, wer kann lesen, wie tauscht man sie? Auch: Sicherungen, Logs, Absturzberichte, Git-Verlauf |
| Agenten | Was kann ein manipulierter Agent höchstens anrichten? Schreibschutz, Freigaben, Schadtext in Inhalten, Abfluss von Daten über ein zweites Tool |
| Tools | Pfade, SSRF, Weiterleitungen, Befehle, fremde Antworten, Begrenzungen je Zugang |
| Netz | TLS, Loopback, andere Prozesse und Webseiten, Reverse-Proxy |
| Lieferkette | Abhängigkeiten, Signaturen, Build, Updates, Image |
| Nachvollziehbarkeit | Prüfspur vollständig und nicht veränderbar? Datenschutz und Löschfristen |
| Notfall | Gerät gestohlen, Konto übernommen, Schlüssel verloren, Server kompromittiert: Übung am Testserver |

Ablauf: Bedrohungsmodell → Code-Durchsicht je Bereich durch zwei getrennte Prüfer (nicht der Entwickler) → Werkzeuge (`cargo audit/deny`, Geheimnis-Suche im gesamten Git-Verlauf, OWASP ASVS Stufe 2 für die Verwaltung) → Angriffstest am Testserver → Befunde beheben → Nachtest. Nur gegen eigene Testsysteme, nie gegen echte Dienste. Empfehlung: vor dem Einsatz außerhalb des eigenen Betriebs zusätzlich ein externer Test.

## B. Oberfläche (Client und Verwaltung)

1. **Jede Seite und jeder Dialog** in dark/light/mono, de/en, 560/800/1200/1600 px, 100/150/200 %, leer/lädt/voll/Fehler, sehr lange Namen, 1/50/500 Einträge. Bilder nebeneinander: egui und Vue **1:1 gleich**.
2. Nichts abgeschnitten, überlagert, außerhalb des Fensters; Dialoge passen in kleine Fenster; Speichern immer erreichbar.
3. **Hand-Cursor** auf allem Klickbaren, Fokus sichtbar, Tastatur ganz durch, Vorlesen (Rollen und Namen), Kontrast AA, keine Information nur über Farbe.
4. **Wege** einmal als neuer Kollege ohne Vorwissen: Anmelden, Tool einrichten, Agent koppeln, Fehler verstehen; als Admin: Gruppe, Satz, Rolle, Gerät sperren. Jede Fehlermeldung sagt, was zu tun ist.
5. Einheitlich: Begriffe (Tool, Eintrag, Satz, Zugang, Gerät), Abstände, Knöpfe, Icons, Logo, Texte, Datumsformate. Keine Fachwörter ohne Erklärung.
6. Echte Geräte: macOS, **Windows und Linux** (Tray, Autostart, Dock/Taskleiste, Datei-Dialog, hohe Auflösung, zwei Bildschirme).
7. Gemeinsamer Durchgang mit dem Nutzer, Seite für Seite.

## Fertig

Keine offenen schweren oder mittleren Befunde, Rest mit Entscheidung in `docs/offen.md`; Bericht mit Nachweisen; Freigabe durch den Nutzer.
