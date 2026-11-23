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
npm run bilder     # Bildschirmfotos (Server muss laufen)
```

Zum Ausprobieren im Betrieb: `cargo run -p quiver-server` im Wurzelverzeichnis,
dann `http://127.0.0.1:8740` öffnen. Der Admin-Token steht in
`daten/admin-token`.

## Aufbau

```
src/
  api/typen.ts        Vertrag, eins zu eins aus crates/protocol und crates/domain
  api/client.ts       fetch mit Bearer-Token; 401 führt zur Anmeldung
  logik/              reine Funktionen mit Tests (Zustände, Prüfung, Format)
  bausteine/          eigene Bausteine, gleiche Namen und Maße wie in
                      crates/oberflaeche/src/bausteine.rs
  ansichten/          Überblick, Bibliothek, Sätze, Gruppen, Geräte
  styles/tokens.css   erzeugt aus tokens/ – nicht von Hand ändern
  styles/schriften.css IBM Plex Sans/Mono aus crates/oberflaeche/schriften
```

Der Zustand einer Seite ist ein Summentyp (`Zustand<T>` in `logik/seite.ts`):
`laedt`, `fertig` mit Daten oder `fehler` mit Text. Die Ansichten entscheiden
nichts; Rechnen, Filtern und Prüfen steht in `logik/` und ist getestet.

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

## Bildschirmfotos

`npm run bilder` schreibt `docs/bilder/admin-{dark,light}-{bibliothek,geraete}.png`,
damit sie neben den egui-Fotos liegen. Es braucht einen laufenden Server mit
Daten und `admin/dist` (`npm run build`). Der Token kommt aus
`QUIVER_ADMIN_TOKEN` oder aus `daten/admin-token`; die Adresse aus
`QUIVER_BILD_BASIS` (Standard `http://127.0.0.1:8740`).
