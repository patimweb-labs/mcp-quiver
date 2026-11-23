// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `quiver` — das Client-Fenster.
//!
//! * ohne Argument: Fenster, Kern und Tray
//! * `--hintergrund`: nur Tray und Kern; das Fenster öffnet sich auf Wunsch
//! * `--headless`: nur der Kern, ohne Fenster und ohne Tray, bis Strg+C
//!
//! Der MCP-Endpunkt und der Abgleich laufen in allen drei Fällen weiter, auch
//! wenn kein Fenster zu sehen ist.

// Unter Windows kein zusätzliches Konsolenfenster im Release-Bau.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use quiver_client::{Kern, KernKonfig};
use quiver_fenster::app::{Fenster, KernZugang};
use quiver_fenster::kern::EchterKern;
use quiver_fenster::modell::Sicht;
use quiver_fenster::profil::Einstellungen;
use quiver_fenster::tray::{self, Tray, TrayZustand};
use quiver_fenster::{autostart, instanz, logik};

fn main() -> anyhow::Result<()> {
    einrichten_log();
    let argumente = logik::argumente(std::env::args().skip(1));
    let konfig = kernkonfig(KernKonfig::standard(), |name| std::env::var(name).ok());
    if argumente.kopflos {
        return kopflos(konfig);
    }
    // Nur eine Instanz je Gerät: Läuft schon eine, zeigt sie ihr Fenster und
    // dieser Start endet. Der Kern bliebe sonst am belegten Port hängen.
    let fensteradresse = instanz::adresse(konfig.mcp_adresse);
    if instanz::zeigen_anfordern(fensteradresse).is_ok() {
        tracing::info!("quiver läuft schon — Fenster der laufenden Instanz geöffnet.");
        return Ok(());
    }
    fenster(konfig, argumente.hintergrund)
}

/// Protokoll nach stderr, Stufe über `RUST_LOG`.
fn einrichten_log() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Die Konfiguration aus der Umgebung: `QUIVER_PROFIL` verlegt den
/// Profilordner, `QUIVER_MCP_ADRESSE` den MCP-Endpunkt (anderer Port, damit
/// zwei Instanzen — etwa eine Demo und eine zum Ausprobieren — nebeneinander
/// arbeiten können) und `QUIVER_TRESOR=speicher` nimmt einen Tresor im
/// Arbeitsspeicher statt der Tresor-Datei (zum Ausprobieren ohne Spuren).
fn kernkonfig(mut konfig: KernKonfig, lesen: impl Fn(&str) -> Option<String>) -> KernKonfig {
    if let Some(pfad) = lesen("QUIVER_PROFIL").filter(|pfad| !pfad.trim().is_empty()) {
        konfig.profil = PathBuf::from(pfad.trim());
        // Ein eigenes Profil hat seinen eigenen Tresor – ohne Umzug.
        konfig.tresor = std::sync::Arc::new(quiver_client::DateiTresor::fuer_profil(
            &konfig.profil,
            false,
        ));
    }
    if let Some(wert) = lesen("QUIVER_MCP_ADRESSE") {
        match wert.trim().parse::<SocketAddr>() {
            Ok(adresse) => konfig.mcp_adresse = adresse,
            Err(fehler) => tracing::warn!(
                "QUIVER_MCP_ADRESSE ist keine Adresse ({fehler}); bleibe bei {}",
                konfig.mcp_adresse
            ),
        }
    }
    if lesen("QUIVER_TRESOR").is_some_and(|wert| wert.trim() == "speicher") {
        konfig.tresor = std::sync::Arc::new(quiver_client::SpeicherTresor::neu());
    }
    konfig
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

/// Fenster, Kern und Tray. Das Fenster ist auch mit `--hintergrund` da, nur
/// weggelegt: `tray-icon` braucht unter macOS und Windows eine laufende
/// Ereignisschleife, und „Fenster öffnen" soll sie nicht erst starten müssen.
fn fenster(konfig: KernKonfig, hintergrund: bool) -> anyhow::Result<()> {
    let einstellungen = Einstellungen::laden(&konfig.profil);
    let profil = konfig.profil.clone();

    // Ein Kanal für alles, was von außen kommt: Tray-Klicks und die Bitte
    // einer zweiten Instanz, das Fenster zu zeigen.
    let (befehl_sender, befehl_empfaenger) = mpsc::channel();
    let zustand = TrayZustand::neu(befehl_sender);
    let waechter =
        match instanz::Waechter::belegen(instanz::adresse(konfig.mcp_adresse), zustand.clone()) {
            Ok(waechter) => Some(waechter),
            Err(fehler) => {
                // Der Port ist besetzt, antwortet aber nicht — dann läuft keine
                // Instanz, die das Fenster zeigen könnte; quiver startet trotzdem.
                tracing::warn!("kein Wächter für die zweite Instanz: {fehler}");
                None
            }
        };
    // Der Wächter lebt bis zum Ende von `fenster()`; sein Faden horcht im
    // Hintergrund weiter auf „Fenster zeigen".
    if let Some(waechter) = &waechter {
        tracing::debug!("zweite Instanz meldet sich auf {}", waechter.adresse());
    }

    let kern = EchterKern::starten(konfig)?;
    let mut sicht = Sicht::neu(kern.modell(), &einstellungen);
    sicht.autostart = autostart::aktiv();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("mcp-quiver")
        .with_app_id("de.weppelmann.mcp-quiver")
        .with_inner_size([1000.0, 720.0])
        .with_min_inner_size([560.0, 460.0]);
    if hintergrund {
        viewport = viewport.with_visible(false).with_active(false);
    }
    let optionen = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "mcp-quiver",
        optionen,
        Box::new(move |cc| {
            quiver_oberflaeche::einrichten(&cc.egui_ctx);
            kern.wecken(cc.egui_ctx.clone());
            zustand.wecken_mit(cc.egui_ctx.clone());
            // Das Menü steht schon beim ersten Aufbau — sonst wäre es einen
            // Augenblick lang leer.
            zustand.setzen(logik::tray_menu(
                &sicht.modell,
                sicht.autostart,
                sicht.sprache,
            ));
            let tray = Tray::starten(zustand.clone());
            if tray.is_none() {
                // Ohne Tray gäbe es keinen Weg zum Fenster — dann gleich zeigen,
                // auch wenn `--hintergrund` gewünscht war.
                tracing::warn!("kein Tray — das Fenster ist der einzige Weg zu quiver.");
                tray::zeigen(&cc.egui_ctx);
            }
            let fenster = Fenster::neu(kern, sicht, Some(profil))
                .mit_autostart(Box::new(autostart::setzen))
                .mit_tray(tray, befehl_empfaenger);
            Ok(Box::new(fenster))
        }),
    )
    .map_err(|fehler| anyhow::anyhow!("Fenster nicht startbar: {fehler}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aus<'a>(werte: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            werte
                .iter()
                .find(|(schluessel, _)| *schluessel == name)
                .map(|(_, wert)| wert.to_string())
        }
    }

    #[test]
    fn konfiguration_kommt_aus_der_umgebung() {
        let standard = KernKonfig::standard();
        // Ohne Variablen bleibt es beim Standard.
        let gleich = kernkonfig(standard.clone(), aus(&[]));
        assert_eq!(gleich.mcp_adresse, standard.mcp_adresse);
        assert_eq!(gleich.profil, standard.profil);

        // Eigener Port und eigenes Profil: so laufen Demo und Versuch nebeneinander.
        let eigen = kernkonfig(
            standard.clone(),
            aus(&[
                ("QUIVER_MCP_ADRESSE", "127.0.0.1:18741"),
                ("QUIVER_PROFIL", " /tmp/quiver-test "),
            ]),
        );
        assert_eq!(eigen.mcp_adresse.port(), 18741);
        assert_eq!(eigen.profil, PathBuf::from("/tmp/quiver-test"));

        // Unsinn in der Adresse ändert nichts, statt den Start zu verhindern.
        let unsinn = kernkonfig(
            standard.clone(),
            aus(&[("QUIVER_MCP_ADRESSE", "kein-port")]),
        );
        assert_eq!(unsinn.mcp_adresse, standard.mcp_adresse);

        // Leeres Profil zählt nicht als Angabe.
        let leer = kernkonfig(standard.clone(), aus(&[("QUIVER_PROFIL", "  ")]));
        assert_eq!(leer.profil, standard.profil);

        // Tresor im Speicher: keine Tresor-Datei, keine Nachfrage des Systems.
        let speicher = kernkonfig(standard.clone(), aus(&[("QUIVER_TRESOR", "speicher")]));
        speicher
            .tresor
            .schreiben("probe", "wert")
            .expect("beschreibbar");
        assert_eq!(
            speicher.tresor.lesen("probe").expect("lesbar").as_deref(),
            Some("wert")
        );
    }
}
