# mcp-quiver

**English:** [README.md](README.md)

> **Stand: Konzept.** Noch nichts zu installieren – das Repo enthält bisher die
> Spezifikation ([SPEC.md](SPEC.md)).

**Ein Köcher voller Tools für alle deine Agenten.** mcp-quiver ist eine zentrale
Bibliothek von [MCP](https://modelcontextprotocol.io/)-Tools und ein kleiner
Client auf jedem Gerät. Admins legen in einer Weboberfläche fest, welche Tools
eine Gruppe bekommt. Der Client installiert und konfiguriert sie, und jeder
Agent auf dem Gerät – pi, Hermes und weitere – spricht mit genau einem
Endpunkt. Anmeldungen passieren einmal, im Client. Agenten sehen nie ein
Passwort oder ein Token.

Reines Rust: Server, Client und das Client-Fenster. Die Admin-Oberfläche ist
Vue und sieht genauso aus wie der Client.

## Die Idee

```
 pi-agent ──┐                        ┌──────────── quiver-server ────────────┐
 Hermes   ──┼──► quiver (Client) ◄──►│ Bibliothek: MCP-Tools (eigene+fremde)│
 …        ──┘    ein Endpunkt        │ Zuweisung über Entra-ID-Gruppen       │
                 führt Tools aus     │ Admin-Oberfläche (Vue)                │
                 hält die Logins     └───────────────────────────────────────┘
```

- **Bibliothek auf dem Server.** Eigene Tools (in Rust geschrieben) und fremde
  MCP-Server in einem Katalog, versioniert, auf Basis des offiziellen
  `server.json`-Formats der MCP Registry.
- **Zuweisung über Gruppen.** Ein Admin weist Tool-Sätze Gruppen in Microsoft
  Entra ID zu. Jedes Gerät bekommt, was die Gruppen seines Users erlauben –
  automatisch installiert, aktualisiert und entfernt.
- **Konfiguration in Schichten.** Der Server pusht, was er weiß (Endpunkte,
  Tenant, Vorgaben). Der Client ergänzt, was nur das Gerät weiß (lokale Pfade,
  persönliche Geheimnisse im Schlüsselbund des Betriebssystems).
- **Authentifizierung an einer Stelle.** Agent → Client und Client → Server,
  sonst nichts. Der Client fügt Zugangsdaten bei jedem Tool-Aufruf ein. Der
  Agent sieht immer nur Tools.
- **Microsoft zuerst.** Der erste Tool-Satz deckt Microsoft 365 über Microsoft
  Graph ab und handelt mit den Rechten des angemeldeten Users. So gilt das
  Rollen- und Rechtemodell von Microsoft unverändert.
- **Jeder Agent.** Agenten, die MCP sprechen, verbinden sich direkt. Für alle
  anderen gibt es Plugins, geschrieben in Rust (pi über ein Node-Binding,
  Hermes über ein Python-Binding).

## Plattformen

- **Server:** ein Container-Image (Docker, Azure Container Apps, überall).
- **Client:** macOS, Windows und Linux.

## Entwicklung

```bash
# alles prüfen (vor jedem Commit)
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

Die Token-Dateien in `tokens/` sind die einzige Quelle für Farben, Maße und
Schriften — für das Client-Fenster (egui) und für die Admin-Oberfläche (Vue).
Das CSS der Admin-Oberfläche wird daraus erzeugt und eingecheckt, nie von Hand
geändert:

```bash
cargo run -p quiver-oberflaeche --bin tokens_css > admin/src/styles/tokens.css
```

### Admin-Oberfläche

```bash
git submodule update --init admin/vendor/weppelmann-ui
cd admin
npm ci
npm run dev     # Vite auf 5173, /api → 127.0.0.1:8740
npm run build   # nach admin/dist; der Server liefert sie unter / aus
npm run typecheck
npm test
```

Aufbau, Bausteine und Bildschirmfotos: [admin/README.md](admin/README.md).

### Client-Fenster

```bash
cargo run -p quiver-fenster                # das Fenster
cargo run -p quiver-fenster -- --headless  # nur der Kern, bis Strg+C
```

Aufbau wie die Admin-Oberfläche: Kopf, Navigation links, Inhalt als Karten, drei
Themes aus denselben Tokens. Die Bildschirmfotos erzeugt der Oberflächentest
(mit Attrappe, ohne Server und ohne Laufzeit):

```bash
QUIVER_BILDER=docs/bilder cargo test -p quiver-fenster --test oberflaeche
QUIVER_BILDER=docs/bilder cargo test -p quiver-oberflaeche --test muster
```

Der `muster`-Test zeichnet die Vergleichsseite, die es in der Verwaltung unter
`/muster` gibt (nur im Entwicklungsbau); beide lesen dieselben Token-Dateien,
deshalb unterscheiden sich die Bilder nur in der Schriftglättung:

![Musterseite, egui](docs/bilder/muster-egui-dark.png)
![Musterseite, Vue](docs/bilder/muster-vue-dark.png)

![Client-Fenster, dunkel](docs/bilder/fenster-dark-tools.png)
![Client-Fenster, hell](docs/bilder/fenster-light-tools.png)

## Lizenz

[MIT](LICENSE)
