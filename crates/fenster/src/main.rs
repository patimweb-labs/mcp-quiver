// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver` — das Client-Fenster. Mit `--headless` läuft nur der Kern, ohne
//! Fenster, bis Strg+C (für Server-Rechner und Tests).

// Unter Windows kein zusätzliches Konsolenfenster im Release-Bau.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use quiver_client::{Kern, KernKonfig};
use quiver_fenster::app::{Fenster, KernZugang};
use quiver_fenster::kern::EchterKern;
use quiver_fenster::modell::Sicht;
use quiver_fenster::profil::Einstellungen;

fn main() -> anyhow::Result<()> {
    einrichten_log();
    let konfig = KernKonfig::standard();
    if std::env::args().any(|arg| arg == "--headless") {
        return kopflos(konfig);
    }
    fenster(konfig)
}

/// Protokoll nach stderr, Stufe über `RUST_LOG`.
fn einrichten_log() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Nur der Kern, ohne Fenster: Strg+C beendet ihn.
fn kopflos(konfig: KernKonfig) -> anyhow::Result<()> {
    let laufzeit = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let kern = laufzeit.block_on(Kern::starten(konfig))?;
    tracing::info!("quiver läuft ohne Fenster — Strg+C beendet.");
    laufzeit.block_on(async {
        let _ = tokio::signal::ctrl_c().await;
    });
    laufzeit.block_on(kern.beenden());
    Ok(())
}

/// Das Fenster.
fn fenster(konfig: KernKonfig) -> anyhow::Result<()> {
    let einstellungen = Einstellungen::laden(&konfig.profil);
    let profil = konfig.profil.clone();
    let kern = EchterKern::starten(konfig)?;
    let sicht = Sicht::neu(kern.modell(), &einstellungen);

    let optionen = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mcp-quiver")
            .with_app_id("de.weppelmann.mcp-quiver")
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([560.0, 460.0]),
        ..Default::default()
    };
    eframe::run_native(
        "mcp-quiver",
        optionen,
        Box::new(move |cc| {
            quiver_oberflaeche::einrichten(&cc.egui_ctx);
            kern.wecken(cc.egui_ctx.clone());
            Ok(Box::new(Fenster::neu(kern, sicht, Some(profil))))
        }),
    )
    .map_err(|fehler| anyhow::anyhow!("Fenster nicht startbar: {fehler}"))
}
