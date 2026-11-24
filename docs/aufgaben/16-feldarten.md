# 16 – Feldarten (Checkbox, Zahl, Url), Titel, sichtbares Scrollen, fehlende Pflichtfelder

Auslöser: Im Dialog „System anlegen“ (ABAP) waren die Schalter `zertifikat_pruefen` und
`schreiben` Textfelder. Die Pflichtfelder `benutzer` und `url` lagen weggescrollt, und
„Speichern“ blieb ohne Erklärung gesperrt.

## Domain (`crates/domain`)

`Feld` bekommt zwei optionale Felder. Beide sind abwärtskompatibel, d. h. alte JSON-Daten
gelten weiter.

- `art: FeldArt` mit `#[serde(default)]`:
  - Varianten `text` (Standard), `passwort`, `zahl`, `schalter`, `url` und
    `auswahl { optionen: [{wert, anzeige}] }`
  - JSON in snake_case
  - `Quelle::Geheim` gilt weiterhin als Geheimfeld, egal welche Art.
- `titel: Option<String>`: die Anzeige statt des Feldnamens.

Der Server speichert beide Felder unverändert, das braucht einen Test. Der Client reicht sie ins
Modell durch (`EintragSicht.felder`, Zugangs-Felder).

## Darstellung: egui (`crates/fenster`) und Vue (`admin`), 1:1 gleich

| Art | Darstellung | Wert |
|---|---|---|
| `schalter` | **Checkbox** mit Titel daneben | `an`/`aus`. Ohne gespeicherten Wert gilt `standard`, sonst `aus`. |
| `zahl` | Eingabe nur für Ziffern | Prüfung beim Speichern |
| `url` | Eingabe | Prüfung `http(s)://` |
| `auswahl` | Auswahlliste | |
| `passwort`/geheim | wie bisher | |

Für alle Arten gilt:
- Die Beschriftung ist `titel`, sonst `name`.
- Pointer-Cursor auf Checkbox und Auswahl.

Ändern gilt in allen Dialogen mit Feldern: Werte, Zugang anlegen/bearbeiten und in der
Verwaltung der Feldeditor (`Feldkarte.vue`). Im Feldeditor kann man Art und Titel auswählen.

## Bedienung

- Die Feldliste scrollt wie bisher, aber mit **immer sichtbarer Scrollleiste**, sobald sie
  überläuft:
  - egui: `ScrollArea` mit `scroll_bar_visibility(AlwaysVisible)`
  - Vue: entsprechend
- Neben „Speichern“ steht, solange es gesperrt ist, der Grund, z. B. **„Es fehlt: SAP-Benutzer,
  ADT-Adresse“** (Titel der leeren Pflichtfelder) oder „Ungültige Zahl: Mandant“.
- „Speichern“ ist nur gesperrt, wenn ein Pflichtfeld leer oder ein Wert ungültig ist.
- Leere, nicht verpflichtende Felder sind immer erlaubt.

## Tests

- Domain: Serde-Rundlauf mit und ohne `art`/`titel`, alte JSON-Daten lesbar.
- Server: speichert `art`/`titel` unverändert.
- kittest:
  - Ein Schalter-Feld ist eine Checkbox mit Hand-Cursor.
  - Ein Klick setzt `an`/`aus`.
  - Der Standard `an` ist vorgehakt.
  - Ein fehlendes Pflichtfeld wird neben „Speichern“ genannt.
  - Die Scrollleiste ist sichtbar, wenn die Felder überlaufen.
- Vue (vitest): dieselben Fälle für Darstellung und Sperrgrund, dazu die Feldkarte mit
  Art-Auswahl.

**Prüfbefehl grün** (Rust und admin: `npm run typecheck && npm test`).
**Nicht committen.**
