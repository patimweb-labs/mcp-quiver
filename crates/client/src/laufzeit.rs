// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wie ein Eintrag tatsächlich läuft: als Kindprozess mit MCP über
//! stdin/stdout oder als entfernter MCP-Server über Streamable HTTP.
//!
//! Beides endet in derselben Form: ein [`Peer`], über den der Client
//! `tools/list` und `tools/call` spricht. Der Endpunkt für die Agenten reicht
//! die Aufrufe nur weiter – Konfiguration und Zugangsdaten bleiben hier.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use quiver_domain::EintragId;
use quiver_domain::Laufzeit;
use rmcp::model::Tool;
use rmcp::service::{ClientInitializeError, RunningService, RunningServiceCancellationToken};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{Peer, RoleClient, ServiceExt};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::protokoll::Protokoll;

/// Eine laufende Verbindung zu genau einem Eintrag.
pub struct EintragVerbindung {
    peer: Peer<RoleClient>,
    /// Wird wahr, wenn der Dienst von selbst endet (Prozess tot, Leitung weg).
    ende: tokio::sync::oneshot::Receiver<()>,
    abbruch: Mutex<Option<RunningServiceCancellationToken>>,
}

impl EintragVerbindung {
    /// Die Werkzeuge, die der Eintrag gerade anbietet.
    pub async fn werkzeuge(&self) -> anyhow::Result<Vec<Tool>> {
        self.peer
            .list_all_tools()
            .await
            .context("tools/list beim Eintrag")
    }

    /// Der Peer für den Endpunkt (Aufrufe der Agenten).
    pub fn peer(&self) -> &Peer<RoleClient> {
        &self.peer
    }

    /// Warten, bis der Dienst endet (Prozess tot oder Leitung zu).
    pub async fn warten(&mut self) {
        let _ = (&mut self.ende).await;
    }

    /// Die Verbindung schließen – bei stdio beendet das den Kindprozess.
    pub fn beenden(&self) {
        let abbruch = self
            .abbruch
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .take();
        if let Some(abbruch) = abbruch {
            abbruch.cancel();
        }
    }
}

/// Eine Laufzeit starten und die MCP-Verbindung aufbauen. Was der Prozess auf
/// stderr schreibt, landet im Protokoll (Aufgabe 17).
pub async fn verbinden(
    name: &str,
    laufzeit: &Laufzeit,
    protokoll: &Arc<Protokoll>,
) -> anyhow::Result<EintragVerbindung> {
    match laufzeit {
        Laufzeit::Stdio {
            befehl,
            argumente,
            umgebung,
        } => stdio(name, befehl, argumente, umgebung, protokoll).await,
        Laufzeit::Remote { url, kopfzeilen } => remote(url, kopfzeilen).await,
    }
}

async fn stdio(
    name: &str,
    befehl: &str,
    argumente: &[String],
    umgebung: &quiver_domain::Werte,
    protokoll: &Arc<Protokoll>,
) -> anyhow::Result<EintragVerbindung> {
    let kommando = tokio::process::Command::new(befehl).configure(|cmd| {
        cmd.args(argumente);
        for (schluessel, wert) in umgebung {
            cmd.env(schluessel, wert);
        }
    });

    let (transport, stderr) = TokioChildProcess::builder(kommando)
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("{befehl} lässt sich nicht starten"))?;
    if let Some(stderr) = stderr {
        stderr_ins_log(name.to_string(), stderr, protokoll.clone());
    }
    aufbauen(().serve(transport)).await
}

async fn remote(url: &str, kopfzeilen: &quiver_domain::Werte) -> anyhow::Result<EintragVerbindung> {
    let mut kopf = HashMap::new();
    for (name, wert) in kopfzeilen {
        let feld = axum::http::HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("Kopfzeile {name} ist ungültig"))?;
        let inhalt = axum::http::HeaderValue::from_str(wert)
            .with_context(|| format!("Wert der Kopfzeile {name} ist ungültig"))?;
        kopf.insert(feld, inhalt);
    }
    let config =
        StreamableHttpClientTransportConfig::with_uri(url.to_string()).custom_headers(kopf);
    let transport = StreamableHttpClientTransport::from_config(config);
    aufbauen(().serve(transport)).await
}

/// Der gemeinsame Teil: Verbindung aufbauen und die Aufsicht starten.
async fn aufbauen(
    aufbau: impl std::future::Future<
        Output = Result<RunningService<RoleClient, ()>, ClientInitializeError>,
    >,
) -> anyhow::Result<EintragVerbindung> {
    let lauf = aufbau.await.context("MCP-EintragVerbindung zum Eintrag")?;
    let peer = lauf.peer().clone();
    let abbruch = lauf.cancellation_token();
    let (sender, ende) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = lauf.waiting().await;
        let _ = sender.send(());
    });
    Ok(EintragVerbindung {
        peer,
        ende,
        abbruch: Mutex::new(Some(abbruch)),
    })
}

/// Was der Kindprozess auf stderr schreibt, gehört ins Protokoll (Aufgabe 17) –
/// je Eintrag, ohne ANSI-Farbcodes und maskiert. Zusätzlich steht es im Log.
fn stderr_ins_log(name: String, stderr: tokio::process::ChildStderr, protokoll: Arc<Protokoll>) {
    let eintrag = EintragId::neu(name.clone());
    tokio::spawn(async move {
        let mut zeilen = BufReader::new(stderr).lines();
        while let Ok(Some(zeile)) = zeilen.next_line().await {
            tracing::info!(eintrag = %name, "{zeile}");
            protokoll.zeile_schreiben(&eintrag, &zeile);
        }
    });
}

/// Wie lange ein Aufbau höchstens dauern darf, bevor der Eintrag als fehlerhaft gilt.
pub const AUFBAU_TIMEOUT: Duration = Duration::from_secs(20);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unbekannter_befehl_ist_ein_fehler() {
        let laufzeit = Laufzeit::Stdio {
            befehl: "gibt-es-garantiert-nicht-quiver".into(),
            argumente: Vec::new(),
            umgebung: quiver_domain::Werte::new(),
        };
        let ordner = std::env::temp_dir().join(format!(
            "quiver-laufzeit-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        let protokoll = Arc::new(Protokoll::neu(&ordner));
        let ergebnis = verbinden("test", &laufzeit, &protokoll).await;
        assert!(ergebnis.is_err(), "Start muss scheitern");
        std::fs::remove_dir_all(&ordner).ok();
    }
}
