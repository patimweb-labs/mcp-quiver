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
//! - `QUIVER_DEMO_LANGSAM_MS`: optional; jeder Werkzeugaufruf wartet so lange
//!   (Paket 3: Frist je Aufruf).
//! - `QUIVER_DEMO_FORTSCHRITT`: optional; so viele Fortschrittsmeldungen
//!   schickt jeder Aufruf (Paket 3: sie verlängern die Frist).
//! - `QUIVER_DEMO_FORTSCHRITT_MS`: Abstand dazwischen (Standard 200 ms).
//! - `QUIVER_DEMO_GROSS`: optional; die Antwort wird so viele Zeichen lang
//!   (Paket 3: Größengrenzen).
//! - `QUIVER_DEMO_VIELE`: optional; so viele Werkzeuge bietet der Server an
//!   (Paket 3: höchstens 500).
//! - `QUIVER_DEMO_UMGEBUNG`: optional; der Name einer Umgebungsvariablen, die
//!   in die Antwort geschrieben wird (`gesetzt`/`leer`) – so prüft ein Test,
//!   dass ein Werkzeug nicht die ganze Umgebung erbt (Paket 3).
//! - `QUIVER_DEMO_PID`: optional, ein Ordner. Jeder Start legt dort eine Datei
//!   mit seiner PID ab und entfernt sie beim Beenden wieder – so zählt ein
//!   Test, wie viele Demo-Prozesse wirklich laufen (Aufgabe 19).
//!
//! Jede Zeile geht nach stderr (landet im Protokoll des Clients) und ggf. in die Datei.

use rmcp::handler::server::ServerHandler;
use rmcp::model::RequestParamsMeta;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ProgressNotificationParam, ServerCapabilities, ServerConfig, Tool,
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
    /// So lange wartet jeder Aufruf (Paket 3: Frist).
    langsam_ms: u64,
    /// So viele Fortschrittsmeldungen schickt jeder Aufruf.
    fortschritte: u64,
    /// Abstand der Fortschrittsmeldungen.
    fortschritt_ms: u64,
    /// So lang wird die Antwort (Paket 3: Größengrenzen).
    gross: u64,
    /// Der Name einer Umgebungsvariablen, die in die Antwort kommt.
    umgebung: Option<String>,
}

impl Demo {
    fn aus_umgebung() -> Self {
        let name = std::env::var("QUIVER_DEMO_NAME").unwrap_or_else(|_| "demo".into());
        let werkzeuge: Vec<(String, String)> = match std::env::var("QUIVER_DEMO_VIELE")
            .ok()
            .and_then(|wert| wert.trim().parse::<usize>().ok())
        {
            Some(anzahl) => (0..anzahl)
                .map(|nummer| (format!("werkzeug{nummer}"), format!("Werkzeug {nummer}")))
                .collect(),
            None => std::env::var("QUIVER_DEMO_TOOLS")
                .unwrap_or_else(|_| "ausfuehren=Führt etwas aus (Demo)".into())
                .split(';')
                .filter_map(|teil| {
                    let (n, b) = teil.split_once('=').unwrap_or((teil, "Demo-Werkzeug"));
                    let n = n.trim();
                    (!n.is_empty()).then(|| (n.to_string(), b.trim().to_string()))
                })
                .collect(),
        };
        let flagge = |name: &str| std::env::var(name).is_ok_and(|wert| wert.trim() == "1");
        let zahl = |name: &str| {
            std::env::var(name)
                .ok()
                .and_then(|wert| wert.trim().parse::<u64>().ok())
        };
        Self {
            name,
            werkzeuge,
            geheim: std::env::var("QUIVER_DEMO_GEHEIM")
                .ok()
                .filter(|wert| !wert.is_empty()),
            echo_geheim: flagge("QUIVER_DEMO_GEHEIM_ECHO"),
            farbe: flagge("QUIVER_DEMO_FARBE"),
            langsam_ms: zahl("QUIVER_DEMO_LANGSAM_MS").unwrap_or(0),
            fortschritte: zahl("QUIVER_DEMO_FORTSCHRITT").unwrap_or(0),
            fortschritt_ms: zahl("QUIVER_DEMO_FORTSCHRITT_MS").unwrap_or(200),
            gross: zahl("QUIVER_DEMO_GROSS").unwrap_or(0),
            umgebung: std::env::var("QUIVER_DEMO_UMGEBUNG")
                .ok()
                .filter(|wert| !wert.trim().is_empty()),
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
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let werkzeug = request.name.to_string();
        if !self.werkzeuge.iter().any(|(n, _)| *n == werkzeug) {
            return Err(McpError::invalid_params(
                format!("unbekannt: {werkzeug}"),
                None,
            ));
        }
        let fortschritts_token = request.progress_token();
        let argumente = serde_json::to_string(&request.arguments.unwrap_or_default())
            .unwrap_or_else(|_| "{}".into());
        self.protokoll(&format!("Aufruf {werkzeug} {argumente}"));
        // Fortschrittsmeldungen (Paket 3): sie verlängern die Frist des
        // Clients, ohne dass die Antwort früher kommt.
        if self.fortschritte > 0 {
            if let Some(token) = fortschritts_token {
                for schritt in 0..self.fortschritte {
                    tokio::time::sleep(std::time::Duration::from_millis(self.fortschritt_ms)).await;
                    let meldung = ProgressNotificationParam::new(token.clone(), schritt as f64)
                        .with_total(self.fortschritte as f64)
                        .with_message(format!("Schritt {}", schritt + 1));
                    let _ = context.peer.notify_progress(meldung).await;
                }
            }
        }
        if self.langsam_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.langsam_ms)).await;
        }
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
        if let Some(name) = &self.umgebung {
            let stand = if std::env::var_os(name).is_some() {
                "gesetzt"
            } else {
                "leer"
            };
            inhalt.push_str(&format!("\n{name}={stand}"));
        }
        if self.gross > 0 {
            let platzhalter = "x".repeat(usize::try_from(self.gross).unwrap_or(usize::MAX));
            inhalt.push_str(&format!("\n{platzhalter}"));
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
    let _pid = PidDatei::neu();
    let dienst = demo.serve(rmcp::transport::stdio_pipes()).await?;
    dienst.waiting().await?;
    Ok(())
}

/// Legt beim Start `<QUIVER_DEMO_PID>/<pid>` an und entfernt die Datei beim
/// Beenden. Ein Test zählt damit die wirklich laufenden Demo-Prozesse
/// (Aufgabe 19: „genau ein Prozess“).
struct PidDatei {
    pfad: Option<std::path::PathBuf>,
}

impl PidDatei {
    fn neu() -> Self {
        let pfad = std::env::var("QUIVER_DEMO_PID")
            .ok()
            .filter(|wert| !wert.trim().is_empty())
            .map(|ordner| std::path::PathBuf::from(ordner).join(std::process::id().to_string()));
        if let Some(pfad) = &pfad {
            if let Some(ordner) = pfad.parent() {
                let _ = std::fs::create_dir_all(ordner);
            }
            let _ = std::fs::write(pfad, b"");
        }
        Self { pfad }
    }
}

impl Drop for PidDatei {
    fn drop(&mut self) {
        if let Some(pfad) = &self.pfad {
            let _ = std::fs::remove_file(pfad);
        }
    }
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
