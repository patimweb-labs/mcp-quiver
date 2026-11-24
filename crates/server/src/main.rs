// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-server`: liest die Umgebung, baut [`quiver_server::app`], bindet und
//! fährt bei Strg+C sauber herunter.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use quiver_server::{admin_token_ermitteln, app, Konfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let daten =
        PathBuf::from(std::env::var("QUIVER_DATEN").unwrap_or_else(|_| String::from("./daten")));
    let adresse: SocketAddr = std::env::var("QUIVER_ADRESSE")
        .unwrap_or_else(|_| String::from("0.0.0.0:8740"))
        .parse()
        .context("QUIVER_ADRESSE ist keine Adresse (Beispiel: 0.0.0.0:8740)")?;
    let admin_dist = Some(
        std::env::var("QUIVER_ADMIN_DIST")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./admin/dist")),
    );

    let (admin_token, neu) = admin_token_ermitteln(&daten)?;
    if neu {
        // Das ist die einzige Stelle, an der ein Token im Log landet.
        tracing::warn!("Admin-Token erzeugt: {admin_token}");
    }

    let router = app(Konfig {
        daten,
        admin_token,
        admin_dist,
    })
    .await?;

    let listener = tokio::net::TcpListener::bind(adresse)
        .await
        .with_context(|| format!("kann nicht auf {adresse} lauschen"))?;
    tracing::info!("quiver-server lauscht auf http://{adresse}");
    println!("quiver-server lauscht auf http://{adresse}");

    axum::serve(listener, router)
        .with_graceful_shutdown(herunterfahren())
        .await?;
    Ok(())
}

async fn herunterfahren() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("Strg+C: beende");
    } else {
        tracing::warn!("kein Signal-Handler, beende");
    }
}
