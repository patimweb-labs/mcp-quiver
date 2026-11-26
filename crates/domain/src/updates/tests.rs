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
