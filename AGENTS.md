# AGENTS.md — Vorlage für alle Weppelmann-Projekte

> Diese Datei liegt in **jedem** Repo (UI-Bibliothek und Apps). Sie ist die verbindliche
> Arbeitsanweisung für Menschen und Agenten. Abweichungen nur mit Eintrag in
> `docs/adr/` und Vermerk hier. Der projektspezifische Teil ist §9.
>
> **Vermerk:** mcp-quiver hat zwei Oberflächen: den Client in Rust/egui und die
> Admin-Oberfläche in Vue. Für den Client gilt das Rust-Gegenstück aus §3, §4
> und §9, für die Admin-Oberfläche die Vue-Regeln der Vorlage.

## 1. Worum es geht

Eine zentrale Bibliothek von MCP-Tools und ein Client auf jedem Gerät. Admins
weisen Tool-Sätze über Entra-ID-Gruppen zu, der Client installiert,
konfiguriert und stellt sie allen Agenten über einen Endpunkt bereit.
Anmeldungen passieren nur im Client. Details: [SPEC.md](SPEC.md).

## 2. Nicht verhandelbar

1. **Datenorientiert.** Das Datenmodell ist die einzige Wahrheit, die Oberfläche ist
   `render(model) → Sicht`. Kein Zustand in der Darstellung, der nicht im Modell steht.
2. **Zustände als Summentypen.** `enum Zustand { Leer, Laedt, Fertig(Daten), Fehler(Text) }`
   statt Fächer aus Booleans. Ein Rust-`enum` mit `serde` und ein TS-Diskriminator
   beschreiben denselben Vertrag.
3. **Sichtbarkeit folgt aus dem Modell.** „Einblenden" ist keine Anweisung, sondern eine
   Folge des Zustands. Kein `show()`, kein `display:none` als Logik, keine `v-if`-Kette
   für Geschäftslogik.
4. **Keine Logik in der Darstellung.** Rechnen, laden, filtern in `domain` (Rust) oder in
   reinen TS-Funktionen — beide mit Unit-Tests.
5. **SIMPLE** (Wrigstad): immer ein lauffähiges Programm, nach jeder Änderung bauen,
   Tests laufen lassen, Problem rekursiv verkleinern. Cheats sind erlaubt, aber
   ticketpflichtig, lokal begrenzt und werden abgebaut.
6. **Keine Farben, Größen oder Abstände im Code.** Nur semantische Tokens
   (`var(--wp-color-text-default)` in Vue, `f.text_default` / `thema::SPACE_4` in egui).
   Hex-Werte, Pixel-Magie und Grundwerte in Ansichten sind Fehler.
7. **Drei Themes, sonst nichts:** `dark`, `light`, `mono`. Keine Runtime-Themes,
   keine Nutzer-Themes, kein Theme-Editor.
8. **Tests sind Teil der Lieferung.** Unit, Integration, Vertrag, Rundlauf.
   Ein Fix ohne Test ist ein Fix auf Probe.
9. **Keine Secrets** in Code, Commits, Logs oder Testdaten. Konfiguration über
   Umgebung, Beispiel in `.env.example`.
10. **Plattformneutral.** Kein `cfg(target_os)` und keine Pfadannahmen im Frontend.
    Unterschiede kommen als Daten (`platform` im Modell → `data-platform` am Root).
    Tastenkürzel über `Mod`, nie fest `Ctrl`.
11. **1:1 gleiches Aussehen.** Client (egui) und Admin (Vue) sehen identisch aus.
    Beide erzeugen ihre Themes aus denselben Token-Dateien; Bausteine heißen gleich
    und haben gleiche Maße.
12. **Hand-Cursor auf allem Klickbaren.** In egui nicht Standard: die Bausteine setzen
    `CursorIcon::PointingHand` zentral, ein Oberflächentest prüft es. In Vue
    `cursor: pointer` im Baustein, nie in der Ansicht.

## 3. Struktur (geplant)

```
crates/domain          Datenmodell: Eintrag, Satz, Zuweisung, Soll/Ist   → Unit-Tests
crates/bibliothek      server.json + quiver-_meta lesen, prüfen          → Unit-Tests
crates/protocol        Client ↔ Server: Soll-Zustand, Ereignisse         → Vertragstests
crates/server          Bibliothek, Zuweisung, Admin-API + `quiver-server`→ Integration
crates/client          Abgleich, Laufzeiten, MCP-Endpunkt, Tresor-Datei → Integration
crates/laufzeit        wasm (wasmtime), nativ, npx/uvx, oci, remote      → Integration
crates/agent-sdk       Agent → Client für Plugins                        → Vertragstests
crates/oberflaeche     Bausatz: Themes aus Tokens, Texte, Schriften
crates/fenster         Client-Fenster (egui), dünn                       → Oberflächentests
plugins/pi             napi-rs-Hülle um agent-sdk
plugins/hermes         PyO3-Hülle um agent-sdk
tools/microsoft        Tool-Satz Microsoft 365 (Graph)
admin/                 Admin-Oberfläche (Vue, weppelmann-ui)
tokens/                gemeinsame Token-Dateien für egui und Vue
docs/                  architektur.md, datenfluss.md, adr/
```

Richtung der Abhängigkeiten: `domain ← bibliothek ← protocol ← client ← fenster`,
`domain ← protocol ← server`. Nie zurück. `fenster` kennt keine Domäne, nur
Aktionen, Meldungen und Zustände; `oberflaeche` kennt weder Client noch Server.

## 4. Befehle

```bash
# alles prüfen (das ist der Befehl vor jedem Commit)
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

`cargo test` **mit** `--workspace`: ohne das prüft Cargo nur das Wurzelpaket.
Weitere Befehle kommen mit dem Code.

## 5. Definition of Done

- [ ] Tests grün (Unit, Integration, Vertrag, Rundlauf)
- [ ] `clippy -D warnings` sauber, in `admin/` zusätzlich `vue-tsc`
- [ ] alle drei Themes geprüft (dark, light, mono), in egui **und** Vue
- [ ] Hand-Cursor auf allen klickbaren Elementen
- [ ] Tastaturbedienung geprüft, Fokus sichtbar, Rollen und Namen gesetzt
- [ ] Screenshot im PR (vorher/nachher bei UI-Änderungen)
- [ ] keine Tokens umgangen, keine Hex-Werte, keine neuen Abhängigkeiten ohne Begründung
- [ ] Dokumentation angepasst: `README.md`, `README.de.md`, `docs/`, `SPEC.md`
- [ ] Cheats dieses PRs als Ticket vermerkt

## 6. Commits und Releases

- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `build:`, `ci:`).
- `release-please` mit `bump-minor-pre-major: true`.
- Kleine Commits, ein Gedanke pro Commit, deutschsprachige Betreffzeile erlaubt.
- **Immer über einen Branch und einen Pull Request.** `main` ist immer lauffähig.

## 7. Dokumentation

- `README.md` (Englisch) und `README.de.md`: was die App tut, für wen, Screenshots
  (dark + light), Installation je Plattform, ein Absatz zu Daten und Datenschutz.
- `docs/architektur.md`, `docs/datenfluss.md`, `docs/adr/NNNN-titel.md`.
- `SPEC.md`: jede Festlegung mit Begründung und Preis.
- Screenshots werden **erzeugt** (egui: `egui_kittest`, Vue: Playwright), nicht von
  Hand abgelegt.

## 8. Marke und Icons

Logo und Icon liefert der Nutzer (Stand 2026-11-23: in Arbeit). Quelle werden die
SVGs in `assets/`, die Icons entstehen per Skript in `packaging/icons/` — wie bei
sprechwerk (dort ADR 0015). Regeln folgen mit dem Logo als ADR.

## 9. Projektspezifisch

- **Name, Zweck, Zielgruppe:** mcp-quiver — zentrale MCP-Bibliothek mit Client je
  Gerät; Teams und Firmen mit Entra ID und mehreren Agenten (pi, Hermes, weitere).
- **Besonderheiten:**
  * Der Server ist nur Bibliothek und Steuerung, er führt keine Tools aus.
  * Zugangsdaten liegen nur im Client (Tresor-Datei) und erreichen nie den Agenten.
  * Firmenspezifische Rollen-Abbildungen gehören in die Konfiguration des Betreibers,
    nicht in dieses Repo.
  * Eigene Tools und Plugins sind reines Rust.
