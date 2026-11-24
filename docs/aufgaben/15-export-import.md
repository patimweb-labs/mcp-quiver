# 15 – Einstellungen der Tools exportieren und importieren (Client)

Der User soll die Angaben seiner Tools sichern und auf einem anderen Gerät wieder einspielen
können. Gemeint sind die Werte je Eintrag (`lokale_werte`), die Zugangs-Listen (`zugaenge`, z. B.
SSH-Verbindungen, SAP-Systeme, DevOps-Organisationen) und **optional** die Geheimnisse aus der
Tresor-Datei (`wert/<eintrag>/<feld>`, Zugangs-Geheimnisse).

**Nicht** exportiert werden: das Geräte-Token, Agent-Tokens und Anmeldungen der Tools selbst
(Refresh-Tokens von m365/zep). Diese gehören zum Gerät bzw. zur Anmeldung.

## Format

Eine JSON-Datei `quiver-einstellungen-<datum>.json`:
`{ "format": "mcp-quiver/einstellungen", "version": 1, "erstellt": RFC3339, "eintraege": { "<id>": { "werte": {…}, "zugaenge": { "<name>": {…} } } }, "geheim": null | { "kdf": "argon2id", "salz", "nonce", "daten" } }`

- **Ohne Passwort:** Es werden nur offene Werte exportiert, `geheim` ist `null`.
- **Mit Passwort:**
  - Alle Geheimnisse kommen als JSON in `geheim.daten`.
  - Verschlüsselt wird mit XChaCha20-Poly1305 (wie `dateitresor.rs`).
  - Der Schlüssel kommt aus Argon2id (m=64 MiB, t=3, p=1) mit einem zufälligen Salz von 16 Byte.
  - Das Passwort hat mindestens 8 Zeichen.
- Die Datei wird mit den Rechten 0600 geschrieben.
- Es stehen nie Klartext-Geheimnisse in der Datei, im Log oder in Fehlermeldungen.

## Import

- Datei wählen, Vorschau anzeigen, dann übernehmen.
- **Vorschau:** Je Eintrag steht dort, was neu ist, was sich ändert und was übersprungen wird.
  Übersprungen wird ein Eintrag, den es auf diesem Gerät nicht gibt (nicht im Soll). Er bleibt
  trotzdem in der Datei vermerkt.
- **Mischen:**
  - Werte aus der Datei überschreiben gleichnamige Werte.
  - Werte, die nur am Gerät stehen, bleiben.
  - Zugänge werden über den Namen zusammengeführt.
- **Geheimnisse:**
  - Sie gehen nur mit Passwort in den Tresor.
  - Ein falsches Passwort führt zu einer klaren Meldung. Dabei wird **nichts** teilweise übernommen.
- Danach werden die betroffenen Einträge neu gestartet, wie bei `werte_setzen`.

## Kern (crates/client)

- `Kern::einstellungen_exportieren(pfad, passwort: Option<&str>) -> Result<Zusammenfassung>`
- `Kern::einstellungen_pruefen(pfad, passwort: Option<&str>) -> Result<Vorschau>`
- `Kern::einstellungen_importieren(pfad, passwort: Option<&str>) -> Result<Zusammenfassung>`
- Neue Abhängigkeit `argon2` (RustCrypto). Neues Modul `einstellungen.rs`.

## Fenster (crates/fenster, egui)

- **Seite Tools, Kopfzeile:** zwei Knöpfe, „Exportieren“ und „Importieren“, mit Icons und
  Pointer-Cursor (Pflicht, siehe AGENTS.md).
- **Datei wählen:**
  - Nativer Dialog über `rfd`, nur reines Rust, auf allen drei Plattformen.
  - In Tests: ein Pfad aus der Sicht, kein Dialog.
- **Dialog Export:** Schalter „Geheimnisse mitnehmen“. Ist er an, kommen zwei Passwortfelder
  dazu, mit Prüfung auf Gleichheit und Mindestlänge.
- **Dialog Import:**
  - Ein Passwortfeld erscheint nur, wenn die Datei Geheimnisse enthält.
  - Darunter steht die Vorschau, dann „Übernehmen“.
  - Die Feldliste scrollt, wie seit dem Fix für E-Mail.
- **Texte** in `texte.json`, auf Deutsch und Englisch.
- **Ergebnis:** Ein Hinweis oben auf der Seite, z. B. „6 Einträge exportiert (mit Geheimnissen)“.

## Tests

- Rundlauf: Export und Import auf einem frischen Kern ergeben dieselben Werte und Zugänge.
- Rundlauf mit Geheimnissen.
- Ohne Passwort:
  - Die Datei enthält keine Geheimnisse.
  - Ein Test prüft, dass `streng-geheim` nicht im Dateitext steht.
- Falsches Passwort: Fehler, der Tresor bleibt unverändert.
- Mischen: Werte, die nur am Gerät stehen, bleiben.
- Ein unbekannter Eintrag wird übersprungen und in der Vorschau gemeldet.
- Fremdes oder kaputtes Format: Fehler ohne Inhalt.
- kittest:
  - Die Knöpfe sind sichtbar und haben einen Hand-Cursor.
  - Im Export-Dialog sind die Passwortfelder erst nach dem Schalter da.
  - Der Import-Dialog zeigt die Vorschau.
- Die Rechte der Datei sind 0600 (unter Unix).

**Prüfbefehl grün:**
`cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
**Nicht committen.**
