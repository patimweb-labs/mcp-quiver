# Admin-Oberfläche (Vue)

Die Verwaltung von mcp-quiver: Bibliothek, Sätze, Gruppen, Geräte. Sie wird vom
Server unter `/` ausgeliefert (`admin/dist`) und muss **1:1 aussehen wie das
Client-Fenster** (AGENTS.md §2.11): gleiche Tokens, gleiche Schrift, gleiche
Abstände, gleiche Bausteine.

## Einmal einrichten

Die Bausteine kommen aus [weppelmann-ui](https://github.com/Smotherer007/weppelmann-ui),
als Git-Submodul unter `vendor/weppelmann-ui`:

```bash
git submodule update --init admin/vendor/weppelmann-ui
cd admin
npm ci
```

## Befehle

```bash
npm run dev        # Vite auf 5173; /api geht an http://127.0.0.1:8740
npm run build      # nach admin/dist (der Server liefert sie unter / aus)
npm run typecheck  # vue-tsc
npm test           # vitest, reine Logik
npm run bilder     # Bildschirmfotos: Muster immer, Verwaltung mit Server
```

Zum Ausprobieren im Betrieb: `cargo run -p quiver-server` im Wurzelverzeichnis,
dann `http://127.0.0.1:8740` öffnen. Der Admin-Token steht in
`daten/admin-token`.

## Aufbau

```
src/
  api/typen.ts        Vertrag, eins zu eins aus crates/protocol und crates/domain
  api/client.ts       fetch mit Bearer-Token; 401 führt zur Anmeldung
  logik/              reine Funktionen mit Tests (Zustände, Prüfung, Format,
                      Breitenstufen)
  bausteine/          eigene Bausteine, gleiche Namen und Maße wie in
                      crates/oberflaeche/src/bausteine.rs: Kopf,
                      Seitenleiste, Segmentauswahl, Karte, Status, Feld, …
  ansichten/Muster.vue Musterseite für den Vergleich (nur im Dev-Bau)
  ansichten/          Überblick, Bibliothek, Sätze, Gruppen, Geräte
  styles/tokens.css   erzeugt aus tokens/ – nicht von Hand ändern
  styles/schriften.css IBM Plex Sans/Mono aus crates/oberflaeche/schriften
```

Der Zustand einer Seite ist ein Summentyp (`Zustand<T>` in `logik/seite.ts`):
`laedt`, `fertig` mit Daten oder `fehler` mit Text. Die Ansichten entscheiden
nichts; Rechnen, Filtern und Prüfen steht in `logik/` und ist getestet.

## Fenstergröße

Die Verwaltung folgt denselben Grenzen wie das Client-Fenster (Aufgabe 18):
`logik/breite.ts` misst das Fenster, liest die Grenzen aus den Tokens
(`--wp-breakpoint-formular`, `--wp-breakpoint-schmal`) und schreibt die Stufe als
`data-breite` an `<html>`. Unter *schmal* (760 px) trägt die Seitenleiste nur
die Zeichen — der Name steht in `title` und `aria-label` —, unter *eng* (700 px)
öffnet ein Zeichen im Kopf das Farbschema als Menü. Der Inhalt endet bei
`size.inhalt-max` (1100 px) und bleibt mittig; Formulare (`Feld.vue`) und
Werkzeugleisten brechen von sich aus um. Die Stufen rechnet `logik/breite.ts`
und ist mit `npm test` geprüft.

In der Bibliothek trägt jeder Eintrag ein `aktiv` (Standard `true`, siehe
`crates/domain`). Ein abgeschalteter Eintrag bleibt in Bibliothek und Sätzen,
läuft aber auf keinem Gerät: die Liste zeigt das Abzeichen *Abgeschaltet* und
der Schalter in der Zeile dreht den Wert ohne den Bearbeiten-Dialog.

Ein Eintrag kann außerdem eine *Anmeldung* und eine *Einrichtung* haben (beide
nur für `stdio`, beide als Argumentliste für dasselbe Programm). Die Liste zeigt
*mit Anmeldung* bzw. *mit Einrichtung*; im Bearbeiten-Dialog stehen dafür zwei
zugeklappte Abschnitte, die sich öffnen, sobald etwas drinsteht.

## Themes und Tokens

Drei Themes, sonst nichts: `dark`, `light`, `mono` (`data-theme` am `<html>`).
Farben, Maße und Schriften kommen ausschließlich aus `src/styles/tokens.css`.
In Ansichten steht kein Hex-Wert und kein Pixelmaß.

`weppelmann-ui` erwartet zusätzlich `--wp-fact-label` (Breite der
Beschriftungsspalte in `WpFactList`), mit Rückfall auf `130px`. Die Variable
fehlt in unseren Token-Dateien; sie gehört nach `tokens/`, nicht in die
erzeugte `tokens.css`. (Nicht von Hand nachtragen.)

## Hand-Cursor

Alles Klickbare zeigt den Hand-Cursor. Das kommt aus
`vendor/weppelmann-ui/packages/components/src/style.css` für native Elemente
und Komponenten und aus den eigenen Bausteinen (`cursor: pointer` im Baustein,
nie in der Ansicht).

## Musterseite

Unter `/muster` steht im Entwicklungsbau (`npm run dev`) dieselbe Seite wie im
Client-Fenster (`crates/oberflaeche/tests/muster.rs`): Kopf, Seitenleiste mit
drei Einträgen, Seitenkopf, Karte mit Hauptknopf und Knopf, drei Statuszeilen,
ein Eingabefeld. Sie braucht keine Anmeldung und zeigt keine Daten — sie ist
der Vergleich der Maße zwischen Vue und egui (AGENTS.md §2.11). Im gebauten
Stand gibt es sie nicht.

## Bildschirmfotos

`npm run bilder` schreibt zwei Sätze Bilder nach `docs/bilder/`:

- `muster-vue-{dark,light}.png` (1200 × 800) — dafür startet das Skript den
  Entwicklungs-Server selbst; es braucht weder Server noch Anmeldung. Das
  Gegenstück erzeugt `QUIVER_BILDER=docs/bilder cargo test -p quiver-oberflaeche
  --test muster`.
- `admin-{dark,light}-{bibliothek,geraete}.png` — dafür muss ein Server mit
  Daten laufen und `admin/dist` gebaut sein (`npm run build`). Der Token kommt
  aus `QUIVER_ADMIN_TOKEN` oder aus `daten/admin-token`; die Adresse aus
  `QUIVER_BILD_BASIS` (Standard `http://127.0.0.1:8740`). Fehlt der Token,
  bleiben diese Bilder unverändert.
