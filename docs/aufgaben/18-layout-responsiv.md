# 18 – Layout: Protokoll aufräumen, alle Seiten responsiv

Rückmeldung des Users: Die Bereiche reagieren schlecht auf die Fenstergröße, und das Protokoll
sieht unaufgeräumt aus.

## Abnahme mit Bildern (Pflicht)

`QUIVER_BILDER=docs/bilder` erzeugt jede Seite des Fensters in **drei Größen**: 560×460 (Minimum),
800×600 und 1200×820. Gemeint sind Verbindung, Tools, Zugänge, Agenten und Protokoll (Aufrufe und
Tool-Logs), die Dialoge, `dark` und `light`.

Dateiname: `fenster-<thema>-<seite>-<breite>.png`. Auf keinem Bild darf etwas abgeschnitten
sein, sich überlappen oder seitlich hinausragen. Gibt es das, wird umgebrochen oder gescrollt.
Die Bilder sind Teil der Lieferung.

## Responsiv (alle Seiten)

- **Seitenleiste:** Unter 760 px Breite schmal, nur Icons mit Tooltip.
- **Inhalt:** höchstens 1100 px breit, zentriert, Ränder mit Tokens.
- **Filter- und Knopfleisten:** Sie brechen um (`horizontal_wrapped`) statt hinauszuragen.
  Gleich hohe Steuerelemente, einheitlicher Abstand (`SPACE_*`).
- **Formulare:** Beschriftung links und Feld rechts ab 700 px. Darunter steht die Beschriftung
  über dem Feld. Den Baustein dafür gibt es (`bausteine::feld`), er wird angepasst.
- **Karten und Listen:** volle Breite, lange Texte mit Ellipse und vollem Text im Tooltip, nie
  hinausragend.
- **Kopfzeile:** Theme- und Sprachwahl brechen unter 700 px in ein Menü um, oder sie werden
  kompakter.

## Protokoll

**Filterleiste in einer Zeile, die umbricht:**
- Auswahlen für Eintrag, Tool und Agent
- „nur Fehler“ als **Checkbox direkt in derselben Leiste**, nicht allein in einer eigenen Zeile
- Zeitraum als Segmentauswahl
- Rechts „Protokoll leeren“

**Tabelle:** feste Spaltenbreiten in Abhängigkeit von der Breite.
- **Zeit:** `HH:MM:SS`, Monoschrift, Datum im Tooltip
- **Status:** als Abzeichen direkt hinter der Zeit
- **Tool:** `eintrag › tool`, mit Ellipse
- **Agent**
- **Dauer:** rechtsbündig

Die Zeilen:
- haben **Zebra-Streifen und Hover**
- öffnen per Klick das Detail, mit **Hand-Cursor**
- sind mit `SPACE_2`/`SPACE_3` kompakt
- unter 700 px fallen Agent und Dauer weg und stehen im Detail

**Kopfzeile der Tabelle:** fest stehend, mit dünner Trennlinie.

**Leerer Zustand:** eine zentrierte Zeile wie „Noch keine Aufrufe in diesem Zeitraum“ mit
Hinweis auf die Filter.

**Tool-Logs:**
- Monoschrift, Zeilen mit Zeit und Stufe
- Fehlerzeilen in Fehlerfarbe
- Zeilenumbruch an
- automatisch ans Ende scrollen, solange der User unten ist

**Detail-Dialog:**
- Abschnitte Argumente, Fehler und Antwort
- jeweils ein Block in Monoschrift mit Rahmen und dem Knopf „Kopieren“
- scrollt im Dialog

## Vue (`admin/`)

Wo die Verwaltung dieselben Bausteine hat (Feld, Filterleiste, Liste), gelten dieselben
Regeln. Parität 1:1.

## Tests

- kittest bei 560 px: Kein Steuerelement liegt außerhalb des Fensters. Geprüft wird über die
  Rechtecke der Widgets, je Seite.
- Die Checkbox „nur Fehler“ liegt in derselben Zeile wie die Filter, wenn die Breite reicht.
- Zeilen-Hover hat einen Hand-Cursor.
- Unter 700 px gibt es keine Spalten Agent und Dauer.
- Alle bestehenden Tests bleiben grün.

**Prüfbefehl grün** (Rust und admin). **Nicht committen.**
