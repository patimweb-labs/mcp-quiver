// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests für den Update-Hinweis und das Angebot (Aufgabe 34, 39).

use std::path::PathBuf;

use super::*;
use quiver_domain::updates::PaketArt;

/// Eine Aufstellung im Testordner: portable ZIP-Installation.
fn aufstellung(art: Installationsart) -> Aufstellung {
    Aufstellung {
        art,
        programm: PathBuf::from("/test/quiver"),
        schreibbar: true,
    }
}

fn paket(art: PaketArt, sha: &str) -> UpdatePaket {
    UpdatePaket {
        name: format!("quiver-0.3.0-linux-x86_64.{}", art.endung()),
        groesse: 1024,
        sha256: sha.to_string(),
        art: art.als_text().to_string(),
    }
}

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn angebot(neueste: Option<&str>) -> UpdateAngebot {
    UpdateAngebot {
        aktuelle_version: String::from("0.2.0"),
        neueste_version: neueste.map(str::to_string),
        aenderungen: Some(String::from(
            "https://github.com/patimweb-labs/mcp-quiver/releases/tag/v0.3.0",
        )),
        pakete: vec![paket(PaketArt::Zip, SHA)],
    }
}

#[test]
fn eine_neuere_version_wird_angezeigt() {
    let lage = lage_aus(
        &angebot(Some("v0.3.0")),
        &Vorgaben::default(),
        "0.2.0",
        &aufstellung(Installationsart::Zip),
    );
    let UpdateLage::Verfuegbar {
        version,
        hinweis,
        aenderungen,
        angebot,
    } = lage
    else {
        panic!("erwartet: verfügbar, war {lage:?}");
    };
    assert_eq!(version, "0.3.0");
    assert_eq!(hinweis, "Version 0.3.0 ist verfügbar");
    assert!(aenderungen.is_some_and(|link| link.starts_with("https://")));
    // Ein passendes Paket: der Knopf darf erscheinen.
    assert!(matches!(angebot, Angebot::Bereit { .. }));
}

#[test]
fn ohne_neuere_version_gibt_es_nichts_zu_zeigen() {
    for neueste in [None, Some("0.2.0"), Some("0.1.0"), Some("kaputt")] {
        assert_eq!(
            lage_aus(
                &angebot(neueste),
                &Vorgaben::default(),
                "0.2.0",
                &aufstellung(Installationsart::Zip)
            ),
            UpdateLage::Keins,
            "{neueste:?}"
        );
    }
}

#[test]
fn ein_verwaltetes_geraet_sagt_dass_die_verwaltung_es_einspielt() {
    let vorgaben = Vorgaben {
        server_url: Some(String::from("https://quiver.example.test")),
        ..Vorgaben::default()
    };
    let lage = lage_aus(
        &angebot(Some("0.3.0")),
        &vorgaben,
        "0.2.0",
        &aufstellung(Installationsart::Zip),
    );
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
    let lage = lage_aus(
        &angebot(Some("0.3.0")),
        &vorgaben,
        "0.2.0",
        &aufstellung(Installationsart::Zip),
    );
    let UpdateLage::Verfuegbar { hinweis, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert_eq!(hinweis, "Bitte 0.3.0 über Intune holen");
}

#[test]
fn ein_link_ohne_https_wird_nicht_angeboten() {
    let mut angebot = angebot(Some("0.3.0"));
    angebot.aenderungen = Some(String::from("javascript:alert(1)"));
    let lage = lage_aus(
        &angebot,
        &Vorgaben::default(),
        "0.2.0",
        &aufstellung(Installationsart::Zip),
    );
    let UpdateLage::Verfuegbar { aenderungen, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert_eq!(aenderungen, None);
}

#[test]
fn die_abfrage_enthaelt_nur_unbedenkliche_zeichen() {
    assert_eq!(url_teil("0.3.0-rc.1 / ../x"), "0.3.0-rc.1..x");
}

// ------------------------------------------------------ Das Angebot (39)

#[test]
fn eine_vorgabe_schaltet_das_angebot_ab() {
    let vorgaben = Vorgaben {
        update_angebot: Some(false),
        ..Vorgaben::default()
    };
    let lage = lage_aus(
        &angebot(Some("0.3.0")),
        &vorgaben,
        "0.2.0",
        &aufstellung(Installationsart::Zip),
    );
    let UpdateLage::Verfuegbar { angebot, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert_eq!(
        angebot,
        Angebot::Keins {
            grund: AngebotGrund::Vorgabe
        },
        "der Hinweis bleibt, der Knopf nicht"
    );
}

#[test]
fn ohne_paket_fuer_diese_installation_gibt_es_keinen_knopf() {
    for art in [
        Installationsart::Msi,
        Installationsart::AppBundle,
        Installationsart::AppImage,
        Installationsart::TarGz,
    ] {
        let lage = lage_aus(
            &angebot(Some("0.3.0")),
            &Vorgaben::default(),
            "0.2.0",
            &aufstellung(art),
        );
        let UpdateLage::Verfuegbar { angebot, .. } = lage else {
            panic!("erwartet: verfügbar");
        };
        assert_eq!(
            angebot,
            Angebot::Keins {
                grund: AngebotGrund::KeinPaket
            },
            "{art:?}"
        );
    }
    // Ein Paket, das zu keiner Art passt, wird nicht angeboten.
    let mut falsch = angebot(Some("0.3.0"));
    falsch.pakete = vec![paket(PaketArt::Msi, SHA)];
    let lage = lage_aus(
        &falsch,
        &Vorgaben::default(),
        "0.2.0",
        &aufstellung(Installationsart::Zip),
    );
    let UpdateLage::Verfuegbar { angebot, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert!(matches!(
        angebot,
        Angebot::Keins {
            grund: AngebotGrund::KeinPaket
        }
    ));
}

#[test]
fn ohne_schreibrecht_oder_mit_rootbedarf_gibt_es_nur_einen_hinweis() {
    let mut ohne = aufstellung(Installationsart::Zip);
    ohne.schreibbar = false;
    for (art, grund) in [
        (ohne, AngebotGrund::KeinSchreibrecht),
        (
            aufstellung(Installationsart::Deb),
            AngebotGrund::BrauchtRechte,
        ),
        (
            aufstellung(Installationsart::Rpm),
            AngebotGrund::BrauchtRechte,
        ),
        (
            aufstellung(Installationsart::Unbekannt),
            AngebotGrund::Unbekannt,
        ),
    ] {
        let lage = lage_aus(&angebot(Some("0.3.0")), &Vorgaben::default(), "0.2.0", &art);
        let UpdateLage::Verfuegbar { angebot, .. } = lage else {
            panic!("erwartet: verfügbar");
        };
        assert_eq!(angebot, Angebot::Keins { grund }, "{grund:?}");
    }
}

#[test]
fn ein_paket_ohne_brauchbare_pruefsumme_wird_nicht_angeboten() {
    for (sha, groesse) in [
        (String::from("kurz"), 1024),
        (String::new(), 1024),
        (
            String::from("ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"),
            1024,
        ),
        (SHA.to_string(), 0),
    ] {
        let mut falsch = angebot(Some("0.3.0"));
        falsch.pakete = vec![UpdatePaket {
            name: String::from("quiver-0.3.0-linux-x86_64.zip"),
            groesse,
            sha256: sha.clone(),
            art: String::from("zip"),
        }];
        let lage = lage_aus(
            &falsch,
            &Vorgaben::default(),
            "0.2.0",
            &aufstellung(Installationsart::Zip),
        );
        let UpdateLage::Verfuegbar { angebot, .. } = lage else {
            panic!("erwartet: verfügbar");
        };
        assert!(
            matches!(
                angebot,
                Angebot::Keins {
                    grund: AngebotGrund::KeinPaket
                }
            ),
            "{sha:?} / {groesse}"
        );
    }
}

#[test]
fn ein_paketname_mit_pfad_wird_nicht_genommen() {
    let mut falsch = angebot(Some("0.3.0"));
    falsch.pakete[0].name = String::from("../../quiver.zip");
    let lage = lage_aus(
        &falsch,
        &Vorgaben::default(),
        "0.2.0",
        &aufstellung(Installationsart::Zip),
    );
    let UpdateLage::Verfuegbar { angebot, .. } = lage else {
        panic!("erwartet: verfügbar");
    };
    assert!(matches!(
        angebot,
        Angebot::Keins {
            grund: AngebotGrund::KeinPaket
        }
    ));
}
