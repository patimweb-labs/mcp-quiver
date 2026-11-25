## Was ändert sich

<!-- Ein Absatz. Was war vorher, was ist jetzt? -->

## Warum

<!-- Die Lage, nicht nur die Lösung. Bei einer Festlegung: Begründung und Preis. -->

## Prüfen

- [ ] `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace`
- [ ] `cargo deny check` (bei neuen oder gehobenen Kisten)
- [ ] in `admin/`: `npm run typecheck`, `npm test`, `npm run build`
- [ ] alle drei Farbschemata geprüft (dark, light, mono), in egui **und** Vue
- [ ] Hand-Cursor auf allen klickbaren Elementen
- [ ] Tastaturbedienung geprüft, Fokus sichtbar, Rollen und Namen gesetzt
- [ ] Screenshot dabei (vorher/nachher), falls sich die Oberfläche ändert
- [ ] keine Hex-Werte, keine Pixelmaße, keine neuen Abhängigkeiten ohne Begründung
- [ ] Dokumentation nachgezogen (`README.md`, `README.de.md`, `docs/`, `SPEC.md`)
- [ ] Cheats dieses PRs als Ticket vermerkt

## Sicherheit

- [ ] keine Geheimnisse in Code, Commits, Logs oder Testdaten
- [ ] neue Eingaben geprüft, neue Grenzen gesetzt, Fehler nach außen kurz
- [ ] betrifft eine Sicherheitszusage aus `SECURITY.md`? Dann bitte **nicht** hier beschreiben, sondern über Private Vulnerability Reporting melden.
