// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver-server`: Unterbefehl oder Server. Liest die Umgebung, baut
//! [`quiver_server::app_mit_hintergrund_und_abschalter`], bindet und fährt bei
//! SIGTERM oder Strg+C sauber herunter.
//!
//! SIGTERM muss genauso wirken wie Strg+C: im Container ist der Server PID 1,
//! und dort wird SIGTERM (das `docker stop` schickt) nur beachtet, wenn der
//! Prozess es selbst behandelt. Sonst endet `docker stop` hart mit Exit 137.
//!
//! Reihenfolge beim Ende: Ereignis-Ströme sofort beenden, offene Anfragen
//! höchstens fünf Sekunden auslaufen lassen, WAL zurückschreiben, Ende mit 0.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use quiver_server::{
    app_mit_hintergrund_und_abschalter, benutzer, betrieb, rechte, unterbefehl, Konfig,
};
use tracing_subscriber::EnvFilter;

/// So lange dürfen offene Anfragen beim Herunterfahren noch laufen.
pub const ENDE_ZEIT: std::time::Duration = std::time::Duration::from_secs(5);

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

    let konfig = Konfig::aus_umgebung_mit_adresse(daten, adresse)?.mit_admin_dist(Some(admin_dist));
    tracing::info!("Basis-URL: {}", konfig.oeffentlich);
    tracing::info!("lauscht auf {adresse}");
    if let Some(entra) = &konfig.entra {
        tracing::info!(
            "Entra-Mandant {} (Authority {})",
            entra.tenant,
            entra.authority
        );
    } else {
        tracing::warn!("kein Entra konfiguriert: Geräte nutzen Einmal-Codes");
    }

    let (router, abschalter) = app_mit_hintergrund_und_abschalter(konfig).await?;

    let listener = tokio::net::TcpListener::bind(adresse)
        .await
        .with_context(|| format!("kann nicht auf {adresse} lauschen"))?;
    tracing::info!("quiver-server lauscht auf http://{adresse}");
    println!("quiver-server lauscht auf http://{adresse}");

    let ende = async {
        abschalter_endesignal().await;
        abschalter.ende_jetzt();
    };
    betrieb::bedienen(listener, router, ende).await?;
    abschalter.abschluss().await;
    tracing::info!("beendet");
    Ok(())
}

/// Auf Strg+C oder SIGTERM warten.
async fn abschalter_endesignal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut beenden = match signal(SignalKind::terminate()) {
            Ok(beenden) => beenden,
            Err(fehler) => {
                tracing::warn!("kein SIGTERM-Handler: {fehler}");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => tracing::info!("Strg+C: beende"),
            _ = beenden.recv() => tracing::info!("SIGTERM: beende"),
        }
    }
    #[cfg(not(unix))]
    {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("Strg+C: beende");
        } else {
            tracing::warn!("kein Signal-Handler, beende");
        }
    }
}
