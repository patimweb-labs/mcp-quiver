# 05 – Client-Fenster (egui)

Crate: `crates/fenster` (`quiver-fenster`, Binärname `quiver`). Vorbild für Aufbau und
Optik: `crates/fenster` in **wledambilight** und **sprechwerk**
(`gh api repos/patimweb-labs/sprechwerk/contents/crates/fenster/src/app.rs -q .content | base64 -d`).
Gleicher Stil: Kopfzeile, Navigation links, Inhalt als Karten, Bausteine aus `quiver-oberflaeche`.

## Abhängigkeiten

`eframe = { version = "0.37", default-features = false, features = ["default_fonts", "glow", "accesskit", "wayland", "x11"] }`,
`quiver-oberflaeche`, `quiver-client`, `quiver-domain`, `quiver-protocol`, `tokio`, `anyhow`, `tracing`, `tracing-subscriber`.
Dev: `egui_kittest = { version = "0.37", features = ["snapshot", "wgpu"] }`.

## Aufbau (datenorientiert, AGENTS.md §2)

- `modell.rs`: `Sicht` = `quiver_client::Modell` + reiner UI-Zustand (gewählte Seite, offener Dialog,
  Eingaben, Theme). Kein anderer Zustand.
- `logik.rs`: reine Funktionen (z. B. welche Seite bei `NichtEingebunden` gezeigt wird,
  Statusfarbe aus `EintragZustand`) – mit Unit-Tests.
- `app.rs`: `render(&Sicht) → Vec<Aktion>`; Aktionen gehen an ein Trait
  ```rust
  pub trait KernZugang { fn ausfuehren(&self, a: Aktion); fn modell(&self) -> quiver_client::Modell; }
  ```
  echt: über `quiver_client::Kern` (Tokio-Runtime im Hintergrund-Thread), im Test: Attrappe.
- `ansichten/`: `kopf.rs`, `tools.rs`, `agenten.rs`, `verbindung.rs`, `dialog.rs`.

## Seiten

1. **Verbindung** (Startseite, solange `NichtEingebunden`): Server-Adresse, Einmal-Code,
   Knopf „Gerät einbinden“. Danach Status (verbunden/getrennt mit Fehlertext) und „Gerät abmelden“.
2. **Tools**: eine Karte je Eintrag: Icon `Paket`, Titel, Beschreibung, Status-Abzeichen
   (`status`-Baustein: ok = läuft mit Anzahl Tools, warn = braucht Angaben, error = Fehler,
   leise = startet). Klick auf „Braucht Angaben“ oder Stift → Dialog mit einem Feld je
   Client-/Geheim-Feld (Geheim als Passwortfeld; gesetzte Geheimnisse zeigen „••••“,
   leer lassen = unverändert). Aufgeklappt: Liste der Tool-Namen. Leer → `leer`-Baustein.
3. **Agenten**: Liste mit Name und Punkt (verbunden ja/nein), „Entfernen“. „Agent koppeln“ →
   Dialog: Name → zeigt Endpunkt und Token (je mit `kopierbar`) und den Hinweis, dass das Token
   nur einmal sichtbar ist, dazu zwei fertige Schnipsel zum Kopieren:
   - pi: `pi mcp add quiver --url <endpunkt> --header "Authorization: Bearer <token>"`
     (**Syntax mit `pi mcp --help` prüfen und anpassen**)
   - allgemein (JSON für MCP-Clients): `{"mcpServers":{"quiver":{"type":"http","url":"…","headers":{"Authorization":"Bearer …"}}}}`
4. **Kopf**: Wortmarke (`marke`), Verbindungsstatus, Theme-Auswahl (dark/light/mono), Sprache (de/en).
   Theme und Sprache werden im Profil gemerkt.

Fenster schließen beendet den Kern sauber (`kern.beenden()`).
`quiver --headless`: nur Kern, ohne Fenster, bis Strg+C (für Server-Rechner und Tests).

## Abnahme

- `tests/oberflaeche.rs` mit `egui_kittest` und Attrappe:
  - Startseite bei `NichtEingebunden` ist „Verbindung“.
  - Tools-Seite zeigt drei Karten mit den drei Zuständen; Klick auf „Braucht Angaben“ öffnet den Dialog.
  - Agent koppeln zeigt Token und Endpunkt.
  - **Hand-Cursor** über jedem Knopf der Tools-Seite.
- Bildschirmfotos: mit `QUIVER_BILDER=docs/bilder` erzeugt der Test
  `docs/bilder/fenster-{dark,light}-{tools,agenten,verbindung}.png`.
- Prüfbefehl grün. `cargo run -p quiver-fenster` öffnet das Fenster.
