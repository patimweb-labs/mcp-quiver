# 07 – egui und Vue 1:1 angleichen

AGENTS.md §2.11: Client (egui) und Admin (Vue) müssen **gleich aussehen**. Ein Vergleich der
Bildschirmfotos in `docs/bilder/` (`fenster-*` gegen `admin-*`) zeigt Abweichungen:

| Teil | Vue (Admin) | egui (Fenster) |
|---|---|---|
| Kopf | Wortmarke, rechts „Farbschema“ als Segmentauswahl, gewählt = Akzentfläche | Wortmarke + Status, Theme als einzelne Knöpfe, gewählt = graue Fläche; Sprache ebenso |
| Seitenleiste | Abschnitt „VERWALTUNG“, Zeilen ≈ 29 px, gewählt = Hover-Fläche + 2-px-Akzentbalken links | Abschnitt „NAVIGATION“, Zeilen ≈ 40 px, gewählt = Auswahlfläche, kein Balken |
| Knöpfe | Hauptknopf ≈ 32 px hoch, Schrift md | Hauptknopf ≈ 28 px, kleinere Schrift |
| Kopfhöhe | ≈ 62 px | ≈ 44 px |

## Ziel

Ein **gemeinsames Maß** für Kopf, Seitenleiste, Segmentauswahl und Knöpfe, festgelegt in den
Token-Dateien – beide Oberflächen lesen es von dort.

## Schritte

1. Neue Maß-Tokens in `tokens/primitives.tokens.json` (Gruppe `size`), z. B.
   `size.kopf` (Kopfhöhe), `size.seitenleiste` (Breite, 192), `size.navzeile` (Zeilenhöhe),
   `size.akzentbalken` (2), `size.control.md` / `size.control.sm` falls noch nicht vorhanden.
   Werte: nimm die der Vue-Oberfläche als Maß, sie wirkt ruhiger. `admin/src/styles/tokens.css`
   neu erzeugen (`cargo run -p quiver-oberflaeche --bin tokens_css > admin/src/styles/tokens.css`).
2. **Ein** Muster je Teil, in beiden Welten gleich gebaut:
   - Kopf: Wortmarke links; rechts Segmentauswahl Farbschema (gewählt = Akzentfläche,
     Schrift `text-on-accent`), daneben im egui-Fenster die Segmentauswahl Sprache, im Admin
     „Abmelden“. Der Verbindungsstatus im egui-Kopf bleibt, im gleichen `status`-Stil.
   - Seitenleiste: Abschnittsname klein in Großbuchstaben (`VERWALTUNG` bzw. `GERÄT`),
     Zeilen mit Icon + Text, gewählt = Hover-Fläche + Akzentbalken links.
   - Hauptknopf und Knopf: gleiche Höhe, gleiche Schriftgröße, gleiche Innenabstände.
   In egui gehören diese Muster in `crates/oberflaeche/src/bausteine.rs` (`kopf`,
   `seitenleiste`, `segmentauswahl`), in Vue in `admin/src/bausteine/` mit denselben Namen.
   Wo `weppelmann-ui` feste Pixelwerte hat, die abweichen: eigenen Baustein verwenden statt
   den aus `weppelmann-ui` (Vendor-Code nicht ändern).
3. **Musterseite** zum Vergleich: dieselbe Seite in beiden Welten, mit festen Beispieldaten:
   Kopf, Seitenleiste mit drei Einträgen, Seitenkopf, eine Karte mit Hauptknopf, Knopf,
   Statuszeilen (ok/warn/error) und einem Eingabefeld.
   - egui: Test in `crates/oberflaeche/tests/muster.rs` zeichnet sie mit `egui_kittest`
     (1200 × 800) → `docs/bilder/muster-egui-{dark,light}.png` (bei `QUIVER_BILDER=docs/bilder`).
   - Vue: Route `/muster` (nur im Dev-Build, ohne Anmeldung), Playwright 1200 × 800 →
     `docs/bilder/muster-vue-{dark,light}.png` (in `npm run bilder`).
4. Fenster- und Admin-Bildschirmfotos neu erzeugen.

## Abnahme

- Die vier Musterbilder liegen nebeneinander und unterscheiden sich nur in der Schriftglättung
  (gleiche Höhen, Breiten, Abstände, Farben). Leg in deiner Antwort für jeden Teil die
  gemessenen Höhen beider Seiten dar.
- Hand-Cursor-Test erweitert um Seitenleiste und Segmentauswahl.
- Prüfbefehl grün, `npm run build && npm run typecheck && npm test` in `admin/` grün.
