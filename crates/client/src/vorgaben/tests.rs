// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests für die Vorgaben der Firma (Aufgabe 34): Lesen, Prüfen, Verwerfen.
//!
//! Der Ort ist immer eine Datei im temporären Ordner – nie `/etc`,
//! `/Library` oder die Registry des Rechners.

use std::path::PathBuf;

use super::*;

fn ordner(tag: &str) -> PathBuf {
    let stempel = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Systemzeit nach 1970")
        .as_nanos();
    let pfad = std::env::temp_dir().join(format!(
        "quiver-vorgaben-{}-{tag}-{stempel}",
        std::process::id()
    ));
    std::fs::create_dir_all(&pfad).unwrap();
    pfad
}

#[test]
fn json_wird_gelesen_und_bereinigt() {
    let vorgaben = aus_json(
        r#"{"ServerUrl":"https://quiver.example.test/","Autostart":true,"UpdateHinweis":"Neue Version {version}"}"#,
    )
    .bereinigen();
    assert_eq!(
        vorgaben.server_url.as_deref(),
        Some("https://quiver.example.test")
    );
    assert_eq!(vorgaben.autostart, Some(true));
    assert_eq!(
        vorgaben.update_hinweis.as_deref(),
        Some("Neue Version {version}")
    );
    assert!(vorgaben.verwaltet());
    assert!(Vorgaben::default().ist_leer());
    assert!(!Vorgaben::default().verwaltet());
}

#[test]
fn eine_ungueltige_adresse_wird_ignoriert_und_der_rest_bleibt() {
    let vorgaben = aus_json(r#"{"ServerUrl":"ftp://example.test","Autostart":false}"#).bereinigen();
    assert_eq!(vorgaben.server_url, None);
    assert_eq!(vorgaben.autostart, Some(false));
    // http nur auf der eigenen Maschine.
    let loopback = aus_json(r#"{"ServerUrl":"http://127.0.0.1:8080"}"#).bereinigen();
    assert_eq!(
        loopback.server_url.as_deref(),
        Some("http://127.0.0.1:8080")
    );
    let fremd = aus_json(r#"{"ServerUrl":"http://example.test"}"#).bereinigen();
    assert_eq!(fremd.server_url, None);
}

#[test]
fn ein_mehrzeiliger_hinweis_wird_eine_zeile() {
    let vorgaben =
        aus_json(r#"{"UpdateHinweis":"Bitte\nüber\tIntune  installieren"}"#).bereinigen();
    assert_eq!(
        vorgaben.update_hinweis.as_deref(),
        Some("Bitte über Intune installieren")
    );
    let leer = aus_json(r#"{"UpdateHinweis":"   "}"#).bereinigen();
    assert_eq!(leer.update_hinweis, None);
}

#[test]
fn schalter_werden_verstanden() {
    for (text, erwartet) in [
        ("1", Some(true)),
        ("0", Some(false)),
        ("true", Some(true)),
        ("an", Some(true)),
        ("aus", Some(false)),
        ("vielleicht", None),
    ] {
        assert_eq!(bool_lesen(text), erwartet, "{text}");
    }
}

#[test]
fn eine_datei_wird_gelesen() {
    let ordner = ordner("datei");
    let pfad = ordner.join("vorgaben.json");
    std::fs::write(
        &pfad,
        r#"{"ServerUrl":"https://quiver.example.test","Autostart":true}"#,
    )
    .unwrap();
    let vorgaben = lesen(&Ort::Datei(pfad));
    assert_eq!(
        vorgaben.server_url.as_deref(),
        Some("https://quiver.example.test")
    );
    assert_eq!(vorgaben.autostart, Some(true));
    // Eine fehlende Datei heißt: keine Vorgaben.
    assert!(lesen(&Ort::Datei(ordner.join("fehlt.json"))).ist_leer());
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn eine_plist_wird_gelesen() {
    let text = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>ServerUrl</key>
	<string>https://quiver.example.test</string>
	<key>Autostart</key>
	<true/>
	<key>UpdateHinweis</key>
	<string>Neue Version {version} über Intune</string>
	<key>Etwas</key>
	<integer>3</integer>
</dict>
</plist>
"#;
    let vorgaben = aus_plist(text).bereinigen();
    assert_eq!(
        vorgaben.server_url.as_deref(),
        Some("https://quiver.example.test")
    );
    assert_eq!(vorgaben.autostart, Some(true));
    assert_eq!(
        vorgaben.update_hinweis.as_deref(),
        Some("Neue Version {version} über Intune")
    );
    // Werte mit XML-Ersetzungen kommen heil an.
    let werte = plist_werte("<key>UpdateHinweis</key><string>a &amp; b</string>");
    assert_eq!(
        werte.get("UpdateHinweis").map(String::as_str),
        Some("a & b")
    );
}

/// Das Angebot (Aufgabe 39) ist ein eigener Schalter – in allen drei Formen.
#[test]
fn das_update_angebot_wird_gelesen() {
    let json = aus_json(r#"{"UpdateAngebot":false}"#).bereinigen();
    assert_eq!(json.update_angebot, Some(false));
    assert!(json.verwaltet(), "ein Schalter ist eine Vorgabe");
    // In JSON und plist ist es ein echter Schalter (true/false); Text wie
    // „aus“ gilt nur in der Registry.
    assert_eq!(aus_json(r#"{"UpdateAngebot":"aus"}"#).update_angebot, None);
    // Unsinn wird verworfen und ändert nichts.
    assert_eq!(
        aus_json(r#"{"UpdateAngebot":"vielleicht"}"#).update_angebot,
        None
    );

    let plist = aus_plist("<key>UpdateAngebot</key><false/><key>Autostart</key><true/>");
    assert_eq!(plist.update_angebot, Some(false));
    assert_eq!(plist.autostart, Some(true));

    let mut werte = BTreeMap::new();
    werte.insert(String::from("UpdateAngebot"), String::from("0"));
    let registry = aus_werten(&werte);
    assert_eq!(registry.update_angebot, Some(false));
    assert_eq!(registry.autostart, None);

    // Ohne Angabe gilt: das Angebot ist an (Vorgabe des Clients).
    assert_eq!(Vorgaben::default().update_angebot, None);
}

#[test]
fn registry_werte_werden_gelesen() {
    let mut werte = BTreeMap::new();
    werte.insert(String::from("ServerUrl"), String::from("https://a.test"));
    werte.insert(String::from("Autostart"), String::from("1"));
    let vorgaben = aus_werten(&werte).bereinigen();
    assert_eq!(vorgaben.server_url.as_deref(), Some("https://a.test"));
    assert_eq!(vorgaben.autostart, Some(true));
    assert_eq!(vorgaben.update_hinweis, None);
}
