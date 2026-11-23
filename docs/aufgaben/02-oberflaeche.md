# 02 – Bausatz: Themes, Texte, Bausteine, Tokens für Vue

Crate: `crates/oberflaeche` (`quiver-oberflaeche`). Er ist aus sprechwerk
kopiert und muss an mcp-quiver angepasst werden. Die alten Dateien liegen als
`src/lib.rs.sw` und `build.rs.sw` daneben.

## Ziel

1. Der Bausatz baut und kennt nichts mehr von sprechwerk.
2. Die Token-Dateien in `tokens/` (Wurzel des Repos) sind die **einzige** Quelle für
   egui **und** für die Vue-Admin-Oberfläche (AGENTS.md §2.11).
3. Der Hand-Cursor auf allem Klickbaren ist durch einen Test abgesichert (AGENTS.md §2.12).

## Schritte

1. `lib.rs.sw` → `src/lib.rs`, `build.rs.sw` → `build.rs`. `build.rs` liest die Tokens aus
   `../../tokens/` statt `tokens/` (`cargo:rerun-if-changed` mit anpassen).
2. `Cargo.toml`: Abhängigkeiten wie in sprechwerk:
   `egui = { version = "0.37", default-features = false, features = ["default_fonts", "monochrome_emoji_fonts"] }`,
   `egui_extras = { version = "0.37", default-features = false, features = ["svg"] }`,
   `serde_json` (normal und als build-dependency). Kein `sprechwerk-domain`.
   Für Tests: `egui_kittest = { version = "0.37" }` als dev-dependency.
3. Raus, weil sprechwerk-spezifisch: `rollenfarben`, `rollenabzeichen`, `pegelbalken`,
   `pegelregler`, `wert_aus_position`, `rolle_kurz`, `rolle` und alles, was
   `sprechwerk_domain` braucht. Icons, die nur zu Sprache/Audio gehören
   (mic, mic_off, headphones, headphone_off, speaker, sprechblase, react, smiley, film,
   send, anhang), samt ihren `Icon`-Varianten.
4. **Logo:** Es gibt noch keins (kommt vom Nutzer). `marke()` zeichnet bis dahin die
   Wortmarke `mcp-quiver` als Text in `thema::fett()`, Größe `FONT_SIZE_LG`, Farbe
   `text_default`. Alles, was `assets/sprechwerk-*.svg` einbindet (auch das
   Fenster-Icon in `thema.rs`), wird entfernt; das Fenster-Icon gibt `None` bzw. fällt weg.
5. **Neue Icons** (Lucide, ISC-Lizenz liegt bei; SVG von
   `https://unpkg.com/lucide-static@latest/icons/<name>.svg` holen und wie die anderen
   ablegen): `package` (Eintrag), `layers` (Satz), `users` (Gruppe, gibt es schon),
   `monitor` (Gerät), `key-round` (Geheimnis), `bot` (Agent), `check`, `circle-alert`,
   `pencil`, `upload` (Import), `search`. `Icon`-Varianten mit deutschen Namen:
   `Paket`, `Satz`, `Gruppe`, `Geraet`, `Schluessel`, `Agent`, `Haken`, `Hinweis`,
   `Bearbeiten`, `Hochladen`, `Suchen`.
6. **Texte:** `texte.json` nur noch `de` und `en` (`Sprache` hat nur `De`, `En`).
   Inhalt ersetzen durch die Schlüssel, die der Client braucht (Liste unten).
   Bestehende allgemeine Schlüssel (z. B. Einstellungen, Theme-Namen) darfst du behalten,
   wenn sie noch passen.
7. **Tokens für Vue:** gemeinsamer Code zum Lesen der Tokens in `src/tokens_lesen.rs`
   (wird von `build.rs` per `include!` und vom Programm unten benutzt – keine
   doppelte Logik). Neues Programm `src/bin/tokens_css.rs`: schreibt nach stdout eine
   CSS-Datei:
   ```css
   /* Erzeugt von `cargo run -p quiver-oberflaeche --bin tokens_css` – nicht von Hand ändern. */
   :root, :root[data-theme="dark"] { --wp-color-bg-canvas: #090C10; … }
   :root[data-theme="light"] { … }
   :root[data-theme="mono"] { … }
   :root { --wp-space-4: 16px; --wp-radius-md: …; --wp-font-size-md: …; --wp-duration-…: …ms; … }
   ```
   Namen: `--wp-` + Pfad, `.` → `-` (z. B. `color.text.on-accent` → `--wp-color-text-on-accent`).
   Farben aufgelöst wie in `build.rs`. Maße mit ihrer Einheit aus der Token-Datei.
   Ausgabe nach `admin/src/styles/tokens.css` (Ordner anlegen) und einchecken.
8. Test `tests/tokens_css.rs`: erzeugtes CSS == `admin/src/styles/tokens.css`.
   Schlägt fehl mit dem Hinweis, den Befehl aus Schritt 7 auszuführen.
9. Test `tests/cursor.rs` mit `egui_kittest`: ein `hauptknopf`, ein `knopf`, ein
   `icon_knopf` und eine Checkbox nach `thema::anwenden`; mit der Maus darüber ist
   `ctx.output(|o| o.cursor_icon) == CursorIcon::PointingHand`.

## Texte, die der Client braucht (Schlüssel → de / en)

```
app.titel            mcp-quiver / mcp-quiver
app.einstellungen    Einstellungen / Settings
nav.tools            Tools / Tools
nav.agenten          Agenten / Agents
nav.verbindung       Verbindung / Connection
verbindung.server    Server-Adresse / Server address
verbindung.code      Einmal-Code / One-time code
verbindung.einbinden Gerät einbinden / Connect device
verbindung.verbunden Verbunden mit {} / Connected to {}
verbindung.getrennt  Nicht verbunden / Not connected
verbindung.trennen   Gerät abmelden / Disconnect device
tools.leer           Noch keine Tools / No tools yet
tools.leer_text      Sobald ein Admin dir Tools zuweist, erscheinen sie hier. / Tools assigned to you by an admin will show up here.
tools.laeuft         Läuft · {} Tools / Running · {} tools
tools.startet        Startet … / Starting …
tools.wartet         Braucht Angaben / Needs input
tools.fehler         Fehler / Error
tools.werte_titel    Angaben für {} / Settings for {}
tools.speichern      Speichern / Save
agenten.leer         Noch kein Agent gekoppelt / No agent paired yet
agenten.koppeln      Agent koppeln / Pair agent
agenten.name         Name des Agenten / Agent name
agenten.endpunkt     Endpunkt / Endpoint
agenten.token        Token / Token
agenten.entfernen    Entfernen / Remove
agenten.hinweis      Das Token wird nur einmal angezeigt. / The token is shown only once.
allg.kopieren        Kopieren / Copy
allg.kopiert         Kopiert / Copied
allg.abbrechen       Abbrechen / Cancel
thema.dark           Dunkel / Dark
thema.light          Hell / Light
thema.mono           Mono / Mono
```

## Abnahme

- `grep -ri sprechwerk crates/oberflaeche` findet nichts außer ggf. einer Herkunftsnotiz im Modulkommentar.
- Beide neuen Tests grün, Prüfbefehl grün.
- `admin/src/styles/tokens.css` existiert und ist aktuell.
