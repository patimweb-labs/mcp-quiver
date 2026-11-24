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
//! - `QUIVER_DEMO_GEHEIM_ECHO`: optional; mit `1` steht der Geheimwert auch in
//!   der Antwort (Aufgabe 17 prüft daran die Maske des Protokolls).
//! - `QUIVER_DEMO_FARBE`: optional; mit `1` bekommen die stderr-Zeilen
//!   ANSI-Farbcodes (das Protokoll muss sie entfernen).
//! - `QUIVER_DEMO_WARTE_AUF` + `QUIVER_DEMO_FREIGABE`: Testhilfe. Existiert die
//!   erste Datei beim Start, wird auf die zweite gewartet, bevor der Dienst
//!   antwortet – so lässt sich ein Start anhalten (Aufgabe 12: Doppelstart).
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
    geheim: Option<String>,
    /// Mit `QUIVER_DEMO_GEHEIM_ECHO=1` steht der Geheimwert in jeder Antwort.
    echo_geheim: bool,
    /// Mit `QUIVER_DEMO_FARBE=1` tragen die stderr-Zeilen ANSI-Farbcodes.
    farbe: bool,
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
        let flagge = |name: &str| std::env::var(name).is_ok_and(|wert| wert.trim() == "1");
        Self {
            name,
            werkzeuge,
            geheim: std::env::var("QUIVER_DEMO_GEHEIM")
                .ok()
                .filter(|wert| !wert.is_empty()),
            echo_geheim: flagge("QUIVER_DEMO_GEHEIM_ECHO"),
            farbe: flagge("QUIVER_DEMO_FARBE"),
        }
    }

    fn protokoll(&self, zeile: &str) {
        let zeile = format!("[{}] {zeile}", self.name);
        // ANSI-Farbcodes nur, wenn der Test sie verlangt — das Protokoll des
        // Clients muss sie entfernen (Aufgabe 17).
        let zeile = if self.farbe {
            format!("\u{1b}[32m{zeile}\u{1b}[0m")
        } else {
            zeile
        };
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
        let mut inhalt = format!(
            "[{}] {werkzeug} wurde aufgerufen und protokolliert: {argumente}",
            self.name
        );
        // Für die Prüfung der Maske (Aufgabe 17): der Geheimwert aus der
        // Umgebung steht in der Antwort und damit in der Vorschau des Clients.
        if self.echo_geheim {
            if let Some(geheim) = &self.geheim {
                inhalt.push_str(&format!("\nGeheimnis: {geheim}"));
            }
        }
        Ok(CallToolResponse::Complete(CallToolResult::success(vec![
            ContentBlock::text(inhalt),
        ])))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let demo = Demo::aus_umgebung();
    let geheim = match &demo.geheim {
        Some(wert) => format!("Geheimnis gesetzt ({} Zeichen)", wert.chars().count()),
        None => "kein Geheimnis".into(),
    };
    warte_auf_freigabe(&demo);
    let namen: Vec<_> = demo.werkzeuge.iter().map(|(n, _)| n.as_str()).collect();
    demo.protokoll(&format!(
        "gestartet, Werkzeuge: {}; {geheim}",
        namen.join(", ")
    ));
    let dienst = demo.serve(rmcp::transport::stdio_pipes()).await?;
    dienst.waiting().await?;
    Ok(())
}

/// Testhilfe: erst auf die Freigabedatei warten, dann den Dienst anbieten.
///
/// Nur wenn `QUIVER_DEMO_WARTE_AUF` beim Start **existiert**, wird gewartet –
/// so kann ein Test einen Start festhalten, einen zweiten daneben setzen und
/// den ersten danach freigeben.
fn warte_auf_freigabe(demo: &Demo) {
    let (Ok(arm), Ok(frei)) = (
        std::env::var("QUIVER_DEMO_WARTE_AUF"),
        std::env::var("QUIVER_DEMO_FREIGABE"),
    ) else {
        return;
    };
    if arm.trim().is_empty() || !std::path::Path::new(&arm).exists() {
        return;
    }
    demo.protokoll(&format!("warte auf Freigabe: {frei}"));
    while !std::path::Path::new(&frei).exists() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    demo.protokoll("freigegeben");
}
