// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Unit-Tests für die Modell-Ableitung und die Agent-Token-Prüfung.

use std::collections::{BTreeMap, BTreeSet};

use quiver_domain::{
    Aktion, Eintrag, EintragId, Feld, FeldArt, Herkunft, Laufzeit, Mehrfach, Quelle, Soll,
    SollEintrag, Werte, Zugang,
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
        link: None,
        ..Default::default()
    }
}

fn eintrag(id: &str) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "1.0.0".into(),
        titel: format!("Titel {id}"),
        beschreibung: format!("Beschreibung {id}"),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
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

/// Was der Client lokal speichert, in einem Bündel – die Tests brauchen meist
/// nur wenige davon.
fn lokales<'a>(
    werte: &'a BTreeMap<EintragId, Werte>,
    geheim_gesetzt: &'a BTreeMap<EintragId, BTreeSet<String>>,
    zugaenge: &'a BTreeMap<EintragId, BTreeMap<String, Werte>>,
    zugang_geheim: &'a BTreeMap<EintragId, BTreeMap<String, Werte>>,
) -> Lokales<'a> {
    Lokales {
        werte,
        geheim_gesetzt,
        zugaenge,
        zugang_geheim,
        am_geraet_aus: laufend_aus(),
    }
}

/// Keine Einträge sind auf dem Gerät abgeschaltet.
fn laufend_aus() -> &'static BTreeSet<EintragId> {
    static LEER: std::sync::OnceLock<BTreeSet<EintragId>> = std::sync::OnceLock::new();
    LEER.get_or_init(BTreeSet::new)
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
fn sicht_reicht_art_und_titel_durch() {
    let mut eintrag = eintrag("echo");
    eintrag.felder[1].art = FeldArt::Schalter;
    eintrag.felder[1].titel = Some("Pfad prüfen".into());
    let soll = Soll::neu(vec![SollEintrag {
        eintrag,
        werte: Werte::new(),
    }]);
    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let feld = sicht[0]
        .felder
        .iter()
        .find(|feld| feld.name == "PFAD")
        .expect("PFAD");
    assert_eq!(feld.art, FeldArt::Schalter);
    assert_eq!(feld.titel.as_deref(), Some("Pfad prüfen"));
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
            generation: 1,
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

    let sicht = eintraege_sicht(
        &soll,
        &lokales(&lokal, &geheim, &BTreeMap::new(), &BTreeMap::new()),
        &laeufer,
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
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

/// Ein Eintrag mit Zugangsliste (Aufgabe 14).
fn eintrag_mit_zugaengen(id: &str) -> Eintrag {
    let mut eintrag = eintrag(id);
    eintrag.mehrfach = Some(Mehrfach {
        titel: "Verbindungen".into(),
        einzahl: "Verbindung".into(),
        felder: vec![
            feld("benutzer", Quelle::Client, true),
            feld("host", Quelle::Client, true),
            feld("passwort", Quelle::Geheim, false),
        ],
    });
    eintrag
}

#[test]
fn zugaenge_stehen_in_der_sicht_ohne_geheimes() {
    let soll = Soll::neu(vec![SollEintrag {
        eintrag: eintrag_mit_zugaengen("ssh"),
        werte: Werte::new(),
    }]);
    let id = EintragId::neu("ssh");
    let zugaenge = BTreeMap::from([(
        id.clone(),
        BTreeMap::from([
            (
                "web-01".to_string(),
                Werte::from([
                    ("benutzer".to_string(), "root".to_string()),
                    ("host".to_string(), "web-01.example".to_string()),
                ]),
            ),
            ("db-01".to_string(), Werte::new()),
        ]),
    )]);
    let zugang_geheim = BTreeMap::from([(
        id.clone(),
        BTreeMap::from([(
            "web-01".to_string(),
            Werte::from([("passwort".to_string(), "geheim".to_string())]),
        )]),
    )]);

    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &zugaenge,
            &zugang_geheim,
        ),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let zugaenge = sicht[0].zugaenge.as_ref().expect("Zugangsliste");
    assert_eq!(zugaenge.titel, "Verbindungen");
    assert_eq!(zugaenge.einzahl, "Verbindung");
    assert_eq!(zugaenge.felder.len(), 3, "alle Felder der Liste");
    assert_eq!(
        zugaenge
            .liste
            .iter()
            .map(|zugang| zugang.name.as_str())
            .collect::<Vec<_>>(),
        vec!["db-01", "web-01"],
        "nach Namen sortiert"
    );
    let web = &zugaenge.liste[1];
    assert_eq!(
        web.werte.get("host").map(String::as_str),
        Some("web-01.example")
    );
    assert!(
        !web.werte.contains_key("passwort"),
        "Geheimes steht nie im Modell"
    );
    assert_eq!(web.geheim_gesetzt, vec!["passwort".to_string()]);
    assert!(zugaenge.liste[0].geheim_gesetzt.is_empty());
    // Ein Eintrag ohne `mehrfach` hat keine Zugangsliste.
    let ohne = Soll::neu(vec![SollEintrag {
        eintrag: eintrag("echo"),
        werte: Werte::new(),
    }]);
    assert!(eintraege_sicht(
        &ohne,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new()
        ),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )[0]
    .zugaenge
    .is_none());
}

#[test]
fn zugaenge_gehen_als_json_in_die_umgebung() {
    let mehrfach = Mehrfach {
        titel: "Verbindungen".into(),
        einzahl: "Verbindung".into(),
        felder: vec![
            feld("benutzer", Quelle::Client, true),
            feld("host", Quelle::Client, true),
            feld("passwort", Quelle::Geheim, false),
        ],
    };
    let zugaenge = BTreeMap::from([
        (
            "web-01".to_string(),
            Werte::from([
                ("benutzer".to_string(), "root".to_string()),
                ("host".to_string(), "web-01.example".to_string()),
                // Ein Wert zu einem Feld, das die Liste nicht kennt, zählt nicht.
                ("fremd".to_string(), "weg".to_string()),
            ]),
        ),
        ("db-01".to_string(), Werte::new()),
    ]);
    let geheim = BTreeMap::from([(
        "web-01".to_string(),
        Werte::from([("passwort".to_string(), "geheim".to_string())]),
    )]);
    let json = zugaenge_json(&mehrfach, &zugaenge, &geheim);
    let liste: Vec<Zugang> = serde_json::from_str(&json).expect("JSON");
    assert_eq!(liste.len(), 2);
    assert_eq!(liste[0].name, "db-01");
    assert!(
        liste[0].werte.is_empty(),
        "ein Zugang ohne Werte bleibt leer"
    );
    assert_eq!(
        liste[1].werte,
        Werte::from([
            ("benutzer".to_string(), "root".to_string()),
            ("host".to_string(), "web-01.example".to_string()),
            ("passwort".to_string(), "geheim".to_string()),
        ]),
        "Geheimes ist beim Start eingesetzt, fremde Felder fallen weg"
    );
    // Ohne Zugang eine leere Liste – der Eintrag läuft trotzdem.
    assert_eq!(
        zugaenge_json(&mehrfach, &BTreeMap::new(), &BTreeMap::new()),
        "[]"
    );
}

#[test]
fn sicht_ohne_laeufer_wartet() {
    let soll = Soll::neu(vec![SollEintrag {
        eintrag: eintrag("echo"),
        werte: Werte::new(),
    }]);
    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
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
            generation: 1,
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
    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        &laeufer,
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
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

#[test]
fn jeder_agent_nur_einmal() {
    let agenten = vec![
        Agent {
            id: "a1".into(),
            name: "pi".into(),
            token_hash: "00".into(),
        },
        Agent {
            id: "a2".into(),
            name: "Claude Code".into(),
            token_hash: "01".into(),
        },
    ];
    assert!(name_vergeben(&agenten, "pi"), "gleicher Name");
    assert!(name_vergeben(&agenten, "  PI  "), "Groß und Leerzeichen");
    assert!(name_vergeben(&agenten, "claude code"), "ohne Groß/Klein");
    assert!(!name_vergeben(&agenten, "hermes"), "neuer Name");
    assert!(
        !name_vergeben(&agenten, "   "),
        "der leere Name wird vorher abgefangen"
    );
    assert!(!name_vergeben(&[], "pi"), "noch keiner gekoppelt");
}

#[test]
fn anmeldung_steht_nur_bei_eintraegen_die_eine_brauchen() {
    let mut mit = eintrag("postfach");
    mit.anmeldung = Some(quiver_domain::Anmeldung {
        anmelden: vec!["anmelden".into()],
        status: vec!["status".into()],
        abmelden: vec!["abmelden".into()],
    });
    let soll = Soll::neu(vec![
        SollEintrag {
            eintrag: mit,
            werte: Werte::new(),
        },
        SollEintrag {
            eintrag: eintrag("echo"),
            werte: Werte::new(),
        },
    ]);
    let id = EintragId::neu("postfach");
    let anmeldung = BTreeMap::from([(
        id.clone(),
        AnmeldeZustand::Angemeldet {
            konto: Some("test@beispiel.de".into()),
        },
    )]);
    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        &BTreeMap::new(),
        &anmeldung,
        &BTreeMap::new(),
    );
    let postfach = sicht.iter().find(|e| e.id == id).expect("postfach");
    assert_eq!(
        postfach.anmeldung,
        Some(AnmeldeZustand::Angemeldet {
            konto: Some("test@beispiel.de".into())
        })
    );
    let echo = sicht
        .iter()
        .find(|e| e.id == EintragId::neu("echo"))
        .expect("echo");
    assert_eq!(
        echo.anmeldung, None,
        "ohne Anmeldung im Eintrag bleibt das Feld leer"
    );

    // Noch nicht geprüft heißt „unbekannt" – nicht „abgemeldet".
    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let postfach = sicht.iter().find(|e| e.id == id).expect("postfach");
    assert_eq!(postfach.anmeldung, Some(AnmeldeZustand::Unbekannt));
}

#[test]
fn einrichtung_steht_nur_bei_eintraegen_die_eine_anbieten() {
    let mut mit = eintrag("outline");
    mit.einrichtung = Some(quiver_domain::Einrichtung {
        einrichten: vec!["einrichten".into()],
    });
    let soll = Soll::neu(vec![
        SollEintrag {
            eintrag: mit,
            werte: Werte::new(),
        },
        SollEintrag {
            eintrag: eintrag("echo"),
            werte: Werte::new(),
        },
    ]);
    let id = EintragId::neu("outline");

    // Ohne eigenen Zustand ist die Einrichtung bereit – nicht „läuft“.
    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let outline = sicht.iter().find(|e| e.id == id).expect("outline");
    assert_eq!(outline.einrichtung, Some(EinrichtZustand::Bereit));
    let echo = sicht
        .iter()
        .find(|e| e.id == EintragId::neu("echo"))
        .expect("echo");
    assert_eq!(
        echo.einrichtung, None,
        "ohne Einrichtung im Eintrag bleibt das Feld leer"
    );

    // Ein gemeldeter Zustand steht genau so in der Sicht.
    let zustaende = BTreeMap::from([(
        id.clone(),
        EinrichtZustand::Fehler {
            text: "Die Einrichtung endet mit Code 3".into(),
        },
    )]);
    let sicht = eintraege_sicht(
        &soll,
        &lokales(
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        ),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &zustaende,
    );
    let outline = sicht.iter().find(|e| e.id == id).expect("outline");
    assert_eq!(
        outline.einrichtung,
        Some(EinrichtZustand::Fehler {
            text: "Die Einrichtung endet mit Code 3".into()
        })
    );
}

#[test]
fn ein_zweiter_abgleich_startet_denselben_eintrag_nicht_noch_einmal() {
    let soll = Soll::neu(vec![SollEintrag {
        eintrag: eintrag("echo"),
        werte: Werte::new(),
    }]);
    let id = EintragId::neu("echo");
    let gleich = BTreeMap::from([(id.clone(), soll.eintraege[0].fingerabdruck())]);
    let anders = BTreeMap::from([(id.clone(), "aelterer-fingerabdruck".to_string())]);

    // Starten: läuft der Eintrag schon (egal mit welchem Fingerabdruck), gibt
    // es keine zweite Start-Aufgabe.
    assert!(
        !ueberholt(&Aktion::Starten(id.clone()), &BTreeMap::new(), &soll),
        "noch nicht gestartet: der Start ist nötig"
    );
    assert!(
        ueberholt(&Aktion::Starten(id.clone()), &anders, &soll),
        "läuft schon: kein zweiter Start"
    );

    // Neustarten: gleicher Fingerabdruck wie der laufende heißt „nichts zu tun“.
    assert!(
        ueberholt(&Aktion::Neustarten(id.clone()), &gleich, &soll),
        "gleicher Fingerabdruck: nicht neu starten"
    );
    assert!(
        !ueberholt(&Aktion::Neustarten(id.clone()), &anders, &soll),
        "anderer Fingerabdruck: neu starten"
    );

    // Stoppen: was schon weg ist, muss nicht gestoppt werden.
    assert!(ueberholt(
        &Aktion::Stoppen(id.clone()),
        &BTreeMap::new(),
        &soll
    ));
    assert!(!ueberholt(&Aktion::Stoppen(id.clone()), &gleich, &soll));
}

#[test]
fn ein_schema_ohne_typ_wird_zum_objekt() {
    let leer = std::sync::Arc::new(JsonObject::new());
    let neu = crate::schema_mit_typ(&leer);
    assert_eq!(neu.get("type"), Some(&serde_json::json!("object")));
    assert_eq!(neu.get("properties"), Some(&serde_json::json!({})));

    // Vorhandene Schemas bleiben, wie sie sind.
    let werkzeug = werkzeug("lesen");
    let gleich = crate::schema_mit_typ(&werkzeug.input_schema);
    assert_eq!(*gleich, *werkzeug.input_schema);
}

/// Ein abgeschalteter Eintrag zeigt den eigenen Zustand, keine Werkzeuge und
/// die Wahl am Gerät (Aufgabe 19).
#[test]
fn ein_abgeschalteter_eintrag_zeigt_abgeschaltet() {
    let soll = Soll::neu(vec![SollEintrag {
        eintrag: eintrag("echo"),
        werte: Werte::new(),
    }]);
    let id = EintragId::neu("echo");
    let laeufer = BTreeMap::from([(
        id.clone(),
        Laeufer {
            generation: 1,
            fingerabdruck: "abc".into(),
            stand: LaeuferStand {
                zustand: EintragZustand::Laeuft { tools: 1 },
                fehlende: Vec::new(),
                tools: vec![werkzeug("echo")],
            },
            peer: None,
            abbruch: Abbruch::neu(),
        },
    )]);
    let aus = BTreeSet::from([id.clone()]);
    let sicht = eintraege_sicht(
        &soll,
        &Lokales {
            werte: &BTreeMap::new(),
            geheim_gesetzt: &BTreeMap::new(),
            zugaenge: &BTreeMap::new(),
            zugang_geheim: &BTreeMap::new(),
            am_geraet_aus: &aus,
        },
        &laeufer,
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    assert_eq!(sicht.len(), 1);
    assert!(sicht[0].am_geraet_aus, "die Wahl steht im Modell");
    assert_eq!(sicht[0].zustand, EintragZustand::AmGeraetAus);
    assert!(
        sicht[0].tools.is_empty(),
        "abgeschaltet bietet keine Werkzeuge an: {:?}",
        sicht[0].tools
    );
}

/// Die Sicherung nimmt die Wahl „abgeschaltet“ mit (Aufgabe 19).
#[test]
fn die_sicherung_nimmt_die_abschaltung_mit() {
    let id = EintragId::neu("echo");
    let stand = Stand {
        abgelehnt: false,
        anmelde_grund: None,
        server_url: None,
        geraet: None,
        token: None,
        benutzer: None,
        anmeldewege: None,
        anmeldung_abbruch: None,
        lokale_werte: BTreeMap::new(),
        geheim: BTreeMap::new(),
        zugaenge: BTreeMap::new(),
        zugang_geheim: BTreeMap::new(),
        agenten: Vec::new(),
        soll: Soll::neu(Vec::new()),
        laeufer: BTreeMap::new(),
        anmeldung: BTreeMap::new(),
        einrichtung: BTreeMap::new(),
        einrichtung_laufend: BTreeSet::new(),
        einrichtung_abbruch: BTreeMap::new(),
        aufruf_fristen: BTreeMap::new(),
        tresor_fehler: None,
        profil_hinweis: None,
        am_geraet_aus: BTreeSet::from([id.clone()]),
        angenommen: false,
        protokoll_fuer_agenten: false,
        verbindung: Verbindung::NichtEingebunden,
        endpunkt: String::new(),
    };
    let (eintraege, geheim) = export_daten(&stand, false);
    assert!(geheim.is_none());
    let teil = eintraege
        .get("echo")
        .expect("der Eintrag steht in der Datei");
    assert!(teil.am_geraet_aus, "die Wahl gehört in die Sicherung");
    // Aus bleibt aus – auch gegen eine Datei, die „an“ sagt.
    assert!(einstellungen::aus_mischen(true, false));
    assert!(einstellungen::aus_mischen(false, true));
    assert!(!einstellungen::aus_mischen(false, false));
}

/// `https` überall, `http` nur auf Loopback (ADR 0002). Ein Pfad, eine
/// Abfrage und ein Benutzer in der Adresse sind Fehler.
#[test]
fn server_adresse_nur_https_oder_loopback() {
    assert_eq!(
        server_adresse(" https://quiver.example/ ").expect("https"),
        "https://quiver.example"
    );
    assert_eq!(
        server_adresse("http://127.0.0.1:8080").expect("Loopback"),
        "http://127.0.0.1:8080"
    );
    assert_eq!(
        server_adresse("http://localhost:8080").expect("localhost"),
        "http://localhost:8080"
    );
    assert!(server_adresse("http://[::1]:8080").is_ok(), "IPv6-Loopback");
    for falsch in [
        "",
        "http://quiver.example",
        "ftp://quiver.example",
        "https://user:geheim@quiver.example",
        "https://quiver.example/pfad",
        "https://quiver.example?frage=1",
        "https://quiver.example#fragment",
    ] {
        assert!(
            server_adresse(falsch).is_err(),
            "„{falsch}“ muss abgelehnt werden"
        );
    }
}

/// Ein unbekannter Grund wird wie `neu-anmelden` behandelt: vorsichtig.
#[test]
fn grund_text_ist_verstaendlich() {
    assert!(grund_text(Some(AnmeldeGrund::Gesperrt), "technisch").contains("gesperrt"));
    assert!(grund_text(Some(AnmeldeGrund::KeinZugriff), "technisch").contains("Gruppe"));
    assert!(grund_text(Some(AnmeldeGrund::NeuAnmelden), "technisch").contains("erneut"));
    assert_eq!(grund_text(None, "technisch"), "technisch");
}

/// Eine Kennung, die nicht als Präfix taugt, steht in der Sicht mit Grund –
/// der Eintrag startet nicht (Paket 3).
#[test]
fn eine_unbrauchbare_kennung_steht_mit_grund_in_der_sicht() {
    let soll = Soll::neu(vec![
        SollEintrag {
            eintrag: eintrag("quiver"),
            werte: Werte::new(),
        },
        SollEintrag {
            eintrag: eintrag("echo"),
            werte: Werte::new(),
        },
    ]);
    let werte = BTreeMap::new();
    let geheim = BTreeMap::new();
    let zugaenge = BTreeMap::new();
    let leer = BTreeSet::new();
    let sicht = eintraege_sicht(
        &soll,
        &Lokales {
            werte: &werte,
            geheim_gesetzt: &geheim,
            zugaenge: &zugaenge,
            zugang_geheim: &zugaenge,
            am_geraet_aus: &leer,
        },
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let kaputt = sicht
        .iter()
        .find(|eintrag| eintrag.id == EintragId::neu("quiver"))
        .expect("der Eintrag steht in der Sicht");
    let grund = kaputt.unzulaessig.clone().expect("Grund");
    assert!(grund.contains("reserviert"), "{grund}");
    let gut = sicht
        .iter()
        .find(|eintrag| eintrag.id == EintragId::neu("echo"))
        .expect("echo");
    assert_eq!(gut.unzulaessig, None);
    // Ein unbrauchbarer Eintrag läuft nie – auch nicht, wenn er an wäre.
    let stand = stand_mit(&soll, &leer);
    assert!(!darf_laufen(&stand, &EintragId::neu("quiver")));
    assert!(darf_laufen(&stand, &EintragId::neu("echo")));
}

/// Ein Gerüst für Prüfungen an [`darf_laufen`].
fn stand_mit(soll: &Soll, aus: &BTreeSet<EintragId>) -> Stand {
    Stand {
        abgelehnt: false,
        anmelde_grund: None,
        server_url: None,
        geraet: None,
        token: None,
        benutzer: None,
        anmeldewege: None,
        anmeldung_abbruch: None,
        lokale_werte: BTreeMap::new(),
        geheim: BTreeMap::new(),
        zugaenge: BTreeMap::new(),
        zugang_geheim: BTreeMap::new(),
        agenten: Vec::new(),
        soll: soll.clone(),
        laeufer: BTreeMap::new(),
        anmeldung: BTreeMap::new(),
        einrichtung: BTreeMap::new(),
        einrichtung_laufend: BTreeSet::new(),
        einrichtung_abbruch: BTreeMap::new(),
        aufruf_fristen: BTreeMap::new(),
        tresor_fehler: None,
        profil_hinweis: None,
        am_geraet_aus: aus.clone(),
        angenommen: false,
        protokoll_fuer_agenten: false,
        verbindung: Verbindung::NichtEingebunden,
        endpunkt: String::new(),
    }
}

/// „Verbunden“ eines Agenten folgt allein aus den offenen Sitzungen
/// (Paket 3): ohne Sitzung ist er es nicht, mit Sitzung schon.
#[test]
fn verbunden_folgt_aus_den_offenen_sitzungen() {
    let soll = Soll::neu(Vec::new());
    let aus = BTreeSet::new();
    let stand = Stand {
        agenten: vec![profil::Agent {
            id: "a1".into(),
            name: "pi".into(),
            token_hash: "00".into(),
        }],
        ..stand_mit(&soll, &aus)
    };
    let ohne = stand.modell(&BTreeSet::new());
    assert!(!ohne.agenten[0].verbunden, "ohne Sitzung nicht verbunden");
    let mit = stand.modell(&BTreeSet::from(["a1".to_string()]));
    assert!(mit.agenten[0].verbunden, "mit Sitzung verbunden");
}

/// Sitzungen werden abgeräumt: abgelaufene zuerst, sonst die älteste, und
/// höchstens 64 (Paket 3).
#[test]
fn sitzungen_laufen_ab_und_bleiben_bei_64() {
    let jetzt = std::time::Instant::now();
    let mut zeiten: BTreeMap<u64, std::time::Instant> = BTreeMap::new();
    assert_eq!(sitzung_weicht(&zeiten, jetzt), None, "Platz für die erste");

    // Eine abgelaufene Sitzung weicht zuerst – auch wenn noch Platz wäre.
    zeiten.insert(1, jetzt - SITZUNG_FRIST - Duration::from_secs(1));
    zeiten.insert(2, jetzt);
    assert_eq!(sitzung_weicht(&zeiten, jetzt), Some(1));
    zeiten.remove(&1);

    // Volle Liste ohne abgelaufene: die älteste weicht.
    let mut voll: BTreeMap<u64, std::time::Instant> = BTreeMap::new();
    for nummer in 0..SITZUNGEN_MAX as u64 {
        voll.insert(
            nummer,
            jetzt - Duration::from_secs(SITZUNGEN_MAX as u64 + 10 - nummer),
        );
    }
    let weicht = sitzung_weicht(&voll, jetzt).expect("die älteste weicht");
    assert_eq!(weicht, 0, "die älteste ist die erste");
    // Unter der Grenze bleibt alles.
    voll.remove(&0);
    assert_eq!(sitzung_weicht(&voll, jetzt), None);
}

/// Der zusätzliche Soll-Abgleich läuft alle fünf Minuten (± Streuung).
#[test]
fn die_zusaetzliche_nachfrage_hat_streuung() {
    let abstand = abgleich::nachfrage_abstand_fuer_test();
    assert!(
        abstand >= Duration::from_secs(5 * 60),
        "mindestens fünf Minuten: {abstand:?}"
    );
    assert!(
        abstand <= Duration::from_secs(5 * 60 + 30),
        "höchstens fünf Minuten und die Streuung: {abstand:?}"
    );
}

/// Die Auslastung zählt insgesamt und je Eintrag mit (Paket 3): 8 je Eintrag,
/// 32 insgesamt.
#[test]
fn die_auslastung_zaehlt_gesamt_und_je_eintrag() {
    let mut auslastung = Auslastung::default();
    let eins = EintragId::neu("eins");
    let zwei = EintragId::neu("zwei");
    for _ in 0..Auslastung::EINTRAG_MAX {
        assert!(auslastung.belegen(&eins));
    }
    assert!(
        !auslastung.belegen(&eins),
        "der neunte Aufruf desselben Eintrags ist zu viel"
    );
    // Andere Einträge haben ihren eigenen Anteil.
    assert!(auslastung.belegen(&zwei));

    // Insgesamt höchstens 32: mit 8 + 1 sind noch 23 übrig.
    let mut weitere = 0;
    for nummer in 0..30 {
        if auslastung.belegen(&EintragId::neu(format!("eintrag-{nummer}"))) {
            weitere += 1;
        }
    }
    assert_eq!(
        weitere,
        Auslastung::GESAMT_MAX - Auslastung::EINTRAG_MAX - 1,
        "die Gesamtgrenze greift"
    );
    for _ in 0..Auslastung::EINTRAG_MAX {
        auslastung.freigeben(&eins);
    }
    assert!(auslastung.belegen(&eins), "nach dem Freigeben wieder frei");
}

/// Die Größe einer Antwort wird gezählt, ohne die Antwort zusammenzukleben.
#[test]
fn die_antwortgroesse_zaehlt_ohne_kopie() {
    let ergebnis = rmcp::model::CallToolResult::success(vec![
        rmcp::model::ContentBlock::text("hallo"),
        rmcp::model::ContentBlock::text("welt"),
    ]);
    assert_eq!(endpunkt::antwort_groesse(&ergebnis), 9, "5 + 4 Zeichen");
}

// ------------------------------------------- Werkzeuge (Aufgabe 35, §5 und §6)

/// Ein Eintrag, dessen Programm hier nicht liegt, steht mit `nicht_installiert`
/// in der Sicht — die Karte sagt das dann einmal und bietet keine Anmeldung an.
/// Ein fester Pfad, den es gibt, ist kein solcher Fall.
#[test]
fn ein_fehlendes_programm_steht_in_der_sicht() {
    let mut fehlt = eintrag("fehlt");
    fehlt.laufzeit = Laufzeit::Stdio {
        befehl: "quiver-gibtsnicht-4711".into(),
        argumente: Vec::new(),
        umgebung: Werte::new(),
    };
    let mut fest = eintrag("fest");
    fest.laufzeit = Laufzeit::Stdio {
        befehl: std::env::current_exe()
            .expect("eigenes Programm")
            .to_string_lossy()
            .to_string(),
        argumente: Vec::new(),
        umgebung: Werte::new(),
    };
    // Ein entfernter Eintrag startet nichts: kein Programm, kein Befund.
    let mut entfernt = eintrag("entfernt");
    entfernt.laufzeit = Laufzeit::Remote {
        url: "https://example.org/mcp".into(),
        kopfzeilen: Werte::new(),
    };

    let soll = Soll::neu(vec![
        SollEintrag {
            eintrag: fehlt,
            werte: Werte::new(),
        },
        SollEintrag {
            eintrag: fest,
            werte: Werte::new(),
        },
        SollEintrag {
            eintrag: entfernt,
            werte: Werte::new(),
        },
    ]);
    let werte = BTreeMap::new();
    let geheim = BTreeMap::new();
    let zugaenge = BTreeMap::new();
    let leer = BTreeSet::new();
    let sicht = eintraege_sicht(
        &soll,
        &Lokales {
            werte: &werte,
            geheim_gesetzt: &geheim,
            zugaenge: &zugaenge,
            zugang_geheim: &zugaenge,
            am_geraet_aus: &leer,
        },
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );
    let finde = |id: &str| {
        sicht
            .iter()
            .find(|eintrag| eintrag.id == EintragId::neu(id))
            .expect("der Eintrag steht in der Sicht")
            .clone()
    };
    assert_eq!(
        finde("fehlt").nicht_installiert.as_deref(),
        Some("quiver-gibtsnicht-4711"),
        "der Programmname steht im Befund – kein Pfad"
    );
    assert_eq!(finde("fest").nicht_installiert, None);
    assert_eq!(finde("entfernt").nicht_installiert, None);
}

/// Rechnet ein Eintrag mit dem Paket? Nur ein bloßer Programmname tut das;
/// ein fester Pfad und ein entfernter Eintrag kommen ohne den Ordner `tools`
/// aus (Aufgabe 35, §6).
#[test]
fn nur_ein_blosser_name_braucht_das_paket() {
    let mit = |id: &str, befehl: &str| SollEintrag {
        eintrag: {
            let mut eintrag = eintrag(id);
            eintrag.laufzeit = Laufzeit::Stdio {
                befehl: befehl.into(),
                argumente: Vec::new(),
                umgebung: Werte::new(),
            };
            eintrag
        },
        werte: Werte::new(),
    };
    assert!(braucht_paket(&Soll::neu(vec![mit("a", "quiver-devops")])));
    assert!(braucht_paket(&Soll::neu(vec![
        mit("a", "quiver-devops"),
        mit("b", "quiver-jira")
    ])));
    assert!(!braucht_paket(&Soll::neu(vec![mit(
        "a",
        "/opt/quiver/quiver-devops"
    )])));
    assert!(!braucht_paket(&Soll::neu(vec![mit(
        "a",
        r"C:\Programme\mcp-quiver\tools\quiver-m365.exe"
    )])));
    assert!(!braucht_paket(&Soll::neu(vec![mit("a", "")])));
    let entfernt = SollEintrag {
        eintrag: {
            let mut eintrag = eintrag("c");
            eintrag.laufzeit = Laufzeit::Remote {
                url: "https://example.org/mcp".into(),
                kopfzeilen: Werte::new(),
            };
            eintrag
        },
        werte: Werte::new(),
    };
    assert!(!braucht_paket(&Soll::neu(vec![entfernt])));
}
