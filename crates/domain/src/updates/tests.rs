// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests für [`super`]: Versionen vergleichen und die Antwort der
//! Veröffentlichung lesen. Keine Datei, kein Netz.

use super::*;

#[test]
fn versionen_werden_verglichen() {
    assert!(version_neuer("0.3.0", "0.2.9"));
    assert!(version_neuer("v0.3.0", "0.2.0"));
    assert!(version_neuer("1.0.0", "1.0.0-rc.1"));
    assert!(!version_neuer("0.2.0", "0.2.0"));
    assert!(!version_neuer("0.2.0", "0.3.0"));
    // Unlesbares ist nie neuer – ein Rückschritt ist ausgeschlossen.
    assert!(!version_neuer("kaputt", "0.1.0"));
    assert!(!version_neuer("0.3.0", ""));
    assert_eq!(
        version_lesen("v1.2.3-rc.1").unwrap().to_string(),
        "1.2.3-rc.1"
    );
    assert!(version_lesen("1.2.3.4.5").is_none());
    assert!(version_lesen("1..3").is_none());
}

#[test]
fn der_hinweis_setzt_die_version_ein() {
    assert_eq!(
        hinweis_text(TEXT_OHNE_VERWALTUNG, "0.3.0"),
        "Version 0.3.0 ist verfügbar"
    );
    assert_eq!(
        hinweis_text(TEXT_MIT_VERWALTUNG, "0.3.0"),
        "Version 0.3.0 kommt über die Geräteverwaltung"
    );
    // Ohne Platzhalter bleibt der Text, wie ihn die Firma geschrieben hat.
    assert_eq!(
        hinweis_text("Bitte über Intune", "0.3.0"),
        "Bitte über Intune"
    );
}

#[test]
fn eine_veroeffentlichung_wird_gelesen() {
    let antwort = r#"{
        "tag_name": "v0.3.0",
        "html_url": "https://github.com/patimweb-labs/mcp-quiver/releases/tag/v0.3.0",
        "published_at": "2026-03-01T10:00:00Z",
        "draft": false,
        "prerelease": false
    }"#;
    let veroeffentlichung = veroeffentlichung_lesen(antwort).unwrap();
    assert_eq!(veroeffentlichung.version, "0.3.0");
    assert_eq!(
        veroeffentlichung.veroeffentlicht.as_deref(),
        Some("2026-03-01T10:00:00Z")
    );
    assert!(veroeffentlichung
        .aenderungen
        .as_deref()
        .unwrap()
        .starts_with("https://github.com/"));
}

#[test]
fn entwuerfe_und_vorabfassungen_sind_kein_angebot() {
    for antwort in [
        r#"{"tag_name":"v0.3.0","draft":true}"#,
        r#"{"tag_name":"v0.3.0-rc.1","prerelease":true}"#,
        r#"{"tag_name":""}"#,
        r#"{"tag_name":"nicht-neu"}"#,
        "kein JSON",
    ] {
        assert!(
            veroeffentlichung_lesen(antwort).is_err(),
            "muss abgelehnt werden: {antwort}"
        );
    }
}

#[test]
fn ein_link_ohne_https_wird_verschwiegen() {
    let antwort = r#"{"tag_name":"v0.3.0","html_url":"javascript:alert(1)"}"#;
    let veroeffentlichung = veroeffentlichung_lesen(antwort).unwrap();
    assert_eq!(veroeffentlichung.version, "0.3.0");
    assert_eq!(veroeffentlichung.aenderungen, None);
    assert_eq!(veroeffentlichung.veroeffentlicht, None);
}

// ------------------------------------------------------------- Pakete (39)

const SHA_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn veroeffentlichung_mit(dateien: &[(&str, u64)]) -> Veroeffentlichung {
    Veroeffentlichung {
        version: String::from("0.5.0"),
        veroeffentlicht: None,
        aenderungen: None,
        dateien: dateien
            .iter()
            .map(|(name, groesse)| ReleaseDatei {
                name: String::from(*name),
                adresse: Some(format!("https://example.test/download/{name}")),
                groesse: *groesse,
            })
            .collect(),
    }
}

#[test]
fn die_art_kommt_aus_der_endung() {
    assert_eq!(PaketArt::aus_name("a.zip"), Some(PaketArt::Zip));
    assert_eq!(PaketArt::aus_name("a.MSI"), Some(PaketArt::Msi));
    assert_eq!(PaketArt::aus_name("a.tar.gz"), Some(PaketArt::TarGz));
    assert_eq!(PaketArt::aus_name("a.AppImage"), Some(PaketArt::AppImage));
    assert_eq!(PaketArt::aus_name("a.dmg"), Some(PaketArt::Dmg));
    assert_eq!(PaketArt::aus_name("a.deb"), Some(PaketArt::Deb));
    assert_eq!(PaketArt::aus_name("a.rpm"), Some(PaketArt::Rpm));
    assert_eq!(PaketArt::aus_name("a.pkg"), Some(PaketArt::Pkg));
    assert_eq!(PaketArt::aus_name("SHA256SUMS"), None);
    // Jede Art findet sich über ihren kurzen Namen wieder.
    for art in [
        PaketArt::Zip,
        PaketArt::Msi,
        PaketArt::Dmg,
        PaketArt::Pkg,
        PaketArt::AppImage,
        PaketArt::TarGz,
        PaketArt::Deb,
        PaketArt::Rpm,
    ] {
        assert_eq!(PaketArt::aus_text(art.als_text()), Some(art));
    }
    assert_eq!(PaketArt::aus_text("exe"), None);
}

#[test]
fn ein_paket_gehoert_zu_system_und_architektur() {
    assert!(datei_passt(
        "quiver-0.5.0-linux-x86_64.tar.gz",
        "linux",
        "x64"
    ));
    assert!(datei_passt(
        "quiver-0.5.0-windows-x86_64.zip",
        "windows",
        "x64"
    ));
    assert!(datei_passt(
        "quiver-0.5.0-macos-arm64.dmg",
        "macos",
        "arm64"
    ));
    // aarch64 gilt als arm64.
    assert!(datei_passt(
        "quiver-0.5.0-linux-aarch64.AppImage",
        "linux",
        "arm64"
    ));
    assert!(!datei_passt(
        "quiver-0.5.0-macos-arm64.dmg",
        "linux",
        "arm64"
    ));
    assert!(!datei_passt(
        "quiver-0.5.0-linux-x86_64.tar.gz",
        "linux",
        "arm64"
    ));
    assert!(!datei_passt(
        "quiver-0.5.0-linux-x86_64.tar.gz",
        "linux",
        "riscv"
    ));
}

#[test]
fn pruefsummen_werden_gelesen() {
    let text = format!(
        "{SHA_A}  quiver-0.5.0-linux-x86_64.tar.gz\n\
         {SHA_A} *quiver-0.5.0-windows-x86_64.zip\n\
         # ein Kommentar\n\
         kaputt quiver-0.5.0-macos-arm64.dmg\n\
         {SHA_A} ../boese\n"
    );
    let karte = pruefsummen_lesen(&text);
    assert_eq!(
        karte
            .get("quiver-0.5.0-linux-x86_64.tar.gz")
            .map(String::as_str),
        Some(SHA_A)
    );
    assert!(karte.contains_key("quiver-0.5.0-windows-x86_64.zip"));
    // Was keinen reinen Dateinamen hat oder keine Prüfsumme trägt, fehlt.
    assert!(!karte.contains_key("../boese"));
    assert!(!karte.contains_key("quiver-0.5.0-macos-arm64.dmg"));
    assert!(pruefsummen_lesen("").is_empty());
}

#[test]
fn nur_gepruefte_pakete_des_eigenen_systems_werden_angeboten() {
    let veroeffentlichung = veroeffentlichung_mit(&[
        ("quiver-0.5.0-linux-x86_64.tar.gz", 100),
        ("quiver-0.5.0-linux-x86_64.AppImage", 200),
        ("quiver-0.5.0-macos-arm64.dmg", 300),
        ("quiver-0.5.0-linux-x86_64.zip", 0),
    ]);
    let mut sums = BTreeMap::new();
    for name in [
        "quiver-0.5.0-linux-x86_64.tar.gz",
        "quiver-0.5.0-linux-x86_64.AppImage",
        "quiver-0.5.0-macos-arm64.dmg",
        "quiver-0.5.0-linux-x86_64.zip",
    ] {
        sums.insert(String::from(name), String::from(SHA_A));
    }
    let pakete = pakete_fuer(&veroeffentlichung, "linux", "x64", &sums);
    let namen: Vec<&str> = pakete.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        namen,
        vec![
            "quiver-0.5.0-linux-x86_64.tar.gz",
            "quiver-0.5.0-linux-x86_64.AppImage"
        ],
        "nur Linux, und das ZIP ohne Größe fehlt"
    );
    assert_eq!(pakete[0].art, PaketArt::TarGz);

    // Ohne Prüfsumme gibt es kein Paket – der Client könnte es nicht prüfen.
    assert!(pakete_fuer(&veroeffentlichung, "linux", "x64", &BTreeMap::new()).is_empty());
    // Ein anderes System bekommt nichts.
    assert!(pakete_fuer(&veroeffentlichung, "windows", "x64", &sums).is_empty());
}

#[test]
fn nur_adressen_mit_https_oder_loopback() {
    assert!(adresse_erlaubt("https://github.com/a/b.zip"));
    assert!(adresse_erlaubt("http://127.0.0.1:8080/download/x"));
    assert!(adresse_erlaubt("http://localhost:1/x"));
    assert!(!adresse_erlaubt("http://example.test/x"));
    assert!(!adresse_erlaubt("ftp://example.test/x"));
    assert!(!adresse_erlaubt("javascript:alert(1)"));
    assert!(!adresse_erlaubt(""));
}

#[test]
fn die_dateien_der_veroeffentlichung_werden_gelesen() {
    let antwort = r#"{
        "tag_name": "v0.5.0",
        "html_url": "https://github.com/patimweb-labs/mcp-quiver/releases/tag/v0.5.0",
        "assets": [
            {"name":"quiver-0.5.0-linux-x86_64.tar.gz",
             "browser_download_url":"https://github.com/x/download/a.tar.gz","size":123},
            {"name":"SHA256SUMS","browser_download_url":"https://github.com/x/download/SHA256SUMS","size":99},
            {"name":"../boese.zip","browser_download_url":"https://github.com/x/download/b","size":1},
            {"name":"quiver-0.5.0.zip","browser_download_url":"http://example.test/x","size":1}
        ]
    }"#;
    let veroeffentlichung = veroeffentlichung_lesen(antwort).unwrap();
    assert_eq!(veroeffentlichung.version, "0.5.0");
    let namen: Vec<&str> = veroeffentlichung
        .dateien
        .iter()
        .map(|datei| datei.name.as_str())
        .collect();
    assert_eq!(
        namen,
        vec![
            "quiver-0.5.0-linux-x86_64.tar.gz",
            "SHA256SUMS",
            "quiver-0.5.0.zip"
        ],
        "der unsichere Name fällt weg"
    );
    // Die unsichere Adresse bleibt leer – geholt wird von dort nichts.
    assert_eq!(veroeffentlichung.dateien[2].adresse, None);
    assert_eq!(veroeffentlichung.dateien[0].groesse, 123);
    assert!(veroeffentlichung.pruefsummen_datei().is_some());
}
