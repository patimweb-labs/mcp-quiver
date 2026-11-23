// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ein winziger MCP-Server über stdio, mit genau einem Werkzeug: `echo`.
//!
//! Er dient dem Ende-zu-Ende-Test (`tests/ende.rs`) als Eintrag, den der
//! Client startet, und lässt sich auch von Hand ausprobieren:
//!
//! ```text
//! QUIVER_ECHO_PRAEFIX=kind- cargo run -p quiver-client --bin quiver-echo-mcp
//! ```
//!
//! `QUIVER_ECHO_PRAEFIX` wird der Antwort vorangestellt – damit kann ein Test
//! sehen, dass wirklich der Client die Umgebung des Eintrags gesetzt hat.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::{schemars, tool, tool_handler, tool_router};
use rmcp::{ServerHandler, ServiceExt};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EchoAnfrage {
    /// Der Text, der zurückkommt.
    text: String,
}

#[derive(Debug, Clone)]
struct Echo {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Echo {
    fn neu() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Gibt den übergebenen Text unverändert zurück")]
    fn echo(&self, Parameters(anfrage): Parameters<EchoAnfrage>) -> String {
        let praefix = std::env::var("QUIVER_ECHO_PRAEFIX").unwrap_or_default();
        format!("{praefix}{}", anfrage.text)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Echo {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dienst = Echo::neu().serve(rmcp::transport::stdio_pipes()).await?;
    dienst.waiting().await?;
    Ok(())
}
