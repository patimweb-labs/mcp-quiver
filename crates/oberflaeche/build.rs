// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Erzeugt `tokens.rs` aus den Token-Dateien in `tokens/` (AGENTS.md §2.6).
//!
//! Die Werte stehen in `tokens/*.tokens.json`; im Code stehen danach nur Namen
//! wie `f.text_default` — nie ein Farbwert. Gelesen wird mit demselben Code wie
//! im Programm (`src/tokens_lesen.rs`), damit es keine zweite Wahrheit gibt.

#[allow(dead_code)]
mod tokens_lesen {
    include!("src/tokens_lesen.rs");
}

use std::fmt::Write;
use tokens_lesen::{aufloesen, farb_pfade, lesen, pfad, GRUNDWERTE, THEMEN};

/// Namen der Konstanten je Theme, in der Reihenfolge von `THEMEN`.
const KONSTANTEN: [&str; 3] = ["DUNKEL", "HELL", "MONO"];

/// Gruppen der Grundwerte, die als `f32` ins Programm wandern.
const MASSE: [&str; 5] = ["space", "size", "duration", "radius", "breakpoint"];

fn ist_mass(pfad: &str) -> bool {
    let gruppe = pfad.split('.').next().unwrap_or("");
    MASSE.contains(&gruppe) || pfad.starts_with("font.size.") || pfad.starts_with("font.line.")
}

fn farbe(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    assert_eq!(h.len(), 6, "Farbe als #RRGGBB: {hex}");
    let r = u8::from_str_radix(&h[0..2], 16).unwrap();
    let g = u8::from_str_radix(&h[2..4], 16).unwrap();
    let b = u8::from_str_radix(&h[4..6], 16).unwrap();
    format!("Color32::from_rgb({r}, {g}, {b})")
}

fn zahl(wert: &str) -> String {
    let w = wert.trim_end_matches("px").trim_end_matches("ms");
    let f: f32 = w.parse().unwrap_or_else(|_| panic!("Zahl: {wert}"));
    format!("{f:?}")
}

fn bezeichner(pfad: &str) -> String {
    pfad.replace(['.', '-'], "_")
}

fn main() {
    for datei in std::iter::once(GRUNDWERTE).chain(THEMEN.iter().map(|(datei, _)| *datei)) {
        println!("cargo:rerun-if-changed={}", pfad(datei).display());
    }
    let grund = lesen(GRUNDWERTE);
    let namen = farb_pfade(&lesen(THEMEN[0].0));

    let mut rs = String::from(
        "// Erzeugt von build.rs aus tokens/*.tokens.json — nicht von Hand ändern.\n\
         use egui::Color32;\n\n\
         /// Die semantischen Farben eines Themes.\n\
         #[derive(Debug, Clone, Copy, PartialEq)]\npub struct Farben {\n",
    );
    for n in &namen {
        writeln!(
            rs,
            "    pub {}: Color32,",
            bezeichner(n.trim_start_matches("color."))
        )
        .unwrap();
    }
    rs.push_str("}\n\n");
    for ((datei, _), konst) in THEMEN.iter().zip(KONSTANTEN) {
        let thema = lesen(datei);
        writeln!(rs, "pub const {konst}: Farben = Farben {{").unwrap();
        for n in &namen {
            let roh = thema.get(n).unwrap_or_else(|| panic!("{datei}: {n} fehlt"));
            writeln!(
                rs,
                "    {}: {},",
                bezeichner(n.trim_start_matches("color.")),
                farbe(&aufloesen(roh, &grund))
            )
            .unwrap();
        }
        rs.push_str("};\n\n");
    }
    // Maße: Abstände, Größen, Schriftgrößen, Dauer — als f32-Konstanten.
    for (pfad, wert) in &grund {
        if ist_mass(pfad) {
            writeln!(
                rs,
                "pub const {}: f32 = {};",
                bezeichner(pfad).to_uppercase(),
                zahl(wert)
            )
            .unwrap();
        }
    }
    let ziel = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("tokens.rs");
    std::fs::write(ziel, rs).unwrap();
}
