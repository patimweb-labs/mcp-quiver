# 04 – Client-Kern: Abgleich, Laufzeiten, MCP-Endpunkt

Crate: `crates/client` (`quiver-client`), nur Bibliothek, **keine Oberfläche**.
Das Fenster (Aufgabe 05) hängt sich an die Fassade [`Kern`].

## Das wichtigste Abnahmekriterium

> Ein Admin weist ein MCP zu → **höchstens 5 Sekunden später** sieht ein Agent, der am
> Client hängt, dessen Tools und kann sie aufrufen. Ohne Neustart von irgendetwas.

## Abhängigkeiten (nur diese)

`rmcp = { version = "3.11", features = ["client", "server", "transport-child-process", "transport-streamable-http-server", "transport-streamable-http-client-reqwest"] }`
(Feature-Namen in der Doku von rmcp 3.11 prüfen – im Cargo-Register unter `~/.cargo/registry/src`),
`reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }`,
`axum = "0.8"`, `tokio` (workspace), `futures-util`, `keyring = "4"`, `dirs = "6"`,
`serde`, `serde_json`, `sha2`, `hex`, `rand = "0.9"`, `anyhow`, `thiserror`, `tracing`,
`quiver-domain`, `quiver-protocol`.
Dev: `quiver-server` (für den Ende-zu-Ende-Test), `tower`.

## Fassade

```rust
pub struct KernKonfig {
    pub profil: PathBuf,            // Standard: dirs::config_dir()/mcp-quiver
    pub mcp_adresse: SocketAddr,    // Standard: 127.0.0.1:8741
    pub tresor: Arc<dyn Tresor>,    // Standard: Schlüsselbund; in Tests: im Speicher
}

pub trait Tresor: Send + Sync {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>>;
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()>;
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()>;
}
pub struct Schluesselbund;   // keyring, Dienst "mcp-quiver"
pub struct SpeicherTresor;   // HashMap hinter Mutex, für Tests

#[derive(Clone)]
pub struct Kern { /* Handle, billig klonbar */ }

impl Kern {
    /// Startet Hintergrundaufgaben (Abgleich, MCP-Endpunkt) und lädt den gespeicherten Stand.
    pub async fn starten(k: KernKonfig) -> anyhow::Result<Kern>;
    /// Aktuelles Modell für die Oberfläche; ändert sich bei jedem Ereignis.
    pub fn modell(&self) -> tokio::sync::watch::Receiver<Modell>;
    pub async fn einbinden(&self, server: &str, code: &str, name: &str) -> anyhow::Result<()>;
    pub async fn abmelden(&self) -> anyhow::Result<()>;
    pub async fn werte_setzen(&self, eintrag: &EintragId, lokal: Werte, geheim: Werte) -> anyhow::Result<()>;
    /// Gibt das Token **einmal** zurück; gespeichert wird nur sein Hash.
    pub async fn agent_koppeln(&self, name: &str) -> anyhow::Result<String>;
    pub async fn agent_entfernen(&self, id: &str) -> anyhow::Result<()>;
    pub async fn beenden(&self);
}
```

Das `Modell` (serde-serialisierbar, `Clone`, `PartialEq`) – genau das zeigt das Fenster:

```rust
pub struct Modell {
    pub verbindung: Verbindung,
    pub eintraege: Vec<EintragSicht>,
    pub agenten: Vec<AgentSicht>,
    pub endpunkt: String,                    // "http://127.0.0.1:8741/mcp"
}
pub enum Verbindung {
    NichtEingebunden,
    Verbindet { server: String },
    Verbunden { server: String, geraet: String },
    Getrennt { server: String, fehler: String },   // offline: letzter Stand läuft weiter
}
pub struct EintragSicht {
    pub id: EintragId, pub titel: String, pub beschreibung: String,
    pub zustand: quiver_protocol::EintragZustand,
    pub fehlende: Vec<quiver_domain::FehlendesFeld>,
    pub felder: Vec<quiver_domain::Feld>,   // nur Client- und Geheim-Felder, zum Bearbeiten
    pub lokale_werte: Werte,                // Geheimes NIE hier, nur ob gesetzt:
    pub geheim_gesetzt: Vec<String>,
    pub tools: Vec<String>,                 // Namen ohne Präfix
}
pub struct AgentSicht { pub id: String, pub name: String, pub verbunden: bool }
```

## Speicherung im Profil

- `<profil>/zustand.json`: Server-URL, eigenes `Geraet`, lokale Werte je Eintrag,
  Agenten (`id`, `name`, `token_hash`), letzter `Soll` (für offline).
- Im Tresor: `geraet-token`, Geheimnisse als `wert/<eintrag>/<feld>`.

## Abgleich

1. Eingebunden → `GET /geraet/soll`, dann SSE `GET /geraet/ereignisse` offen halten
   (SSE selbst parsen: Zeilen `event:` / `data:`, Leerzeile beendet ein Ereignis).
   Neue Revision → `GET /geraet/soll` → `quiver_domain::abgleichen` → Aktionen ausführen.
2. Nach jedem Abgleich und jeder Zustandsänderung eines Eintrags `POST /geraet/meldung`.
3. Verbindung weg → `Getrennt`, letzter Soll läuft weiter, neu verbinden mit Backoff 1 s … 30 s.
4. Pro Eintrag: `konfig_zusammensetzen` mit lokalen Werten und Geheimnissen.
   `Err(fehlend)` → `WartetAufWerte`. `Ok(laufzeit)` →
   - `Stdio`: Kindprozess über rmcp (`TokioChildProcess`), Umgebung aus der Laufzeit,
     stderr ins Log. Prozess stirbt → `Fehler`, Neustart mit Backoff (max 30 s).
   - `Remote`: rmcp Streamable-HTTP-Client mit den Kopfzeilen.
   - danach `list_tools` → `Laeuft { tools }`.
5. `werte_setzen` → betroffenen Eintrag sofort neu abgleichen.

## MCP-Endpunkt für Agenten

- rmcp-Server über Streamable HTTP unter `http://<mcp_adresse>/mcp`, **nur** `127.0.0.1`.
- Jede Anfrage braucht `Authorization: Bearer <agent-token>` (sha256 gegen gespeicherte Hashes,
  Vergleich in konstanter Zeit), sonst 401.
- `list_tools`: alle Tools aller laufenden Einträge, Name `quiver_domain::tool_name(eintrag, tool)`,
  Beschreibung und Schema unverändert.
- `call_tool`: Name mit `tool_name_zerlegen` auflösen, an den Kindprozess/Remote weiterreichen,
  Antwort unverändert zurück. Unbekannt → MCP-Fehler.
- Capabilities: `tools.list_changed = true`. Ändert sich die Tool-Liste (Start, Stopp,
  Neustart), bekommen alle verbundenen Agenten `notifications/tools/list_changed`.
- `AgentSicht.verbunden`: Agent hat eine offene Sitzung.

## Testhilfe

`crates/client/src/bin/quiver-echo-mcp.rs`: kleiner rmcp-stdio-Server mit einem Tool `echo`
(Parameter `text`, Antwort derselbe Text). Im Integrationstest über
`env!("CARGO_BIN_EXE_quiver-echo-mcp")` starten. Er dient auch zum Ausprobieren von Hand.

## Abnahme

- Unit-Tests: SSE-Parser, Modell-Ableitung (Soll + lokale Werte → `EintragSicht`), Agent-Token-Prüfung.
- **Ende-zu-Ende-Test** `tests/ende.rs`: `quiver_server::app` auf einem freien Port, Kern mit
  `SpeicherTresor` und temporärem Profil, freier MCP-Port.
  1. Admin legt Eintrag `echo` (Stdio → `quiver-echo-mcp`), Satz, Gruppe, Code an.
  2. `kern.einbinden(...)`, `kern.agent_koppeln("test")`.
  3. rmcp-Client über Streamable HTTP mit dem Token → `list_tools` ist leer oder enthält `echo__echo`.
  4. Admin ordnet den Satz der Gruppe **jetzt erst** zu → innerhalb von 5 s enthält
     `list_tools` `echo__echo`, und `call_tool("echo__echo", {"text":"hallo"})` liefert `hallo`.
  5. Eintrag mit Geheim-Feld → Zustand `WartetAufWerte`; nach `werte_setzen` → `Laeuft`.
  6. Ohne Token → 401.
- Prüfbefehl grün.
