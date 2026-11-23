// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein Demo-MCP über stdio: bietet frei wählbare Werkzeuge an, die nichts tun
//! außer den Aufruf zu protokollieren. Damit lässt sich die Verteilung von
//! Ende zu Ende vorführen, ohne echte Dienste.
//!
//! Umgebung:
//! - `QUIVER_DEMO_NAME`: Name für das Protokoll (z. B. `mail`).
//! - `QUIVER_DEMO_TOOLS`: `name=Beschreibung;name2=Beschreibung2`.
//! - `QUIVER_DEMO_LOG`: optional, Datei, an die jede Zeile angehängt wird.
//! - `QUIVER_DEMO_GEHEIM`: optional; protokolliert wird nur, **ob** es gesetzt ist.
//!
//! Jede Zeile geht nach stderr (landet im Protokoll des Clients) und ggf. in die Datei.

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServiceExt};
use serde_json::json;
use std::io::Write;
use std::sync::Arc;

#[derive(Clone)]
struct Demo {
    name: String,
    werkzeuge: Vec<(String, String)>,
}

impl Demo {
    fn aus_umgebung() -> Self {
        let name = std::env::var("QUIVER_DEMO_NAME").unwrap_or_else(|_| "demo".into());
        let werkzeuge = std::env::var("QUIVER_DEMO_TOOLS")
            .unwrap_or_else(|_| "ausfuehren=Führt etwas aus (Demo)".into())
            .split(';')
            .filter_map(|teil| {
                let (n, b) = teil.split_once('=').unwrap_or((teil, "Demo-Werkzeug"));
                let n = n.trim();
                (!n.is_empty()).then(|| (n.to_string(), b.trim().to_string()))
            })
            .collect();
        Self { name, werkzeuge }
    }

    fn protokoll(&self, zeile: &str) {
        let zeile = format!("[{}] {zeile}", self.name);
        eprintln!("{zeile}");
        if let Ok(pfad) = std::env::var("QUIVER_DEMO_LOG") {
            if let Ok(mut datei) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(pfad)
            {
                let _ = writeln!(datei, "{zeile}");
            }
        }
    }
}

impl ServerHandler for Demo {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let schema = json!({
            "type": "object",
            "properties": { "text": { "type": "string", "description": "Beliebiger Text" } },
            "additionalProperties": true
        });
        let schema = Arc::new(schema.as_object().cloned().unwrap_or_default());
        Ok(ListToolsResult::with_all_items(
            self.werkzeuge
                .iter()
                .map(|(n, b)| Tool::new(n.clone(), b.clone(), schema.clone()))
                .collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let werkzeug = request.name.to_string();
        if !self.werkzeuge.iter().any(|(n, _)| *n == werkzeug) {
            return Err(McpError::invalid_params(
                format!("unbekannt: {werkzeug}"),
                None,
            ));
        }
        let argumente = serde_json::to_string(&request.arguments.unwrap_or_default())
            .unwrap_or_else(|_| "{}".into());
        self.protokoll(&format!("Aufruf {werkzeug} {argumente}"));
        Ok(CallToolResponse::Complete(CallToolResult::success(vec![
            ContentBlock::text(format!(
                "[{}] {werkzeug} wurde aufgerufen und protokolliert: {argumente}",
                self.name
            )),
        ])))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let demo = Demo::aus_umgebung();
    let geheim = match std::env::var("QUIVER_DEMO_GEHEIM") {
        Ok(w) if !w.is_empty() => format!("Geheimnis gesetzt ({} Zeichen)", w.chars().count()),
        _ => "kein Geheimnis".into(),
    };
    let namen: Vec<_> = demo.werkzeuge.iter().map(|(n, _)| n.as_str()).collect();
    demo.protokoll(&format!("gestartet, Werkzeuge: {}; {geheim}", namen.join(", ")));
    let dienst = demo.serve(rmcp::transport::stdio_pipes()).await?;
    dienst.waiting().await?;
    Ok(())
}
