// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Unit-Tests für die Modell-Ableitung und die Agent-Token-Prüfung.

use std::collections::{BTreeMap, BTreeSet};

use quiver_domain::{
    Eintrag, EintragId, Feld, Herkunft, Laufzeit, Quelle, Soll, SollEintrag, Werte,
};
use quiver_protocol::EintragZustand;
use rmcp::model::{JsonObject, Tool};

use super::*;
use crate::profil::Agent;

fn feld(name: &str, quelle: Quelle, pflicht: bool) -> Feld {
    Feld {
        name: name.into(),
        beschreibung: format!("Beschreibung {name}"),
        quelle,
        pflicht,
        standard: None,
    }
}

fn eintrag(id: &str) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "1.0.0".into(),
        titel: format!("Titel {id}"),
        beschreibung: format!("Beschreibung {id}"),
        aktiv: true,
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: "echo".into(),
            argumente: vec!["${PFAD}".into()],
            umgebung: Werte::new(),
        },
        felder: vec![
            feld("SERVER", Quelle::Server, true),
            feld("PFAD", Quelle::Client, true),
            feld("TOKEN", Quelle::Geheim, true),
        ],
    }
}

fn werkzeug(name: &str) -> Tool {
    let schema: JsonObject = serde_json::from_value(serde_json::json!({
        "type": "object",
        "properties": { "text": { "type": "string" } }
    }))
    .expect("Schema");
    Tool::new(
        name.to_string(),
        "Ein Testwerkzeug",
        std::sync::Arc::new(schema),
    )
}

#[test]
fn sicht_zeigt_nur_client_und_geheim_felder() {
    let soll = Soll::neu(vec![SollEintrag {
        eintrag: eintrag("echo"),
        werte: Werte::from([("SERVER".to_string(), "vom-server".to_string())]),
    }]);
    let id = EintragId::neu("echo");
    let lokal = BTreeMap::from([(
        id.clone(),
        Werte::from([("PFAD".to_string(), "/tmp/echo".to_string())]),
    )]);
    let geheim = BTreeMap::from([(id.clone(), BTreeSet::from(["TOKEN".to_string()]))]);
    let laeufer = BTreeMap::from([(
        id.clone(),
        Laeufer {
            fingerabdruck: "abc".into(),
            stand: LaeuferStand {
                zustand: EintragZustand::Laeuft { tools: 2 },
                fehlende: Vec::new(),
                tools: vec![werkzeug("echo"), werkzeug("echo_zwei")],
            },
            peer: None,
            abbruch: Abbruch::neu(),
        },
    )]);

    let sicht = eintraege_sicht(&soll, &lokal, &geheim, &laeufer);
    assert_eq!(sicht.len(), 1);
    let eintrag = &sicht[0];
    assert_eq!(eintrag.id, id);
    assert_eq!(eintrag.titel, "Titel echo");
    assert_eq!(
        eintrag
            .felder
            .iter()
            .map(|feld| feld.name.as_str())
            .collect::<Vec<_>>(),
        vec!["PFAD", "TOKEN"],
        "Server-Felder werden am Gerät nicht bearbeitet"
    );
    assert_eq!(
        eintrag.lokale_werte.get("PFAD").map(String::as_str),
        Some("/tmp/echo")
    );
    assert!(
        !eintrag.lokale_werte.contains_key("TOKEN"),
        "Geheimes steht nie im Modell"
    );
    assert_eq!(eintrag.geheim_gesetzt, vec!["TOKEN".to_string()]);
    assert_eq!(
        eintrag.tools,
        vec!["echo".to_string(), "echo_zwei".to_string()],
        "Werkzeugnamen ohne Präfix"
    );
    assert_eq!(eintrag.zustand, EintragZustand::Laeuft { tools: 2 });
}

#[test]
fn sicht_ohne_laeufer_wartet() {
    let soll = Soll::neu(vec![SollEintrag {
        eintrag: eintrag("echo"),
        werte: Werte::new(),
    }]);
    let sicht = eintraege_sicht(&soll, &BTreeMap::new(), &BTreeMap::new(), &BTreeMap::new());
    assert_eq!(sicht.len(), 1);
    assert_eq!(sicht[0].zustand, EintragZustand::Startet);
    assert!(sicht[0].tools.is_empty());
    assert!(sicht[0].lokale_werte.is_empty());
    assert!(sicht[0].geheim_gesetzt.is_empty());
}

#[test]
fn fehlende_werte_stehen_in_der_sicht() {
    let soll = Soll::neu(vec![SollEintrag {
        eintrag: eintrag("echo"),
        werte: Werte::new(),
    }]);
    let id = EintragId::neu("echo");
    let laeufer = BTreeMap::from([(
        id.clone(),
        Laeufer {
            fingerabdruck: "abc".into(),
            stand: LaeuferStand {
                zustand: EintragZustand::WartetAufWerte {
                    felder: vec!["TOKEN".into()],
                },
                fehlende: vec![FehlendesFeld {
                    name: "TOKEN".into(),
                    beschreibung: "Zugangstoken".into(),
                    quelle: Quelle::Geheim,
                }],
                tools: Vec::new(),
            },
            peer: None,
            abbruch: Abbruch::neu(),
        },
    )]);
    let sicht = eintraege_sicht(&soll, &BTreeMap::new(), &BTreeMap::new(), &laeufer);
    assert_eq!(
        sicht[0].fehlende,
        vec![FehlendesFeld {
            name: "TOKEN".into(),
            beschreibung: "Zugangstoken".into(),
            quelle: Quelle::Geheim,
        }]
    );
}

#[test]
fn agent_wird_ueber_den_token_hash_gefunden() {
    let token = hilfen::token_erzeugen();
    let agenten = vec![
        Agent {
            id: "a1".into(),
            name: "pi".into(),
            token_hash: hilfen::hash("anderes-token"),
        },
        Agent {
            id: "a2".into(),
            name: "hermes".into(),
            token_hash: hilfen::hash(&token),
        },
    ];
    assert_eq!(agent_fuer(&agenten, &token), Some("a2".into()));
    assert_eq!(agent_fuer(&agenten, "falsch"), None);
    assert_eq!(agent_fuer(&[], &token), None);
}
