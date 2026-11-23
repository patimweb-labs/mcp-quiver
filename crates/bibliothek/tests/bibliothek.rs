// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahme Aufgabe 01: `server.json` ↔ [`Eintrag`].

use quiver_bibliothek::{exportieren, importieren, ImportFehler, META_SCHLUESSEL};
use quiver_domain::{Eintrag, EintragId, Feld, Herkunft, Laufzeit, Quelle, Werte};
use serde_json::Value;

fn beispiel(datei: &str) -> Value {
    let pfad = format!("{}/tests/beispiele/{datei}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&pfad).unwrap_or_else(|e| panic!("{pfad}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{pfad}: {e}"))
}

fn werte(paare: &[(&str, &str)]) -> Werte {
    paare
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn stdio(eintrag: &Eintrag) -> (&str, &[String], &Werte) {
    match &eintrag.laufzeit {
        Laufzeit::Stdio {
            befehl,
            argumente,
            umgebung,
        } => (befehl, argumente, umgebung),
        andere => panic!("kein Stdio: {andere:?}"),
    }
}

fn remote(eintrag: &Eintrag) -> (&str, &Werte) {
    match &eintrag.laufzeit {
        Laufzeit::Remote { url, kopfzeilen } => (url, kopfzeilen),
        andere => panic!("kein Remote: {andere:?}"),
    }
}

#[test]
fn npm_mit_geheimer_variable_und_argumenten() {
    let eintrag = importieren(&beispiel("npm-geheim.json")).unwrap();

    assert_eq!(eintrag.id, EintragId::neu("weather"));
    assert_eq!(eintrag.titel, "Weather");
    assert_eq!(eintrag.beschreibung, "Wetterdaten als MCP-Tools");
    assert_eq!(eintrag.version, "1.2.0");
    assert_eq!(eintrag.herkunft, Herkunft::Fremd);

    let (befehl, argumente, umgebung) = stdio(&eintrag);
    assert_eq!(befehl, "npx");
    assert_eq!(
        argumente,
        [
            "-y",
            "@someone/weather-mcp@1.2.0",
            "--fast",
            "--region",
            "emea",
            "--leise"
        ]
    );
    assert_eq!(umgebung, &werte(&[("API_KEY", "${API_KEY}")]));

    assert_eq!(
        eintrag.felder,
        [Feld {
            name: "API_KEY".into(),
            beschreibung: "Schluessel des Wetterdienstes".into(),
            quelle: Quelle::Geheim,
            pflicht: true,
            standard: None,
            link: None,
        }]
    );
}

#[test]
fn pypi_ohne_variablen() {
    let eintrag = importieren(&beispiel("pypi-ohne-variablen.json")).unwrap();

    assert_eq!(eintrag.id, EintragId::neu("notes"));
    // kein title im JSON → die id
    assert_eq!(eintrag.titel, "notes");
    assert_eq!(eintrag.version, "0.4.1");

    let (befehl, argumente, umgebung) = stdio(&eintrag);
    assert_eq!(befehl, "uvx");
    assert_eq!(argumente, ["notes-mcp==0.4.1"]);
    assert!(umgebung.is_empty());
    assert!(eintrag.felder.is_empty());
}

#[test]
fn oci_mit_zwei_variablen_haelt_die_reihenfolge() {
    let eintrag = importieren(&beispiel("oci-zwei-variablen.json")).unwrap();

    // Name wird kleingeschrieben, alles außer a-z0-9- zu '-'
    assert_eq!(eintrag.id, EintragId::neu("postgres-mcp"));

    let (befehl, argumente, umgebung) = stdio(&eintrag);
    assert_eq!(befehl, "docker");
    assert_eq!(
        argumente,
        [
            "run",
            "-i",
            "--rm",
            "-e",
            "DB_HOST",
            "-e",
            "DB_PASSWORD",
            "ghcr.io/someone/db-mcp:3.0.0",
            "--readonly"
        ]
    );
    assert_eq!(
        umgebung,
        &werte(&[("DB_HOST", "${DB_HOST}"), ("DB_PASSWORD", "${DB_PASSWORD}")])
    );

    assert_eq!(eintrag.felder.len(), 2);
    assert_eq!(eintrag.felder[0].quelle, Quelle::Client);
    assert_eq!(eintrag.felder[1].quelle, Quelle::Geheim);
    assert_eq!(eintrag.felder[1].beschreibung, "Passwort der Datenbank");
}

#[test]
fn nur_remote_mit_header() {
    let eintrag = importieren(&beispiel("remote-header.json")).unwrap();

    assert_eq!(eintrag.id, EintragId::neu("docs"));
    let (url, kopfzeilen) = remote(&eintrag);
    assert_eq!(url, "https://example.com/mcp");
    assert_eq!(kopfzeilen, &werte(&[("Authorization", "${Authorization}")]));
    assert_eq!(
        eintrag.felder,
        [Feld {
            name: "Authorization".into(),
            beschreibung: "Bearer-Token".into(),
            quelle: Quelle::Geheim,
            pflicht: true,
            standard: None,
            link: None,
        }]
    );
}

#[test]
fn meta_eintrag_gewinnt_gegen_packages() {
    let eintrag = importieren(&beispiel("meta-gewinnt.json")).unwrap();

    assert_eq!(eintrag.id, EintragId::neu("quiver-eigen"));
    assert_eq!(eintrag.version, "9.9.9");
    assert_eq!(eintrag.titel, "Eigener Eintrag");
    assert_eq!(eintrag.beschreibung, "Gewinnt gegen packages");
    assert_eq!(eintrag.herkunft, Herkunft::Eigen);
    let (url, kopfzeilen) = remote(&eintrag);
    assert_eq!(url, "https://quiver.example/mcp");
    assert_eq!(kopfzeilen, &werte(&[("Authorization", "${Authorization}")]));
}

#[test]
fn rundlauf_importieren_exportieren() {
    let eintrag = Eintrag {
        id: EintragId::neu("wetter-eigen"),
        version: "2.1.0".into(),
        titel: "Wetter (eigen)".into(),
        beschreibung: "Von uns gebaut".into(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: "npx".into(),
            argumente: vec![
                "-y".into(),
                "@patimweb/weather-mcp@2.1.0".into(),
                "--leise".into(),
            ],
            umgebung: werte(&[("API_KEY", "${API_KEY}"), ("STADT", "${STADT}")]),
        },
        felder: vec![
            Feld {
                name: "API_KEY".into(),
                beschreibung: "Schluessel".into(),
                quelle: Quelle::Geheim,
                pflicht: true,
                standard: None,
                link: None,
            },
            Feld {
                name: "STADT".into(),
                beschreibung: "Vorgabe".into(),
                quelle: Quelle::Client,
                pflicht: false,
                standard: Some("Berlin".into()),
                link: None,
            },
        ],
    };

    let json = exportieren(&eintrag);
    assert_eq!(
        json["_meta"][META_SCHLUESSEL]["id"],
        Value::String("wetter-eigen".into())
    );
    assert_eq!(
        json["name"],
        Value::String("io.github.patimweb-labs.quiver/wetter-eigen".into())
    );
    assert_eq!(json["version"], Value::String("2.1.0".into()));
    assert_eq!(
        json["packages"][0]["registryType"],
        Value::String("npm".into())
    );
    assert_eq!(
        json["packages"][0]["identifier"],
        Value::String("@patimweb/weather-mcp".into())
    );

    assert_eq!(importieren(&json).unwrap(), eintrag);
}

#[test]
fn rundlauf_mit_unbekanntem_befehl_traegt_meta() {
    let eintrag = Eintrag {
        id: EintragId::neu("eigen-binary"),
        version: "0.1.0".into(),
        titel: "Eigenes Binary".into(),
        beschreibung: String::new(),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: "quiver-wetter".into(),
            argumente: vec!["--stdio".into()],
            umgebung: Werte::new(),
        },
        felder: Vec::new(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
    };

    let json = exportieren(&eintrag);
    // ohne npm/uvx/docker lässt sich kein Paket ableiten; _meta genügt
    assert!(json.get("packages").is_none());
    assert_eq!(importieren(&json).unwrap(), eintrag);
}

#[test]
fn fehler_ohne_paket_und_remote() {
    let fehler = importieren(&beispiel("ohne-laufzeit.json")).unwrap_err();
    assert!(matches!(fehler, ImportFehler::KeineLaufzeit), "{fehler:?}");
}

#[test]
fn fehler_bei_leerer_id() {
    let fehler = importieren(&beispiel("leerer-name.json")).unwrap_err();
    assert!(matches!(fehler, ImportFehler::LeereId(_)), "{fehler:?}");
}

#[test]
fn fehler_bei_unbekanntem_pakettyp() {
    let fehler = importieren(&beispiel("unbekannter-pakettyp.json")).unwrap_err();
    assert!(
        matches!(fehler, ImportFehler::UnbekannterPakettyp { .. }),
        "{fehler:?}"
    );
}

#[test]
fn fehlende_optionale_felder_sind_kein_fehler() {
    let json = serde_json::json!({
        "name": "io.github.someone/minimal",
        "version": "0.0.1",
        "packages": [{
            "registryType": "pypi",
            "identifier": "minimal",
            "transport": { "type": "stdio" }
        }]
    });
    let eintrag = importieren(&json).unwrap();
    assert_eq!(eintrag.id, EintragId::neu("minimal"));
    assert_eq!(eintrag.titel, "minimal");
    assert_eq!(eintrag.beschreibung, "");
    // Paket ohne version → Version des Eintrags
    assert_eq!(stdio(&eintrag).1, ["minimal==0.0.1"]);
}
