# 17 – Protokoll der Tool-Aufrufe und der Tool-Prozesse

Der User will sehen, welche Anfragen die Agenten auslösen und was in den Tools passiert.
Agenten sollen das Protokoll auch selbst über MCP lesen können, um Fehler zu finden.

## 1. Aufzeichnen (crates/client, neues Modul `protokoll.rs`)

Der Endpunkt (`endpunkt.rs`) schreibt **jeden** `tools/call` mit:
- Zeit (RFC 3339), Agent (Name aus dem Agent-Token), Eintrag und Tool, Dauer in ms
- Ergebnis `ok | fehler | abgelehnt`, wobei `abgelehnt` z. B. „Tool unbekannt“ heißt, dazu der
  Fehlertext (max. 2000 Zeichen)
- Argumente als **gekürztes** JSON: je Wert höchstens 200 Zeichen, gesamt höchstens 4 KB
- Größe der Antwort in Zeichen, dazu eine Vorschau: Text gekürzt auf 500 Zeichen

**Maskieren, Pflicht:**
- Schlüssel, deren Name `pass|token|secret|key|pat|auth|cookie` enthält (Groß- und
  Kleinschreibung egal), werden durch `<verborgen>` ersetzt.
- Ebenso **jeder Wert, der einem Geheimnis aus dem Tresor entspricht**, auch als Teilstring,
  in Argumenten, Vorschau und Fehlertext.

**Tool-Prozesse:** stderr jedes Stdio-Eintrags zeilenweise je Eintrag mitschreiben. Heute geht
es nur ins Client-Log. Die Maske gilt hier genauso. ANSI-Farbcodes werden entfernt.

**Speicher:**
- Im Arbeitsspeicher ein Ringpuffer mit 2000 Aufrufen und 5000 Logzeilen.
- Auf der Platte `<profil>/protokoll/aufrufe-YYYY-MM-DD.jsonl` und `prozesse-YYYY-MM-DD.jsonl`,
  Rechte 0600.
- Was älter als 7 Tage ist, wird beim Start und einmal täglich gelöscht. Höchstens 50 MB
  insgesamt, sonst fliegen die ältesten Dateien zuerst raus.

Das Modell bekommt keine Liste, sondern eine Abfrage-API am Kern:
`Kern::protokoll(filter) -> Vec<Aufruf>` und `Kern::prozess_log(eintrag, filter) -> Vec<Zeile>`.
Der Filter kennt Eintrag, Tool, Agent, nur Fehler, `seit` und `anzahl`. Dazu kommt ein
`watch`-Zähler, damit das Fenster bei neuen Einträgen neu lädt.

## 2. Seite „Protokoll“ im Fenster (egui)

Neue Seite in der Navigation, mit Icon und Pointer-Cursor.

- **Filterleiste:**
  - Auswahl Eintrag/Tool und Agent
  - Schalter „nur Fehler“
  - Zeitraum: letzte Stunde, heute, 7 Tage
- **Liste:** Zeit, Agent, Tool, Dauer, Status als Abzeichen (gut, fehler, warnung).
  - Läuft live mit. Neue Einträge kommen oben dazu.
  - Hat der User gescrollt, bleibt die Position stehen.
- **Klick auf eine Zeile:** Detail-Dialog mit Argumenten, Fehlertext und Antwort-Vorschau, dazu
  „Kopieren“ (wie der Kopier-Knopf beim Token).
- **Zweiter Reiter „Tool-Logs“:** Auswahl des Eintrags, die Zeilen in Monoschrift, mit Filter.
- **Knopf „Protokoll leeren“:** mit Bestätigung, gilt für Speicher und Dateien.

Texte stehen in `texte.json` auf Deutsch und Englisch. Bei langen Listen wird nur der sichtbare
Teil gezeichnet (`show_rows`).

## 3. Eigene MCP-Tools von quiver

Der Endpunkt bietet zusätzlich zu den Einträgen zwei eigene Tools an, Präfix `quiver__`. Beide
sind lesend (`read_only_hint`), jedes Eingabeschema hat `type: object`:
- `quiver__protokoll`: Filter wie oben, Standard die letzten 50 Aufrufe, höchstens 200.
- `quiver__tool_log`: `eintrag` (Pflicht), Filter `enthaelt`, `seit`, `anzahl` (Standard 100,
  höchstens 500).

**Schutz:**
- Neuer Schalter im Fenster unter Agenten: **„Agenten dürfen Inhalte im Protokoll sehen“**,
  Standard **aus**. Er wird in `zustand.json` gespeichert.
- Ist er aus, liefern beide Tools für Agenten **nur Metadaten**: Zeit, Agent, Tool, Dauer,
  Status und Fehlertext. Argumente, Antwort-Vorschau und Prozess-Logzeilen fehlen dann, dafür
  steht dort ein Hinweis, wie man den Schalter einschaltet.
- Fehlertexte sind auch bei ausgeschaltetem Schalter maskiert.
- Die eigenen Aufrufe von `quiver__*` landen **nicht** im Protokoll, sonst entsteht eine
  Rückkopplung.

## Tests

- **Aufzeichnung:**
  - Ein Aufruf wird mit Agent, Dauer und Status protokolliert.
  - Ein Fehler wird festgehalten.
  - Argumente mit `passwort` oder `api_token` sind maskiert.
  - Ein Tresor-Geheimnis erscheint in **keiner** Datei und keiner Antwort, auch nicht als
    Teilstring. Das ist ein Ende-zu-Ende-Test mit echtem Endpunkt und Demo-MCP.
- **Rotation:** Alte Dateien werden gelöscht, die Größengrenze greift.
- **stderr:** Die Zeilen eines Demo-MCP landen je Eintrag ohne ANSI-Codes.
- **MCP:**
  - Ohne Schalter liefert `quiver__protokoll` keine Argumente und `quiver__tool_log` keine
    Zeilen, nur den Hinweis. Mit Schalter beides.
  - `quiver__*` taucht nicht im Protokoll auf.
  - Beide Tools stehen in `tools/list`.
- **kittest:**
  - Die Seite ist erreichbar.
  - Die Filter wirken.
  - Der Detail-Dialog öffnet sich.
  - Hand-Cursor auf allem Klickbaren.
  - Der Schalter auf der Agenten-Seite ist eine Checkbox.

## Doku

- README und SPEC: Protokoll und Datenschutz, also was Agenten sehen.
- `docs/offen.md`: Export des Protokolls, Meldung von Fehlerzahlen an den Server.

**Prüfbefehl grün:**
`cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
**Nicht committen.**
