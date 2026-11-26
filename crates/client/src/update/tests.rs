// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests für den Update-Hinweis: nur anzeigen, nie tauschen.

use super::*;

fn angebot(neueste: Option<&str>) -> UpdateAngebot {
    UpdateAngebot {
        aktuelle_version: String::from("0.2.0"),
        neueste_version: neueste.map(str::to_string),
        aenderungen: Some(String::from(
            "https://github.com/patimweb-labs/mcp-quiver/releases/tag/v0.3.0",
        )),
    }
}

#[test]
fn eine_neuere_version_wird_angezeigt() {
    let lage = lage_aus(&angebot(Some("v0.3.0")), &Vorgaben::default(), "0.2.0");
    let UpdateLage::Verfuegbar {
        version,
        hinweis,
        aenderungen,
    } = lage
    else {
        panic!("erwartet: verfügbar, war {lage:?}");
    };
    assert_eq!(version, "0.3.0");
    assert_eq!(hinweis, "Version 0.3.0 ist verfügbar");
    assert!(aenderungen.is_some_and(|link| link.starts_with("https://")));
}

#[test]
fn ohne_neuere_version_gibt_es_nichts_zu_zeigen() {
    assert_eq!(
        lage_aus(&angebot(None), &Vorgaben::default(), "0.2.0"),
        UpdateLage::Keins
    );
    assert_eq!(
        lage_aus(&angebot(Some("0.2.0")), &Vorgaben::default(), "0.2.0"),
        UpdateLage::Keins
    );
    // Kein Rückschritt, auch wenn der Server eine ältere Version nennt.
    assert_eq!(
        lage_aus(&angebot(Some("0.1.0")), &Vorgaben::default(), "0.2.0"),
        UpdateLage::Keins
    );
    assert_eq!(
        lage_aus(&angebot(Some("kaputt")), &Vorgaben::default(), "0.2.0"),
        UpdateLage::Keins
    );
}

#[test]
fn ein_verwaltetes_geraet_sagt_dass_die_verwaltung_es_einspielt() {
    let vorgaben = Vorgaben {
        server_url: Some(String::from("https://quiver.example.test")),
        ..Vorgaben::default()
    };
    let lage = lage_aus(&angebot(Some("0.3.0")), &vorgaben, "0.2.0");
    let UpdateLage::Verfuegbar { hinweis, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert_eq!(hinweis, "Version 0.3.0 kommt über die Geräteverwaltung");
}

#[test]
fn der_text_der_firma_geht_vor() {
    let vorgaben = Vorgaben {
        update_hinweis: Some(String::from("Bitte {version} über Intune holen")),
        ..Vorgaben::default()
    };
    let lage = lage_aus(&angebot(Some("0.3.0")), &vorgaben, "0.2.0");
    let UpdateLage::Verfuegbar { hinweis, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert_eq!(hinweis, "Bitte 0.3.0 über Intune holen");
}

#[test]
fn ein_link_ohne_https_wird_nicht_angeboten() {
    let mut angebot = angebot(Some("0.3.0"));
    angebot.aenderungen = Some(String::from("javascript:alert(1)"));
    let lage = lage_aus(&angebot, &Vorgaben::default(), "0.2.0");
    let UpdateLage::Verfuegbar { aenderungen, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert_eq!(aenderungen, None);
}

#[test]
fn die_abfrage_enthaelt_nur_unbedenkliche_zeichen() {
    assert_eq!(url_teil("0.3.0-rc.1 / ../x"), "0.3.0-rc.1..x");
}
