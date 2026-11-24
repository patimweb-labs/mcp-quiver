// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-server`: Unterbefehl oder Server. Liest die Umgebung, baut
//! [`quiver_server::app_mit_hintergrund`], bindet und fährt bei Strg+C sauber
//! herunter.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use quiver_server::{app_mit_hintergrund, benutzer, betrieb, rechte, unterbefehl, Konfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Vor allem anderen: neue Dateien gehören von der ersten Sekunde an nur
    // dem Eigentümer (Datenbank, `-wal`, `-shm`, Schlüssel, Sicherungen).
    rechte::umask_setzen();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if unterbefehl::ist_unterbefehl(&args) {
        if unterbefehl::ist_gesund(&args) {
            return unterbefehl::gesund()
                .await
                .inspect_err(|fehler| eprintln!("{fehler:#}"));
        }
        return unterbefehl::ausfuehren(&args).inspect_err(|fehler| eprintln!("{fehler:#}"));
    }
    if !args.is_empty() {
        anyhow::bail!(
            "unbekannter Aufruf {:?}; bekannt sind: {}",
            args,
            unterbefehl::UNTERBEFEHLE.join(", ")
        );
    }

    let daten =
        PathBuf::from(std::env::var("QUIVER_DATEN").unwrap_or_else(|_| String::from("./daten")));
    let adresse: SocketAddr = std::env::var("QUIVER_ADRESSE")
        .unwrap_or_else(|_| String::from("0.0.0.0:8740"))
        .parse()
        .context("QUIVER_ADRESSE ist keine Adresse (Beispiel: 0.0.0.0:8740)")?;
    let admin_dist = std::env::var("QUIVER_ADMIN_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./admin/dist"));

    // Der alte Admin-Schlüssel gilt nicht mehr; einmal warnen, wenn er noch
    // in der Umgebung steht.
    benutzer::admin_token_ignorieren();

    let konfig =
        Konfig::aus_umgebung_mit_port(daten, adresse.port())?.mit_admin_dist(Some(admin_dist));
    tracing::info!("Basis-URL: {}", konfig.oeffentlich);
    if let Some(entra) = &konfig.entra {
        tracing::info!(
            "Entra-Mandant {} (Authority {})",
            entra.tenant,
            entra.authority
        );
    } else {
        tracing::warn!("kein Entra konfiguriert: Geräte nutzen Einmal-Codes");
    }

    let router = app_mit_hintergrund(konfig).await?;

    let listener = tokio::net::TcpListener::bind(adresse)
        .await
        .with_context(|| format!("kann nicht auf {adresse} lauschen"))?;
    tracing::info!("quiver-server lauscht auf http://{adresse}");
    println!("quiver-server lauscht auf http://{adresse}");

    betrieb::bedienen(listener, router, herunterfahren()).await
}

async fn herunterfahren() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("Strg+C: beende");
    } else {
        tracing::warn!("kein Signal-Handler, beende");
    }
}
