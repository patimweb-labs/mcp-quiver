// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das eingecheckte CSS der Admin-Oberfläche ist genau das, was der Bausatz aus
//! den Token-Dateien erzeugt — sonst tragen egui und Vue verschiedene Werte
//! (AGENTS.md §2.11).

const EINGECHECKT: &str = include_str!("../../../admin/src/styles/tokens.css");

/// Dieselben Tokens für die Seiten des Servers (Aufgabe 26, Punkt 5).
const SEITEN_EINGECHECKT: &str = include_str!("../../server/seiten/system.css");

#[test]
fn tokens_css_ist_aktuell() {
    let erzeugt = quiver_oberflaeche::tokens_lesen::als_css();
    assert!(
        erzeugt == EINGECHECKT,
        "admin/src/styles/tokens.css ist veraltet — neu erzeugen mit:\n\
         cargo run -p quiver-oberflaeche --bin tokens_css > admin/src/styles/tokens.css"
    );
}

#[test]
fn seiten_css_ist_aktuell() {
    let erzeugt = quiver_oberflaeche::tokens_lesen::als_seiten_css();
    assert!(
        erzeugt == SEITEN_EINGECHECKT,
        "crates/server/seiten/system.css ist veraltet — neu erzeugen mit:\n\
         cargo run -p quiver-oberflaeche --bin tokens_seite_css > crates/server/seiten/system.css"
    );
}
