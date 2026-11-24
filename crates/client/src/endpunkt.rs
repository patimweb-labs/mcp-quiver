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

use std::sync::atomic::Ordering;
use std::sync::{Arc, MutexGuard};
use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use quiver_domain::{tool_name_zerlegen, EintragId};
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
use crate::{Innen, Sitzung, CLIENT_VERSION};

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
    let Some(token) = bearer(anfrage.headers()) else {
        return nicht_angemeldet();
    };
    let Some((id, name)) = innen.agent_fuer_token(&token) else {
        return nicht_angemeldet();
    };
    anfrage.extensions_mut().insert(AgentKennung { id, name });
    weiter.run(anfrage).await
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

    /// Einen Aufruf protokollieren (Aufgabe 17). Ein Fehler beim Schreiben
    /// darf den Aufruf nie scheitern lassen – das erledigt [`crate::protokoll`].
    fn aufzeichnen(&self, aufzeichnung: Aufzeichnung<'_>) {
        self.innen.protokoll.aufruf_schreiben(aufzeichnung);
    }

    /// Die eigenen Werkzeuge von quiver (Aufgabe 17) – lesend, ohne Eintrag.
    fn eigenes_werkzeug(
        &self,
        name: &str,
        argumente: Option<&JsonObject>,
    ) -> Option<CallToolResult> {
        let schalter = self.innen.stand().protokoll_fuer_agenten;
        match name {
            TOOL_PROTOKOLL => Some(protokoll_lesen(
                self.innen.protokoll.abfragen(&filter_aus(argumente)),
                schalter,
            )),
            TOOL_LOG => Some(log_lesen(&self.innen.protokoll, argumente, schalter)),
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
fn log_lesen(
    protokoll: &Protokoll,
    argumente: Option<&JsonObject>,
    schalter: bool,
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
        .zeilen(&EintragId::neu(eintrag.clone()), &filter)
        .into_iter()
        .map(|zeile| json!({ "zeit": zeile.zeit, "text": zeile.text }))
        .collect();
    let antwort = json!({ "eintrag": eintrag, "zeilen": zeilen });
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
    agent: Option<String>,
    #[serde(default)]
    nur_fehler: bool,
    #[serde(default)]
    seit: Option<String>,
    #[serde(default)]
    anzahl: Option<usize>,
}

fn filter_aus(argumente: Option<&JsonObject>) -> Filter {
    let eingabe: FilterEingabe = eingabe_aus(argumente).unwrap_or_default();
    Filter {
        eintrag: eingabe
            .eintrag
            .filter(|wert| !wert.trim().is_empty())
            .map(EintragId::neu),
        tool: eingabe.tool.filter(|wert| !wert.trim().is_empty()),
        agent: eingabe.agent.filter(|wert| !wert.trim().is_empty()),
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
        "agent": { "type": "string", "description": "Name des Agenten" },
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
            "Zeigt die letzten Werkzeug-Aufrufe der Agenten an diesem Gerät \
             (Zeit, Agent, Werkzeug, Dauer, Ergebnis). Argumente und Antworten \
             nur, wenn der Schalter „Agenten dürfen Inhalte im Protokoll sehen“ an ist.",
            objekt(filtern),
        )
        .with_annotations(ToolAnnotations::new().read_only(true)),
        Tool::new(
            TOOL_LOG,
            "Zeigt die Prozesszeilen (stderr) eines Eintrags – zum Fehlersuchen. \
             Ohne den Schalter „Agenten dürfen Inhalte im Protokoll sehen“ leer.",
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
        self.sitzungen().insert(
            self.schluessel,
            Sitzung {
                agent,
                peer: context.peer.clone(),
            },
        );
        self.innen.modell_schicken();
        self.negotiate_initialize(&request)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
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
        let name = request.name.to_string();
        let agent = agent_aus(&context)
            .map(|kennung| kennung.name)
            .unwrap_or_default();
        let beginn = Instant::now();

        // Die eigenen Werkzeuge von quiver: lesend, ohne Eintrag – und sie
        // landen **nicht** im Protokoll, sonst entsteht eine Rückkopplung.
        if name.starts_with(EIGEN_PRAEFIX) {
            return match self.eigenes_werkzeug(&name, request.arguments.as_ref()) {
                Some(ergebnis) => Ok(CallToolResponse::Complete(ergebnis)),
                None => Err(McpError::invalid_params(
                    format!("Werkzeug {name} ist unbekannt"),
                    None,
                )),
            };
        }

        let (eintrag, werkzeug) = match tool_name_zerlegen(&name) {
            Some((eintrag, werkzeug)) => (eintrag, werkzeug.to_string()),
            None => {
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
            }
        };
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
        let mut anfrage = CallToolRequestParams::new(werkzeug);
        anfrage.arguments = request.arguments.clone();
        match peer.call_tool(anfrage).await {
            Ok(ergebnis) => {
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
                let text = format!("Aufruf von {name}: {fehler}");
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
        let filter = filter_aus(argumente.as_object());
        assert_eq!(filter.eintrag, Some(EintragId::neu("echo")));
        assert_eq!(filter.tool.as_deref(), Some("echo__echo"));
        assert_eq!(filter.agent.as_deref(), Some("pi"));
        assert!(filter.nur_fehler);
        assert_eq!(filter.seit.as_deref(), Some("2026-01-02T03:04:05Z"));
        assert_eq!(filter.anzahl, Some(5));

        // Ohne Angaben: die letzten 50.
        assert_eq!(
            filter_aus(None).anzahl,
            Some(PROTOKOLL_STANDARD),
            "Standard sind die letzten 50"
        );
        // Mehr als 200 gibt es nicht.
        let zuviel = json!({ "anzahl": 5000 });
        assert_eq!(filter_aus(zuviel.as_object()).anzahl, Some(PROTOKOLL_MAX));
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
        ));
        let protokoll = Protokoll::neu(&ordner);
        let ergebnis = log_lesen(&protokoll, Some(&JsonObject::new()), true);
        let text = antwort_text(&ergebnis);
        assert!(text.contains("eintrag"), "{text}");
        assert_eq!(ergebnis.is_error, Some(true));
        std::fs::remove_dir_all(&ordner).ok();
    }
}
