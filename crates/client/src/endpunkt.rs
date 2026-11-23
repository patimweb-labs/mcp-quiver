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
//! Jede Anfrage braucht `Authorization: Bearer <agent-token>`; geprüft wird
//! der sha256-Hash in konstanter Zeit. Ohne gültiges Token: 401.

use std::sync::atomic::Ordering;
use std::sync::{Arc, MutexGuard};

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use quiver_domain::tool_name_zerlegen;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, Implementation, InitializeRequestParams,
    InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData as McpError, RoleServer};
use serde_json::json;

use crate::{Innen, Sitzung, CLIENT_VERSION};

/// Der Pfad, unter dem die Agenten den Endpunkt ansprechen.
pub const PFAD: &str = "/mcp";

/// Der Agent hinter einer Anfrage (vom Token in der Wache eingesetzt).
#[derive(Clone, Debug)]
struct AgentKennung(String);

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
    let Some(agent) = innen.agent_fuer_token(&token) else {
        return nicht_angemeldet();
    };
    anfrage.extensions_mut().insert(AgentKennung(agent));
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
}

impl Drop for Endpunkt {
    fn drop(&mut self) {
        let entfernt = self.sitzungen().remove(&self.schluessel).is_some();
        if entfernt {
            self.innen.modell_schicken();
        }
    }
}

/// Welcher Agent gehört zu dieser Sitzung? Das Token steckt in den
/// HTTP-Teilen, die rmcp an die Anfrage hängt.
fn agent_aus(context: &RequestContext<RoleServer>) -> Option<String> {
    let teile = context.extensions.get::<axum::http::request::Parts>()?;
    let kennung = teile.extensions.get::<AgentKennung>()?;
    Some(kennung.0.clone())
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
        let agent = agent_aus(&context);
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
        Ok(ListToolsResult::with_all_items(self.innen.werkzeuge()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.to_string();
        let Some((eintrag, werkzeug)) = tool_name_zerlegen(&name) else {
            return Err(McpError::invalid_params(
                format!("Werkzeug {name} ist unbekannt"),
                None,
            ));
        };
        let peer = self
            .innen
            .stand()
            .laeufer
            .get(&eintrag)
            .and_then(|laeufer| laeufer.peer.clone());
        let Some(peer) = peer else {
            return Err(McpError::invalid_params(
                format!("Werkzeug {name} läuft gerade nicht"),
                None,
            ));
        };
        let mut anfrage = CallToolRequestParams::new(werkzeug.to_string());
        anfrage.arguments = request.arguments;
        match peer.call_tool(anfrage).await {
            Ok(ergebnis) => Ok(CallToolResponse::Complete(ergebnis)),
            Err(fehler) => Err(McpError::internal_error(
                format!("Aufruf von {name}: {fehler}"),
                None,
            )),
        }
    }
}
