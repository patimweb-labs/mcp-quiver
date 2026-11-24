# 09 – Agenten nur einmal koppeln, Schnipsel je Agent, Einträge abschalten

## Teil A – Client: jeder Agent nur einmal (crates/client, crates/fenster)

**Problem:** Man kann „pi“ zweimal koppeln und bekommt zwei Konfigurationen. Verbinden dürfen
sich beliebig viele Sitzungen eines Agenten (mehrere pi-Fenster) – **koppeln** nur einmal.

1. `Kern::agent_koppeln(name)` lehnt einen Namen ab, den es schon gibt (Vergleich ohne
   Groß-/Kleinschreibung und ohne Leerzeichen am Rand) → Fehler „Agent <name> ist schon gekoppelt“.
2. Neu: `Kern::agent_token_erneuern(id) -> anyhow::Result<String>` – gleicher Agent, neues Token,
   das alte gilt sofort nicht mehr (offene Sitzungen mit dem alten Token werden beim nächsten
   Aufruf mit 401 abgewiesen).
3. **Agent-Arten** als Summentyp im Fenster (`logik.rs`):
   `Pi, ClaudeCode, OpenCode, Copilot, Hermes, Andere(String)`. Der Name ist bei bekannten Arten fest
   (`pi`, `claude-code`, `opencode`, `copilot`, `hermes`).
4. **Koppeln-Dialog:** Auswahl der Art als Liste/Segmentauswahl (Bausteine, Hand-Cursor).
   Schon gekoppelte Arten sind ausgegraut mit Hinweis „schon gekoppelt“. „Andere“ hat ein
   Namensfeld, doppelte Namen werden vor dem Absenden angezeigt (reine Funktion + Test).
5. In der Agentenliste je Agent: „Token erneuern“ (Rückfrage) und „Entfernen“.
6. **Schnipsel je Art** nach dem Koppeln/Erneuern (Endpunkt `E`, Token `T`), jeweils mit
   `kopierbar` und einer Zeile, wo man ihn einfügt:
   - pi: `pi mcp add quiver --url E --header "Authorization=Bearer T"`
   - Claude Code: `claude mcp add --transport http quiver E --header "Authorization: Bearer T"`
   - OpenCode: in `opencode.json`:
     `{"mcp":{"quiver":{"type":"remote","url":"E","headers":{"Authorization":"Bearer T"}}}}`
   - GitHub Copilot (VS Code): in `.vscode/mcp.json` bzw. Benutzer-`mcp.json`:
     `{"servers":{"quiver":{"type":"http","url":"E","headers":{"Authorization":"Bearer T"}}}}`
   - Hermes: in `~/.hermes/config.yaml` unter `mcp_servers:`
     ```yaml
       quiver:
         url: E
         headers:
           Authorization: Bearer T
         enabled: true
     ```
     danach `hermes mcp test quiver`.
   - Andere: das allgemeine JSON wie bisher.
   **Prüfe die Syntax** mit den installierten CLIs, soweit vorhanden (`pi mcp add --help`,
   `claude mcp add --help`, `opencode --help`, `hermes mcp add --help`) und schreibe ins Ergebnis,
   was du prüfen konntest. Nichts davon **ausführen** – keine echten Konfigurationen ändern.
7. Bestehende Profile mit doppelten Namen: beim Laden bleiben sie erhalten (nichts löschen),
   aber neue Duplikate gehen nicht mehr.

## Teil B – Admin: Einträge abschalten (admin/, crates/server)

Im Datenmodell gibt es jetzt `Eintrag.aktiv` (Standard `true`, siehe `crates/domain`).
Abgeschaltete Einträge laufen auf keinem Gerät; der Client beendet sie beim nächsten Abgleich.

1. Bibliothek: Schalter „Aktiv“ im Bearbeiten-Formular, in der Liste ein Abzeichen „Abgeschaltet“
   (Ton leise), dazu ein schneller Schalter in der Zeile (Hand-Cursor).
2. `admin/src/api/typen.ts` um `aktiv` ergänzen.
3. Server: Integrationstest – Eintrag abschalten → Soll des Geräts enthält ihn nicht mehr, und
   das SSE-Ereignis kommt.
4. Client-Ende-zu-Ende-Test (`crates/client/tests/ende.rs`) erweitern: Eintrag abschalten →
   innerhalb von 5 s ist das Tool aus `list_tools` verschwunden und der Prozess beendet.

## Abnahme

Prüfbefehl grün, in `admin/` typecheck, test und build grün. Bildschirmfotos neu erzeugen
(Fenster: Koppeln-Dialog mit Auswahl; Admin: Bibliothek mit einem abgeschalteten Eintrag).
