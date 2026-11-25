// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der MCP-Endpunkt für die Agenten.
//!
//! Liegt auf `http://127.0.0.1:<port>/mcp` (Streamable HTTP) und bietet **alle**
//! Werkzeuge aller laufenden Einträge an, benannt nach
//! [`quiver_domain::tool_name`] (`echo__echo`). Der Aufruf wird unverändert an
//! den Kindprozess bzw. den entfernten Server weitergereicht – Zugangsdaten
//! fügt der Client vorher ein, der Agent sieht sie nie.
//!
//! Dazu kommen zwei eigene Werkzeuge mit dem Präfix `quiver__` (Aufgabe 17):
//! `quiver__protokoll` und `quiver__tool_log`. Beide lesen nur und liefern ohne
//! den Schalter „Agenten dürfen Inhalte im Protokoll sehen“ bloß Metadaten.
//!
//! Jede Anfrage braucht `Authorization: Bearer <agent-token>`; geprüft wird
//! der sha256-Hash in konstanter Zeit. Ohne gültiges Token: 401.
//!
//! Grenzen (Paket 3):
//!
//! - **Herkunft**: eine Anfrage mit `Origin` kommt nur von Loopback (ein
//!   Browser auf diesem Rechner), der `Host` muss der eigene Port sein. Ohne
//!   `Origin` ist sie erlaubt – dann ist es kein Browser.
//! - **Auslastung**: höchstens [`crate::Auslastung::GESAMT_MAX`] Aufrufe
//!   gleichzeitig und [`crate::Auslastung::EINTRAG_MAX`] je Eintrag; darüber
//!   antwortet der Client sofort mit „ausgelastet“.
//! - **Frist und Abbruch**: jeder Aufruf hat eine Frist, Fortschrittsmeldungen
//!   verlängern sie, und ein Abbruch des Agenten bricht auch den Aufruf beim
//!   Tool ab.
//! - **Größe**: eine Antwort höchstens [`ANTWORT_BYTES`]; gezählt wird sie
//!   ohne sie zusammenzukleben.
//! - **Sitzungen**: höchstens [`crate::SITZUNGEN_MAX`], sie laufen nach
//!   [`crate::SITZUNG_FRIST`] ohne Nutzung ab.
//! - **Protokoll**: ein Agent sieht **nur eigene** Aufrufe, und `quiver__tool_log`
//!   nur für Einträge, die er selbst benutzt hat.

use std::sync::atomic::Ordering;
use std::sync::{Arc, MutexGuard};
use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use quiver_domain::EintragId;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    InitializeRequestParams, InitializeResult, JsonObject, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData as McpError, RoleServer};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::protokoll::{Aufruf, Aufzeichnung, Filter, LogFilter, Protokoll, Status};
use crate::{Auslastung, Innen, Sitzung, CLIENT_VERSION};

/// So groß darf die Antwort eines Werkzeugaufrufs höchstens sein.
pub const ANTWORT_BYTES: usize = 8 * 1024 * 1024;

/// Der Pfad, unter dem die Agenten den Endpunkt ansprechen.
pub const PFAD: &str = "/mcp";

/// Das Präfix der eigenen Werkzeuge von quiver.
pub const EIGEN_PRAEFIX: &str = "quiver__";
/// Das Protokoll der Aufrufe, aus Agentensicht.
pub const TOOL_PROTOKOLL: &str = "quiver__protokoll";
/// Die Prozesszeilen eines Eintrags, aus Agentensicht.
pub const TOOL_LOG: &str = "quiver__tool_log";
/// So viele Aufrufe liefert `quiver__protokoll` ohne Angabe.
pub const PROTOKOLL_STANDARD: usize = 50;
/// So viele höchstens.
pub const PROTOKOLL_MAX: usize = 200;
/// So viele Zeilen liefert `quiver__tool_log` ohne Angabe.
pub const LOG_STANDARD: usize = 100;
/// So viele höchstens.
pub const LOG_MAX: usize = 500;

/// Der Hinweis, wenn der Schalter aus ist (Aufgabe 17).
const HINWEIS_ZU: &str = "Inhalte sind gesperrt. Im Fenster unter „Agenten“ lässt sich \
                         „Agenten dürfen Inhalte im Protokoll sehen“ einschalten; \
                         bis dahin gibt es nur Metadaten.";

/// Der Agent hinter einer Anfrage (vom Token in der Wache eingesetzt).
#[derive(Clone, Debug)]
struct AgentKennung {
    id: String,
    name: String,
}

/// Der fertige Router: MCP-Dienst plus Token-Wache.
pub(crate) fn router(innen: Arc<Innen>) -> Router {
    let dienst = StreamableHttpService::new(
        {
            let innen = innen.clone();
            move || Ok(Endpunkt::neu(innen.clone()))
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    Router::new()
        .nest_service(PFAD, dienst)
        .layer(middleware::from_fn_with_state(innen.clone(), wache))
        .with_state(innen)
}

/// Nur mit gültigem Agent-Token durch; merkt sich, wer fragt.
async fn wache(State(innen): State<Arc<Innen>>, mut anfrage: Request, weiter: Next) -> Response {
    // Herkunft zuerst: eine Anfrage aus einem fremden Browser soll nicht
    // einmal erfahren, ob ein Token stimmt.
    if let Some(ablehnung) = herkunft_ablehnen(anfrage.headers(), innen.adresse) {
        return ablehnung;
    }
    let Some(token) = bearer(anfrage.headers()) else {
        return nicht_angemeldet();
    };
    let Some((id, name)) = innen.agent_fuer_token(&token) else {
        return nicht_angemeldet();
    };
    anfrage.extensions_mut().insert(AgentKennung { id, name });
    weiter.run(anfrage).await
}

/// Ist der Host ein Loopback-Name?
fn loopback_host(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

/// `host[:port]` → (Host ohne Klammern, Port).
fn host_und_port(wert: &str) -> (String, Option<u16>) {
    // IPv6 in Klammern: `[::1]:8741`.
    if let Some(rest) = wert.strip_prefix('[') {
        if let Some((host, danach)) = rest.split_once(']') {
            let port = danach.strip_prefix(':').and_then(|p| p.parse().ok());
            return (host.to_string(), port);
        }
    }
    match wert.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            (host.to_string(), port.parse().ok())
        }
        _ => (wert.to_string(), None),
    }
}

/// Prüft `Origin` und `Host` (Paket 3).
///
/// - Mit `Origin`: nur Loopback (`http://127.0.0.1:…`, `http://localhost:…`).
///   Eine Seite von außen kann so keine Aufrufe auslösen, auch wenn sie das
///   Token kennt.
/// - Ohne `Origin`: erlaubt – dann ist es kein Browser (CLI-Agent).
/// - `Host`: nur der eigene Port. So kann ein DNS-Rebinding nicht auf einen
///   anderen Namen zeigen, der auf 127.0.0.1 auflöst.
fn herkunft_ablehnen(kopf: &HeaderMap, adresse: std::net::SocketAddr) -> Option<Response> {
    if let Some(origin) = kopf.get(header::ORIGIN).and_then(|wert| wert.to_str().ok()) {
        let erlaubt = match origin {
            "null" => false,
            wert => match reqwest::Url::parse(wert) {
                Ok(url) => {
                    matches!(url.scheme(), "http" | "https")
                        && url.host_str().is_some_and(loopback_host)
                }
                Err(_) => false,
            },
        };
        if !erlaubt {
            return Some(
                (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "fehler": "Nur Anfragen von diesem Rechner sind erlaubt" })),
                )
                    .into_response(),
            );
        }
    }
    if let Some(host) = kopf.get(header::HOST).and_then(|wert| wert.to_str().ok()) {
        let (name, port) = host_und_port(host);
        if !loopback_host(&name) || port != Some(adresse.port()) {
            return Some(
                (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "fehler": "Unbekannter Host" })),
                )
                    .into_response(),
            );
        }
    }
    None
}

fn bearer(kopf: &HeaderMap) -> Option<String> {
    let wert = kopf.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (art, token) = wert.split_once(' ')?;
    if !art.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn nicht_angemeldet() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "fehler": "Token fehlt oder passt nicht" })),
    )
        .into_response()
}

/// Der MCP-Dienst hinter dem Endpunkt. Je Sitzung eine eigene Instanz.
struct Endpunkt {
    innen: Arc<Innen>,
    schluessel: u64,
}

impl Endpunkt {
    fn neu(innen: Arc<Innen>) -> Self {
        let schluessel = innen.zaehler.fetch_add(1, Ordering::Relaxed);
        Self { innen, schluessel }
    }

    fn sitzungen(&self) -> MutexGuard<'_, std::collections::BTreeMap<u64, Sitzung>> {
        self.innen
            .sitzungen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    /// Diese Sitzung ist benutzt worden – sie läuft nicht mehr ab (Paket 3).
    fn benutzt(&self) {
        let mut sitzungen = self.sitzungen();
        if let Some(sitzung) = sitzungen.get_mut(&self.schluessel) {
            sitzung.zuletzt = Instant::now();
        }
    }

    /// Einen Aufruf belegen; `None` heißt ausgelastet.
    fn auslastung_belegen(&self, eintrag: &EintragId) -> Option<AuslastungWache> {
        let mut auslastung = self
            .innen
            .auslastung
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        if !auslastung.belegen(eintrag) {
            return None;
        }
        Some(AuslastungWache {
            innen: self.innen.clone(),
            eintrag: eintrag.clone(),
        })
    }

    /// Einen Aufruf protokollieren (Aufgabe 17). Ein Fehler beim Schreiben
    /// darf den Aufruf nie scheitern lassen – das erledigt [`crate::protokoll`].
    fn aufzeichnen(&self, aufzeichnung: Aufzeichnung<'_>) {
        self.innen.protokoll.aufruf_schreiben(aufzeichnung);
    }

    /// Die Meldung für einen auf diesem Gerät abgeschalteten Eintrag
    /// (Aufgabe 19) – `None`, wenn er laufen darf.
    fn abgeschaltet_text(&self, eintrag: &EintragId) -> Option<String> {
        let stand = self.innen.stand();
        if !stand.am_geraet_aus.contains(eintrag) {
            return None;
        }
        let titel = stand
            .soll
            .eintraege
            .iter()
            .find(|soll| &soll.eintrag.id == eintrag)
            .map(|soll| soll.eintrag.titel.clone())
            .unwrap_or_else(|| eintrag.als_str().to_string());
        Some(format!(
            "{titel} ist auf diesem Gerät abgeschaltet – im quiver-Fenster unter Tools einschalten."
        ))
    }

    /// Dasselbe für einen Werkzeugnamen, der nicht mehr in der Tabelle steht:
    /// ein abgeschalteter Eintrag bietet keine Werkzeuge mehr an, ist aber
    /// **nicht** unbekannt (Aufgabe 19). Der Name wird hier nur für die
    /// Meldung zugeordnet – die Zuordnung der Aufrufe bleibt die Tabelle.
    fn abgeschaltet_fuer_name(&self, name: &str) -> Option<String> {
        let treffer = {
            let stand = self.innen.stand();
            stand
                .soll
                .eintraege
                .iter()
                .find(|eintrag| {
                    let id = &eintrag.eintrag.id;
                    stand.am_geraet_aus.contains(id)
                        && name
                            .strip_prefix(&format!("{}__", id.als_str()))
                            .is_some_and(|rest| !rest.is_empty())
                })
                .map(|eintrag| eintrag.eintrag.id.clone())
        };
        self.abgeschaltet_text(&treffer?)
    }

    /// Die eigenen Werkzeuge von quiver (Aufgabe 17) – lesend, ohne Eintrag.
    ///
    /// Ein Agent sieht **nur eigene** Aufrufe (Paket 3): der Agenten-Filter
    /// wird auf ihn gesetzt, egal was im Argument steht.
    fn eigenes_werkzeug(
        &self,
        name: &str,
        argumente: Option<&JsonObject>,
        agent: Option<&AgentKennung>,
    ) -> Option<CallToolResult> {
        let schalter = self.innen.stand().protokoll_fuer_agenten;
        let agent_name = agent.map(|kennung| kennung.name.clone());
        match name {
            TOOL_PROTOKOLL => Some(protokoll_lesen(
                self.innen
                    .protokoll
                    .abfragen(&filter_aus(argumente, agent_name)),
                schalter,
            )),
            TOOL_LOG => Some(log_lesen(
                &self.innen.protokoll,
                argumente,
                schalter,
                agent_name.as_deref(),
            )),
            _ => None,
        }
    }
}

/// Die Liste der Aufrufe als Antwort für einen Agenten.
fn protokoll_lesen(aufrufe: Vec<Aufruf>, schalter: bool) -> CallToolResult {
    let liste: Vec<Value> = aufrufe
        .iter()
        .map(|aufruf| {
            let mut wert = json!({
                "zeit": &aufruf.zeit,
                "agent": &aufruf.agent,
                "eintrag": aufruf.eintrag.als_str(),
                "tool": &aufruf.tool,
                "dauer_ms": aufruf.dauer_ms,
                "status": aufruf.status.als_str(),
                "fehler": &aufruf.fehler,
            });
            if schalter {
                let objekt = wert.as_object_mut().expect("Objekt");
                objekt.insert("argumente".into(), json!(&aufruf.argumente));
                objekt.insert("antwort_groesse".into(), json!(aufruf.antwort_groesse));
                objekt.insert("vorschau".into(), json!(&aufruf.vorschau));
            }
            wert
        })
        .collect();
    let mut antwort = json!({ "aufrufe": liste });
    if !schalter {
        antwort["hinweis"] = json!(HINWEIS_ZU);
    }
    CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string_pretty(&antwort).unwrap_or_default(),
    )])
}

/// Die Prozesszeilen eines Eintrags als Antwort für einen Agenten.
///
/// Ein Agent bekommt nur Zeilen von Einträgen, die **er selbst** benutzt hat
/// (Paket 3): die Prozessausgabe eines fremden Werkzeugs kann Zugangsdaten
/// oder Inhalte anderer Agenten enthalten.
fn log_lesen(
    protokoll: &Protokoll,
    argumente: Option<&JsonObject>,
    schalter: bool,
    agent: Option<&str>,
) -> CallToolResult {
    let eingabe: LogEingabe = match eingabe_aus(argumente) {
        Ok(eingabe) => eingabe,
        Err(text) => return CallToolResult::error(vec![ContentBlock::text(text)]),
    };
    let Some(eintrag) = eingabe.eintrag.filter(|wert| !wert.trim().is_empty()) else {
        return CallToolResult::error(vec![ContentBlock::text(
            "Es fehlt „eintrag“ – die Kennung des Eintrags.".to_string(),
        )]);
    };
    let eintrag = EintragId::neu(eintrag.clone());
    if !protokoll.von_agent_benutzt(agent, &eintrag) {
        return CallToolResult::error(vec![ContentBlock::text(
            "Diesen Eintrag hast du nicht benutzt – fremde Prozesszeilen sind nicht lesbar."
                .to_string(),
        )]);
    }
    if !schalter {
        let antwort = json!({ "zeilen": [], "hinweis": HINWEIS_ZU });
        return CallToolResult::success(vec![ContentBlock::text(
            serde_json::to_string_pretty(&antwort).unwrap_or_default(),
        )]);
    }
    let filter = LogFilter {
        enthaelt: eingabe.enthaelt.filter(|wert| !wert.trim().is_empty()),
        seit: eingabe.seit,
        anzahl: Some(eingabe.anzahl.unwrap_or(LOG_STANDARD).min(LOG_MAX)),
    };
    let zeilen: Vec<Value> = protokoll
        .zeilen(&eintrag, &filter)
        .into_iter()
        .map(|zeile| json!({ "zeit": zeile.zeit, "text": zeile.text }))
        .collect();
    let antwort = json!({ "eintrag": eintrag.als_str(), "zeilen": zeilen });
    CallToolResult::success(vec![ContentBlock::text(
        serde_json::to_string_pretty(&antwort).unwrap_or_default(),
    )])
}

fn eingabe_aus<T: for<'de> Deserialize<'de> + Default>(
    argumente: Option<&JsonObject>,
) -> Result<T, String> {
    match argumente {
        Some(argumente) => serde_json::from_value(Value::Object(argumente.clone()))
            .map_err(|fehler| format!("Die Angaben passen nicht: {fehler}")),
        None => Ok(T::default()),
    }
}

/// Die Eingaben von `quiver__protokoll`.
#[derive(Debug, Default, Deserialize)]
struct FilterEingabe {
    #[serde(default)]
    eintrag: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    nur_fehler: bool,
    #[serde(default)]
    seit: Option<String>,
    #[serde(default)]
    anzahl: Option<usize>,
}

fn filter_aus(argumente: Option<&JsonObject>, agent: Option<String>) -> Filter {
    let eingabe: FilterEingabe = eingabe_aus(argumente).unwrap_or_default();
    Filter {
        eintrag: eingabe
            .eintrag
            .filter(|wert| !wert.trim().is_empty())
            .map(EintragId::neu),
        tool: eingabe.tool.filter(|wert| !wert.trim().is_empty()),
        // Immer der Agent dieser Sitzung – egal, was als Argument kam.
        agent,
        nur_fehler: eingabe.nur_fehler,
        seit: eingabe.seit.filter(|wert| !wert.trim().is_empty()),
        anzahl: Some(
            eingabe
                .anzahl
                .unwrap_or(PROTOKOLL_STANDARD)
                .min(PROTOKOLL_MAX),
        ),
    }
}

/// Die Eingaben von `quiver__tool_log`.
#[derive(Debug, Default, Deserialize)]
struct LogEingabe {
    #[serde(default)]
    eintrag: Option<String>,
    #[serde(default)]
    enthaelt: Option<String>,
    #[serde(default)]
    seit: Option<String>,
    #[serde(default)]
    anzahl: Option<usize>,
}

/// Die zwei eigenen Werkzeuge für `tools/list` (Aufgabe 17).
fn eigene_werkzeuge() -> Vec<Tool> {
    let objekt = |eigenschaften: Value| {
        let schema = json!({
            "type": "object",
            "properties": eigenschaften,
            "additionalProperties": false
        });
        schema.as_object().cloned().unwrap_or_default()
    };
    let filtern = json!({
        "eintrag": { "type": "string", "description": "Kennung des Eintrags, z. B. echo" },
        "tool": { "type": "string", "description": "Werkzeugname, wie der Agent ihn aufruft" },
        "nur_fehler": { "type": "boolean", "description": "Nur Fehler und Ablehnungen" },
        "seit": { "type": "string", "description": "Anfang als RFC 3339, z. B. 2026-01-02T03:04:05Z" },
        "anzahl": { "type": "integer", "minimum": 1, "maximum": PROTOKOLL_MAX,
                    "description": "Höchstens so viele, die neuesten zuerst" }
    });
    let log = json!({
        "eintrag": { "type": "string", "description": "Kennung des Eintrags (Pflicht)" },
        "enthaelt": { "type": "string", "description": "Nur Zeilen mit diesem Text" },
        "seit": { "type": "string", "description": "Anfang als RFC 3339" },
        "anzahl": { "type": "integer", "minimum": 1, "maximum": LOG_MAX,
                    "description": "Höchstens so viele, die neuesten zuerst" }
    });
    vec![
        Tool::new(
            TOOL_PROTOKOLL,
            "Zeigt die letzten eigenen Werkzeug-Aufrufe dieses Agenten (Zeit, \
             Agent, Werkzeug, Dauer, Ergebnis). Argumente und Antworten nur, \
             wenn der Schalter „Agenten dürfen Inhalte im Protokoll sehen“ an ist.",
            objekt(filtern),
        )
        .with_annotations(ToolAnnotations::new().read_only(true)),
        Tool::new(
            TOOL_LOG,
            "Zeigt die Prozesszeilen (stderr) eines Eintrags, den dieser Agent \
             selbst benutzt hat – zum Fehlersuchen. Ohne den Schalter „Agenten \
             dürfen Inhalte im Protokoll sehen“ leer.",
            objekt(log),
        )
        .with_annotations(ToolAnnotations::new().read_only(true)),
    ]
}

/// Welcher Agent gehört zu dieser Sitzung? Das Token steckt in den
/// HTTP-Teilen, die rmcp an die Anfrage hängt.
fn agent_aus(context: &RequestContext<RoleServer>) -> Option<AgentKennung> {
    let teile = context.extensions.get::<axum::http::request::Parts>()?;
    let kennung = teile.extensions.get::<AgentKennung>()?;
    Some(kennung.clone())
}

/// Der Text einer Werkzeugantwort (für Größe und Vorschau).
fn antwort_text(ergebnis: &CallToolResult) -> String {
    let mut text = String::new();
    for inhalt in &ergebnis.content {
        if let Some(text_inhalt) = inhalt.as_text() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&text_inhalt.text);
        }
    }
    text
}

/// Wie groß eine Antwort ist – **ohne** sie zusammenzukleben (Paket 3).
///
/// Textblöcke zählen ihre Länge, alles andere wird gezählt, ohne eine Kopie
/// der ganzen Antwort zu bauen.
pub fn antwort_groesse(ergebnis: &CallToolResult) -> usize {
    let mut groesse = 0usize;
    for inhalt in &ergebnis.content {
        match inhalt {
            ContentBlock::Text(text) => groesse = groesse.saturating_add(text.text.len()),
            anderes => {
                let anzahl = serde_json::to_vec(anderes)
                    .map(|bytes| bytes.len())
                    .unwrap_or(0);
                groesse = groesse.saturating_add(anzahl);
            }
        }
    }
    if let Some(struktur) = &ergebnis.structured_content {
        let anzahl = serde_json::to_vec(struktur)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        groesse = groesse.saturating_add(anzahl);
    }
    groesse
}

/// Gibt den belegten Aufruf wieder frei, auch wenn der Aufruf scheitert.
struct AuslastungWache {
    innen: Arc<Innen>,
    eintrag: EintragId,
}

impl Drop for AuslastungWache {
    fn drop(&mut self) {
        self.innen
            .auslastung
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .freigeben(&self.eintrag);
    }
}

impl ServerHandler for Endpunkt {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
        .with_server_info(Implementation::new("mcp-quiver", CLIENT_VERSION))
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        context.peer.set_peer_info(request.clone());
        let agent = agent_aus(&context).map(|kennung| kennung.id);
        let mut sitzungen = self.sitzungen();
        // Abgelaufene und zu viele Sitzungen weichen (Paket 3): höchstens
        // 64, und keine bleibt länger als 10 Minuten ohne Nutzung.
        loop {
            let zeiten: std::collections::BTreeMap<u64, Instant> = sitzungen
                .iter()
                .map(|(schluessel, sitzung)| (*schluessel, sitzung.zuletzt))
                .collect();
            let Some(weicht) = crate::sitzung_weicht(&zeiten, Instant::now()) else {
                break;
            };
            sitzungen.remove(&weicht);
        }
        sitzungen.insert(
            self.schluessel,
            Sitzung {
                agent,
                peer: context.peer.clone(),
                zuletzt: Instant::now(),
            },
        );
        drop(sitzungen);
        self.innen.modell_schicken();
        self.negotiate_initialize(&request)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.benutzt();
        let mut werkzeuge = self.innen.werkzeuge();
        werkzeuge.extend(eigene_werkzeuge());
        werkzeuge.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(ListToolsResult::with_all_items(werkzeuge))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.benutzt();
        let name = request.name.to_string();
        let kennung = agent_aus(&context);
        let agent = kennung
            .as_ref()
            .map(|kennung| kennung.name.clone())
            .unwrap_or_default();
        let beginn = Instant::now();

        // Die eigenen Werkzeuge von quiver: lesend, ohne Eintrag – und sie
        // landen **nicht** im Protokoll, sonst entsteht eine Rückkopplung.
        if name.starts_with(EIGEN_PRAEFIX) {
            return match self.eigenes_werkzeug(&name, request.arguments.as_ref(), kennung.as_ref())
            {
                Some(ergebnis) => Ok(CallToolResponse::Complete(ergebnis)),
                None => Err(McpError::invalid_params(
                    format!("Werkzeug {name} ist unbekannt"),
                    None,
                )),
            };
        }

        // Die Zuordnung steht in einer **Tabelle** (Paket 3) – kein Zerlegen
        // am `__`, und doppelte Namen sind schon beim Aufbau verworfen.
        let tabelle = self.innen.werkzeugtabelle();
        let Some((eintrag, werkzeug)) = tabelle.get(&name).cloned() else {
            // Kein laufendes Werkzeug – vielleicht ist der Eintrag nur
            // abgeschaltet (Aufgabe 19). Das ist kein „unbekannt“.
            if let Some(text) = self.abgeschaltet_fuer_name(&name) {
                self.aufzeichnen(Aufzeichnung {
                    eintrag: &EintragId::neu(name.clone()),
                    tool: &name,
                    agent: &agent,
                    dauer: beginn.elapsed(),
                    status: Status::Abgelehnt,
                    fehler: Some(&text),
                    argumente: request.arguments.as_ref(),
                    antwort: None,
                });
                return Err(McpError::invalid_params(text, None));
            }
            let text = format!("Werkzeug {name} ist unbekannt");
            self.aufzeichnen(Aufzeichnung {
                eintrag: &EintragId::neu(name.clone()),
                tool: &name,
                agent: &agent,
                dauer: beginn.elapsed(),
                status: Status::Abgelehnt,
                fehler: Some(&text),
                argumente: request.arguments.as_ref(),
                antwort: None,
            });
            return Err(McpError::invalid_params(text, None));
        };
        // Ein auf diesem Gerät abgeschalteter Eintrag ist **nicht** unbekannt:
        // er existiert, der User will ihn nur nicht (Aufgabe 19).
        if let Some(text) = self.abgeschaltet_text(&eintrag) {
            self.aufzeichnen(Aufzeichnung {
                eintrag: &eintrag,
                tool: &name,
                agent: &agent,
                dauer: beginn.elapsed(),
                status: Status::Abgelehnt,
                fehler: Some(&text),
                argumente: request.arguments.as_ref(),
                antwort: None,
            });
            return Err(McpError::invalid_params(text, None));
        }
        let peer = self
            .innen
            .stand()
            .laeufer
            .get(&eintrag)
            .and_then(|laeufer| laeufer.peer.clone());
        let Some(peer) = peer else {
            let text = format!("Werkzeug {name} läuft gerade nicht");
            self.aufzeichnen(Aufzeichnung {
                eintrag: &eintrag,
                tool: &name,
                agent: &agent,
                dauer: beginn.elapsed(),
                status: Status::Abgelehnt,
                fehler: Some(&text),
                argumente: request.arguments.as_ref(),
                antwort: None,
            });
            return Err(McpError::invalid_params(text, None));
        };
        // Auslastung (Paket 3): zu viele gleichzeitige Aufrufe werden sofort
        // abgewiesen, statt alle zu verlangsamen.
        let wache = match self.auslastung_belegen(&eintrag) {
            Some(wache) => wache,
            None => {
                let text = format!(
                    "Ausgelastet: höchstens {} Aufrufe gleichzeitig, je Eintrag {} – bitte später erneut",
                    Auslastung::GESAMT_MAX,
                    Auslastung::EINTRAG_MAX
                );
                self.aufzeichnen(Aufzeichnung {
                    eintrag: &eintrag,
                    tool: &name,
                    agent: &agent,
                    dauer: beginn.elapsed(),
                    status: Status::Abgelehnt,
                    fehler: Some(&text),
                    argumente: request.arguments.as_ref(),
                    antwort: None,
                });
                return Err(McpError::internal_error(text, None));
            }
        };
        let mut anfrage = CallToolRequestParams::new(werkzeug);
        anfrage.arguments = request.arguments.clone();
        let frist = self.innen.aufruf_frist(&eintrag);
        let abbruch = context.ct.clone();
        let ergebnis = crate::laufzeit::aufruf(
            &peer,
            anfrage,
            frist,
            crate::laufzeit::AUFRUF_FRIST_MAX,
            async move { abbruch.cancelled().await },
        )
        .await;
        drop(wache);
        match ergebnis {
            Ok(ergebnis) => {
                let groesse = antwort_groesse(&ergebnis);
                if groesse > ANTWORT_BYTES {
                    let text = format!(
                        "Die Antwort von {name} ist zu groß ({groesse} Byte, erlaubt sind {ANTWORT_BYTES}). \
                         Bitte die Menge einschränken."
                    );
                    self.aufzeichnen(Aufzeichnung {
                        eintrag: &eintrag,
                        tool: &name,
                        agent: &agent,
                        dauer: beginn.elapsed(),
                        status: Status::Fehler,
                        fehler: Some(&text),
                        argumente: request.arguments.as_ref(),
                        antwort: None,
                    });
                    return Err(McpError::internal_error(text, None));
                }
                let text = antwort_text(&ergebnis);
                self.aufzeichnen(Aufzeichnung {
                    eintrag: &eintrag,
                    tool: &name,
                    agent: &agent,
                    dauer: beginn.elapsed(),
                    status: if ergebnis.is_error.unwrap_or(false) {
                        Status::Fehler
                    } else {
                        Status::Ok
                    },
                    fehler: None,
                    argumente: request.arguments.as_ref(),
                    antwort: Some(&text),
                });
                Ok(CallToolResponse::Complete(ergebnis))
            }
            Err(fehler) => {
                // Wurde der Eintrag während des Aufrufs abgeschaltet, endet er
                // mit der klaren Meldung der Aufgabe – nicht mit „Prozess weg“.
                let text = match self.abgeschaltet_text(&eintrag) {
                    Some(text) => text,
                    None => format!("Aufruf von {name}: {fehler}"),
                };
                self.aufzeichnen(Aufzeichnung {
                    eintrag: &eintrag,
                    tool: &name,
                    agent: &agent,
                    dauer: beginn.elapsed(),
                    status: Status::Fehler,
                    fehler: Some(&text),
                    argumente: request.arguments.as_ref(),
                    antwort: None,
                });
                Err(McpError::internal_error(text, None))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_filter_liest_die_eingaben_und_begrenzt_sie() {
        let argumente = json!({
            "eintrag": "echo",
            "tool": "echo__echo",
            "agent": "pi",
            "nur_fehler": true,
            "seit": "2026-01-02T03:04:05Z",
            "anzahl": 5
        });
        let filter = filter_aus(argumente.as_object(), Some("pi".to_string()));
        assert_eq!(filter.eintrag, Some(EintragId::neu("echo")));
        assert_eq!(filter.tool.as_deref(), Some("echo__echo"));
        assert_eq!(filter.agent.as_deref(), Some("pi"));
        assert!(filter.nur_fehler);
        assert_eq!(filter.seit.as_deref(), Some("2026-01-02T03:04:05Z"));
        assert_eq!(filter.anzahl, Some(5));

        // Ohne Angaben: die letzten 50.
        assert_eq!(
            filter_aus(None, None).anzahl,
            Some(PROTOKOLL_STANDARD),
            "Standard sind die letzten 50"
        );
        // Mehr als 200 gibt es nicht.
        let zuviel = json!({ "anzahl": 5000 });
        assert_eq!(
            filter_aus(zuviel.as_object(), None).anzahl,
            Some(PROTOKOLL_MAX)
        );
    }

    #[test]
    fn ohne_schalter_stehen_nur_metadaten_in_der_antwort() {
        let aufruf = Aufruf {
            zeit: "2026-01-02T03:04:05Z".into(),
            agent: "pi".into(),
            eintrag: EintragId::neu("echo"),
            tool: "echo__echo".into(),
            dauer_ms: 3,
            status: Status::Ok,
            fehler: None,
            argumente: r#"{"text":"hallo"}"#.into(),
            antwort_groesse: 5,
            vorschau: "hallo".into(),
        };
        let zu = protokoll_lesen(vec![aufruf.clone()], false);
        let text = antwort_text(&zu);
        for erwartet in ["zeit", "agent", "dauer_ms", "status", "hinweis"] {
            assert!(text.contains(erwartet), "{erwartet} fehlt: {text}");
        }
        for gesperrt in ["argumente", "vorschau", "hallo"] {
            assert!(!text.contains(gesperrt), "{gesperrt} ist zu sehen: {text}");
        }

        let auf = protokoll_lesen(vec![aufruf], true);
        let text = antwort_text(&auf);
        assert!(text.contains("argumente"), "{text}");
        assert!(text.contains("hallo"), "{text}");
        assert!(!text.contains("hinweis"), "kein Hinweis nötig: {text}");
    }

    #[test]
    fn beide_werkzeuge_sind_lesend_und_haben_ein_objektschema() {
        let werkzeuge = eigene_werkzeuge();
        let namen: Vec<&str> = werkzeuge.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(namen, vec![TOOL_PROTOKOLL, TOOL_LOG]);
        for werkzeug in &werkzeuge {
            assert_eq!(
                werkzeug.input_schema.get("type"),
                Some(&Value::String("object".into())),
                "{} ohne type: object",
                werkzeug.name
            );
            assert_eq!(
                werkzeug.annotations.as_ref().and_then(|a| a.read_only_hint),
                Some(true),
                "{} ist nicht als lesend markiert",
                werkzeug.name
            );
        }
    }

    #[test]
    fn ein_log_ohne_eintrag_ist_ein_fehler() {
        let ordner = std::env::temp_dir().join(format!(
            "quiver-endpunkt-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|dauer| dauer.as_nanos())
                .unwrap_or(0)
        ));
        let protokoll = Protokoll::neu(&ordner);
        let ergebnis = log_lesen(&protokoll, Some(&JsonObject::new()), true, Some("pi"));
        let text = antwort_text(&ergebnis);
        assert!(text.contains("eintrag"), "{text}");
        assert_eq!(ergebnis.is_error, Some(true));
        std::fs::remove_dir_all(&ordner).ok();
    }
}
