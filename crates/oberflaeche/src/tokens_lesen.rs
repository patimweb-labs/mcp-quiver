// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

// Tokens lesen: eine Wahrheit für `build.rs` (per `include!`), für das
// Programm `tokens_css` und für die Tests (AGENTS.md §2.6 und §2.11).
//
// Diese Datei wird auch von `build.rs` eingebunden; deshalb keine inneren
// Attribute (`//!`) und keine Annahmen über den Modulpfad.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Die Token-Dateien im Wurzelverzeichnis des Repos.
pub const VERZEICHNIS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tokens");

/// Die Grundwerte: Palette, Abstände, Größen, Schrift, Dauer.
pub const GRUNDWERTE: &str = "primitives.tokens.json";

/// Die drei Themes: Datei und Name in `data-theme` (AGENTS.md §2.7).
pub const THEMEN: [(&str, &str); 3] = [
    ("theme-dark.tokens.json", "dark"),
    ("theme-light.tokens.json", "light"),
    ("theme-mono.tokens.json", "mono"),
];

/// Vollständiger Pfad einer Token-Datei.
pub fn pfad(datei: &str) -> PathBuf {
    Path::new(VERZEICHNIS).join(datei)
}

fn flach(wert: &Value, pfad: &str, aus: &mut BTreeMap<String, String>) {
    if let Some(objekt) = wert.as_object() {
        if let Some(v) = objekt.get("$value") {
            aus.insert(
                pfad.to_string(),
                v.as_str().map(str::to_string).unwrap_or(v.to_string()),
            );
            return;
        }
        for (k, v) in objekt {
            if k.starts_with('$') {
                continue;
            }
            let neu = if pfad.is_empty() {
                k.clone()
            } else {
                format!("{pfad}.{k}")
            };
            flach(v, &neu, aus);
        }
    }
}

/// Eine Token-Datei als flache Zuordnung `pfad → Wert`.
pub fn lesen(datei: &str) -> BTreeMap<String, String> {
    let ort = pfad(datei);
    let text = std::fs::read_to_string(&ort).unwrap_or_else(|e| panic!("{}: {e}", ort.display()));
    let json: Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", ort.display()));
    let mut aus = BTreeMap::new();
    flach(&json, "", &mut aus);
    aus
}

/// `{color.neutral.900}` → Wert aus den Grundwerten (auch mehrfach verschachtelt).
pub fn aufloesen(wert: &str, grund: &BTreeMap<String, String>) -> String {
    match wert.strip_prefix('{').and_then(|w| w.strip_suffix('}')) {
        Some(verweis) => aufloesen(
            grund
                .get(verweis)
                .unwrap_or_else(|| panic!("Verweis {verweis} fehlt")),
            grund,
        ),
        None => wert.to_string(),
    }
}

/// Die Farbpfade eines Themes (`color.*`), alphabetisch.
pub fn farb_pfade(thema: &BTreeMap<String, String>) -> Vec<String> {
    thema
        .keys()
        .filter(|k| k.starts_with("color."))
        .cloned()
        .collect()
}

/// Die Maße der Grundwerte: alles außer den Farben (Abstände, Größen,
/// Schriftgrößen, Zeilenhöhen, Dauer, Familien, Gewichte) — mit ihrer Einheit
/// aus der Token-Datei.
pub fn mass_pfade(grund: &BTreeMap<String, String>) -> Vec<(String, String)> {
    grund
        .iter()
        .filter(|(pfad, _)| !pfad.starts_with("color."))
        .map(|(pfad, wert)| (pfad.clone(), wert.clone()))
        .collect()
}

/// `color.text.on-accent` → `--wp-color-text-on-accent`.
pub fn css_name(pfad: &str) -> String {
    format!("--wp-{}", pfad.replace('.', "-"))
}

/// Die Token-Dateien als CSS-Variablen für die Admin-Oberfläche.
pub fn als_css() -> String {
    let grund = lesen(GRUNDWERTE);
    let mut css = String::from(
        "/* Erzeugt von `cargo run -p quiver-oberflaeche --bin tokens_css` \
         – nicht von Hand ändern. */\n",
    );
    for (nr, (datei, name)) in THEMEN.iter().enumerate() {
        let thema = lesen(datei);
        if nr == 0 {
            css.push_str(&format!("\n:root, :root[data-theme=\"{name}\"] {{\n"));
        } else {
            css.push_str(&format!("\n:root[data-theme=\"{name}\"] {{\n"));
        }
        for pfad in farb_pfade(&thema) {
            let wert = aufloesen(thema.get(&pfad).expect("Farbwert im Theme"), &grund);
            css.push_str(&format!("  {}: {wert};\n", css_name(&pfad)));
        }
        css.push_str("}\n");
    }
    css.push_str("\n:root {\n");
    for (pfad, wert) in mass_pfade(&grund) {
        css.push_str(&format!("  {}: {wert};\n", css_name(&pfad)));
    }
    css.push_str("}\n");
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jede_datei_ist_lesbar() {
        let grund = lesen(GRUNDWERTE);
        assert!(grund.contains_key("space.4"));
        assert!(grund.contains_key("color.neutral.950"));
        for (datei, _) in THEMEN {
            let thema = lesen(datei);
            assert!(!farb_pfade(&thema).is_empty(), "{datei} ohne Farben");
        }
    }

    #[test]
    fn verweise_werden_aufgeloest() {
        let grund = lesen(GRUNDWERTE);
        let dunkel = lesen(THEMEN[0].0);
        let roh = dunkel.get("color.bg.canvas").expect("bg.canvas");
        let wert = aufloesen(roh, &grund);
        assert_eq!(wert, aufloesen(&grund["color.neutral.950"], &grund));
        assert!(wert.starts_with('#'), "keine Farbe: {wert}");
    }

    #[test]
    fn css_namen_und_einheiten() {
        assert_eq!(
            css_name("color.text.on-accent"),
            "--wp-color-text-on-accent"
        );
        assert_eq!(css_name("space.4"), "--wp-space-4");
        let grund = lesen(GRUNDWERTE);
        assert_eq!(grund["space.4"], "16px");
        assert_eq!(grund["duration.fast"], "120ms");
    }

    #[test]
    fn css_hat_alle_drei_themes() {
        let css = als_css();
        assert!(css.contains(":root, :root[data-theme=\"dark\"]"));
        assert!(css.contains(":root[data-theme=\"light\"]"));
        assert!(css.contains(":root[data-theme=\"mono\"]"));
        assert!(css.contains("--wp-space-4: 16px;"));
        assert!(css.contains("--wp-color-bg-canvas: #"));
    }
}
