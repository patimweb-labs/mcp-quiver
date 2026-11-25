// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Lieferkette als Test: was in `Cargo.lock` steht, gehört geprüft.
//!
//! `cargo deny check` (Regeln in `deny.toml`) ist der Hauptweg; dieser Test
//! hält die zwei Festlegungen fest, die sich sonst still zurückdrehen ließen:
//! genau eine RSA-Fassung (die aktuelle) und ein SQLite, das nicht älter ist
//! als die behobene Fassung.

use std::collections::BTreeMap;
use std::path::Path;

/// Die kleinste erlaubte RSA-Fassung.
const RSA_MINDESTENS: (u64, u64) = (0, 10);
/// Die kleinste erlaubte rusqlite-Fassung (bringt SQLite 3.54.1 mit).
const RUSQLITE_MINDESTENS: (u64, u64) = (0, 43);

/// Fassungen je Kiste aus `Cargo.lock` (nur Name und Version, ohne Auswertung
/// des TOML – dafür ist die Datei zu gleichmäßig aufgebaut).
fn fassungen() -> BTreeMap<String, Vec<(u64, u64)>> {
    let pfad = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("Cargo.lock");
    let text = std::fs::read_to_string(&pfad).expect("Cargo.lock lesbar");
    let mut aus: BTreeMap<String, Vec<(u64, u64)>> = BTreeMap::new();
    let mut name: Option<String> = None;
    for zeile in text.lines() {
        if let Some(rest) = zeile.strip_prefix("name = \"") {
            name = Some(rest.trim_end_matches('"').to_string());
            continue;
        }
        if let Some(rest) = zeile.strip_prefix("version = \"") {
            let version = rest.trim_end_matches('"');
            let mut teile = version.split('.');
            let gross: u64 = teile.next().unwrap_or("0").parse().unwrap_or(0);
            let klein: u64 = teile.next().unwrap_or("0").parse().unwrap_or(0);
            if let Some(name) = name.take() {
                aus.entry(name).or_default().push((gross, klein));
            }
        }
    }
    aus
}

#[test]
fn genau_eine_rsa_fassung_und_die_aktuelle() {
    let alle = fassungen();
    let rsa = alle.get("rsa").expect("rsa steht im Sperrbaum");
    assert_eq!(
        rsa.len(),
        1,
        "nur eine RSA-Fassung, gefunden: {rsa:?} (0.9 gehört nicht mehr dazu)"
    );
    assert!(
        rsa[0] >= RSA_MINDESTENS,
        "rsa {} ist älter als {RSA_MINDESTENS:?}",
        rsa[0].0
    );
    // Die Testhilfe von damals ist weg; sie zog die alte Fassung nach.
    assert!(
        !alle.contains_key("rsa_alt"),
        "rsa_alt (0.9) gehört nicht mehr in den Baum"
    );
}

#[test]
fn rusqlite_ist_mindestens_die_behobene_fassung() {
    let alle = fassungen();
    let rusqlite = alle.get("rusqlite").expect("rusqlite steht im Sperrbaum");
    assert_eq!(rusqlite.len(), 1, "eine rusqlite-Fassung: {rusqlite:?}");
    assert!(
        rusqlite[0] >= RUSQLITE_MINDESTENS,
        "rusqlite {} ist älter als {RUSQLITE_MINDESTENS:?}",
        rusqlite[0].0
    );
    // `bundled` bringt SQLite selbst mit; die genaue Fassung prüft
    // `speicher::tests::sqlite_fassung_ist_mindestens_die_behobene`.
    let sys = alle
        .get("libsqlite3-sys")
        .expect("libsqlite3-sys steht im Sperrbaum");
    assert_eq!(sys.len(), 1, "eine libsqlite3-sys-Fassung: {sys:?}");
}
