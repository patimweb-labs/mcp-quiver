# 06 – Admin-Oberfläche (Vue)

Ordner: `admin/` (Vue 3, Vite, TypeScript, `vue-tsc`). Wird vom Server unter `/` ausgeliefert
(`admin/dist`). **Muss 1:1 aussehen wie das Client-Fenster** (AGENTS.md §2.11):
gleiche Tokens, gleiche Schrift (IBM Plex Sans/Mono aus `crates/oberflaeche/schriften`),
gleiche Abstände, gleiche Bausteine, gleicher Aufbau (Kopf, Navigation links, Karten).

## Grundlage

- **Tokens:** `admin/src/styles/tokens.css` (erzeugt in Aufgabe 02, nie von Hand ändern).
  Theme über `data-theme` am `<html>`.
- **Bausteine:** aus `weppelmann-ui` (`https://github.com/Smotherer007/weppelmann-ui`,
  `packages/components`). Einbinden als Git-Submodul unter `admin/vendor/weppelmann-ui`
  und Vite-Alias `@weppelmann/components` → `vendor/weppelmann-ui/packages/components/src`.
  Prüfe, welche CSS-Variablen die Bausteine erwarten; fehlen welche in unserer `tokens.css`,
  **nicht** in `tokens.css` ergänzen, sondern in deine Antwort schreiben (dann ergänzen wir
  die Token-Dateien in `tokens/`).
- Wo ein Baustein fehlt, eigener in `admin/src/bausteine/` – gleicher Name und gleiche Maße
  wie in `crates/oberflaeche/src/bausteine.rs`, `cursor: pointer` im Baustein für alles Klickbare.
- Keine Farben, Größen oder Abstände in Ansichten, nur `var(--wp-…)`.

## API

`admin/src/api/` – Typen in `typen.ts` **genau** nach `crates/protocol/src/lib.rs` und
`crates/domain/src/lib.rs` (serde-Namen beachten: `kebab-case`-Enums, `art`-Tag bei `Laufzeit`).
Ein Fetch-Wrapper mit `Authorization: Bearer <admin-token>`. Admin-Token: Anmeldeseite,
gespeichert in `sessionStorage`. 401 → zurück zur Anmeldung.
Im Dev-Modus Proxy `/api` → `http://127.0.0.1:8740`.

## Seiten

1. **Überblick:** Zähler aus `/admin/ueberblick`, Geräte mit Problemen (Fehler/braucht Angaben).
2. **Bibliothek:** Liste der Einträge (Suche). Neu/Bearbeiten: id, Titel, Beschreibung, Version,
   Herkunft, Laufzeit (Stdio: Befehl, Argumente, Umgebung / Remote: URL, Kopfzeilen) und
   Felder (Name, Beschreibung, Quelle server/client/geheim, Pflicht, Standard).
   „Importieren“: server.json hochladen oder einfügen → `/admin/eintraege/import`.
3. **Sätze:** Titel, Priorität, Einträge hinzufügen; je Eintrag Eingabe nur für dessen
   **Server**-Felder.
4. **Gruppen:** Titel, Sätze zuordnen (Mehrfachauswahl).
5. **Geräte:** Tabelle (Name, Gruppen, verbunden, zuletzt gesehen, Soll-Revision ≠ gemeldete
   Revision als Hinweis „wird aktualisiert“), aufklappbar mit Zustand je Eintrag.
   Gruppen ändern, sperren, entfernen. „Gerät einbinden“ → Gruppen wählen → Code groß
   anzeigen mit Kopieren und Ablaufzeit.
6. **Kopf:** Wortmarke `mcp-quiver`, Theme-Auswahl (dark/light/mono), Abmelden.

Zustand der Seiten als diskriminierte Unions (`{ art: 'laedt' } | { art: 'fertig', daten } | …`),
Logik in reinen TS-Funktionen mit Tests (`vitest`).

## Abnahme

- `npm ci && npm run build && npm run typecheck && npm test` in `admin/` grün.
- Mit laufendem `cargo run -p quiver-server`: Eintrag anlegen, Satz, Gruppe, Code erzeugen –
  alles über die Oberfläche möglich.
- Bildschirmfotos (Playwright, `npm run bilder`) nach `docs/bilder/admin-{dark,light}-{bibliothek,geraete}.png`,
  damit wir sie neben die egui-Fotos legen und vergleichen können.
