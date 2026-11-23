// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Oberfläche ohne Server und ohne Laufzeit (AGENTS.md §4): ein
//! Beispielmodell hinein, über AccessKit bedienen (Rollen und Namen müssen
//! stimmen, sonst findet der Test nichts), die entstandenen Aktionen prüfen.
//!
//! Mit `QUIVER_BILDER=<verzeichnis>` schreibt der Lauf außerdem die
//! Bildschirmfotos für README und Doku — erzeugt, nicht von Hand abgelegt.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use egui_kittest::kittest::{By, NodeT, Queryable};
use egui_kittest::Harness;
use quiver_client::{AgentSicht, EintragSicht, Modell, Verbindung};
use quiver_domain::{EintragId, FehlendesFeld, Feld, Quelle, Werte};
use quiver_fenster::app::{Aktion, Fenster, KernZugang};
use quiver_fenster::egui::accesskit::Role;
use quiver_fenster::egui::CursorIcon;
use quiver_fenster::modell::{Dialog, Meldung, Seite, Sicht};
use quiver_fenster::profil::Einstellungen;
use quiver_fenster::thema::Thema;
use quiver_protocol::EintragZustand;

/// Das Token, das die Attrappe beim Koppeln zurückgibt.
const TOKEN: &str = "qvr_7c1f0b2e4d5a";
const ENDPUNKT: &str = "http://127.0.0.1:8741/mcp";

/// Der Kern im Test: merkt sich Aktionen, gibt ein festes Modell zurück.
#[derive(Clone)]
struct Attrappe {
    aktionen: Rc<RefCell<Vec<Aktion>>>,
    modell: Rc<RefCell<Modell>>,
    meldungen: Rc<RefCell<Vec<Meldung>>>,
    beendet: Rc<Cell<bool>>,
}

impl Attrappe {
    fn neu(modell: Modell) -> Self {
        Self {
            aktionen: Rc::default(),
            modell: Rc::new(RefCell::new(modell)),
            meldungen: Rc::default(),
            beendet: Rc::default(),
        }
    }
}

impl KernZugang for Attrappe {
    fn ausfuehren(&self, aktion: Aktion) {
        if let Aktion::AgentKoppeln { name } = &aktion {
            self.meldungen.borrow_mut().push(Meldung::Token {
                name: name.clone(),
                token: TOKEN.to_string(),
            });
        }
        self.aktionen.borrow_mut().push(aktion);
    }

    fn modell(&self) -> Modell {
        self.modell.borrow().clone()
    }

    fn meldungen(&self) -> Vec<Meldung> {
        std::mem::take(&mut *self.meldungen.borrow_mut())
    }

    fn beenden(&self) {
        self.beendet.set(true);
    }
}

type Stand = (Fenster<Attrappe>, Attrappe);

fn feld(name: &str, quelle: Quelle, beschreibung: &str) -> Feld {
    Feld {
        name: name.to_string(),
        beschreibung: beschreibung.to_string(),
        quelle,
        pflicht: true,
        standard: None,
    }
}

fn eintrag(
    id: &str,
    titel: &str,
    beschreibung: &str,
    zustand: EintragZustand,
    felder: Vec<Feld>,
) -> EintragSicht {
    EintragSicht {
        id: EintragId::neu(id),
        titel: titel.to_string(),
        beschreibung: beschreibung.to_string(),
        zustand,
        fehlende: Vec::new(),
        felder,
        lokale_werte: Werte::new(),
        geheim_gesetzt: Vec::new(),
        tools: Vec::new(),
    }
}

/// Drei Einträge, drei Zustände — so viele, wie die Seite unterscheiden muss.
fn eintraege() -> Vec<EintragSicht> {
    let mut laeuft = eintrag(
        "echo",
        "Echo",
        "Werkzeuge zum Ausprobieren der Verbindung.",
        EintragZustand::Laeuft { tools: 2 },
        Vec::new(),
    );
    laeuft.tools = vec!["echo".into(), "ping".into()];

    let mut wartet = eintrag(
        "dateien",
        "Dateien",
        "Liest Dateien aus einem Ordner.",
        EintragZustand::WartetAufWerte {
            felder: vec!["PFAD".into()],
        },
        vec![
            feld("PFAD", Quelle::Client, "Ordner, den das Tool lesen darf"),
            feld("SCHLUESSEL", Quelle::Geheim, "Zugangsschlüssel des Servers"),
        ],
    );
    wartet.fehlende = vec![FehlendesFeld {
        name: "PFAD".into(),
        beschreibung: "Ordner, den das Tool lesen darf".into(),
        quelle: Quelle::Client,
    }];
    wartet.geheim_gesetzt = vec!["SCHLUESSEL".into()];

    let fehler = eintrag(
        "graph",
        "Microsoft 365",
        "Kalender und Mail über Graph.",
        EintragZustand::Fehler {
            text: "Der Prozess wurde beendet".into(),
        },
        Vec::new(),
    );

    vec![laeuft, wartet, fehler]
}

fn modell(verbindung: Verbindung, eintraege: Vec<EintragSicht>) -> Modell {
    Modell {
        verbindung,
        eintraege,
        agenten: vec![AgentSicht {
            id: "a1".into(),
            name: "pi".into(),
            verbunden: true,
        }],
        endpunkt: ENDPUNKT.to_string(),
    }
}

fn verbunden() -> Verbindung {
    Verbindung::Verbunden {
        server: "http://quiver.example:8080".into(),
        geraet: "rechner".into(),
    }
}

/// Ein Fenster mit Attrappe. `seite` überschreibt, was `Sicht::neu` aus dem
/// Modell ableitet.
fn fenster(
    modell: Modell,
    seite: Option<Seite>,
    thema: Thema,
    groesse: (f32, f32),
) -> Harness<'static, Stand> {
    let attrappe = Attrappe::neu(modell.clone());
    let mut sicht = Sicht::neu(modell, &Einstellungen::default());
    sicht.thema = thema;
    if let Some(seite) = seite {
        sicht.seite = seite;
    }
    let fenster = Fenster::neu(attrappe.clone(), sicht, None);
    Harness::builder()
        .with_size(groesse)
        .with_step_dt(1.0 / 30.0)
        .with_context_setup(quiver_oberflaeche::einrichten)
        .build_ui_state(|ui, (f, _): &mut Stand| f.bild(ui), (fenster, attrappe))
}

fn gross() -> (f32, f32) {
    (1200.0, 820.0)
}

#[test]
fn startseite_ohne_server_ist_die_verbindung() {
    let mut h = fenster(
        modell(Verbindung::NichtEingebunden, Vec::new()),
        None,
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert_eq!(h.state().0.sicht.seite, Seite::Verbindung);
    // Das Formular steht da: Adresse und Code (zwei Eingabefelder).
    assert_eq!(
        h.query_all(By::new().role(Role::TextInput)).count(),
        2,
        "Server-Adresse und Einmal-Code"
    );
    assert!(h
        .query_by_role_and_label(Role::Button, "Gerät einbinden")
        .is_some());
    // Die Navigation bietet alle drei Seiten an.
    for seite in ["Verbindung", "Tools", "Agenten"] {
        assert!(
            h.query_by_role_and_label(Role::Button, seite).is_some(),
            "{seite} fehlt in der Leiste"
        );
    }
}

#[test]
fn einbinden_schickt_server_und_code() {
    let mut h = fenster(
        modell(Verbindung::NichtEingebunden, Vec::new()),
        Some(Seite::Verbindung),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    let sicht = &mut h.state_mut().0.sicht;
    sicht.eingaben.server = "http://quiver.example:8080".into();
    sicht.eingaben.code = "K7QM-3XPA".into();
    h.run();
    h.get_by_role_and_label(Role::Button, "Gerät einbinden")
        .click();
    h.run();
    assert!(h.state().1.aktionen.borrow().contains(&Aktion::Einbinden {
        server: "http://quiver.example:8080".into(),
        code: "K7QM-3XPA".into(),
    }));
}

#[test]
fn tools_seite_zeigt_drei_karten_mit_drei_zustaenden() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        None,
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert_eq!(
        h.state().0.sicht.seite,
        Seite::Tools,
        "Startseite mit Server"
    );
    for titel in ["Echo", "Dateien", "Microsoft 365"] {
        assert!(h.query_by_label(titel).is_some(), "{titel} fehlt");
    }
    // Die drei Zustände als Abzeichen.
    assert!(h.query_by_label("Läuft · 2 Tools").is_some());
    assert!(h.query_by_label("Braucht Angaben: PFAD").is_some());
    assert!(h.query_by_label("Fehler").is_some());
    // Der Fehlertext steht unter dem Abzeichen.
    assert!(h.query_by_label("Der Prozess wurde beendet").is_some());
}

#[test]
fn braucht_angaben_oeffnet_die_eingaben() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Braucht Angaben")
        .click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Werte {
            titel, eingaben, ..
        } => {
            assert_eq!(titel, "Dateien");
            // Ein Feld je Client- und Geheim-Feld.
            assert_eq!(eingaben.len(), 2);
            assert_eq!(eingaben[0].name, "PFAD");
            assert!(!eingaben[0].geheim);
            assert_eq!(eingaben[1].name, "SCHLUESSEL");
            assert!(eingaben[1].geheim);
            assert!(eingaben[1].gesetzt, "gesetztes Geheimnis erkennen");
        }
        anderer => panic!("kein Angaben-Dialog: {anderer:?}"),
    }
    // Der Dialog steht mit Speichern und Abbrechen da.
    assert!(h.query_by_label("Speichern").is_some());
    assert!(h.query_by_label("Abbrechen").is_some());

    // Ausfüllen und speichern: das Geheimnis bleibt unverändert.
    if let Dialog::Werte { eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
        eingaben[0].wert = "/var/lib/quiver".into();
    }
    h.run();
    h.get_by_role_and_label(Role::Button, "Speichern").click();
    h.run();
    let aktionen = h.state().1.aktionen.borrow();
    let Some(Aktion::WerteSetzen {
        eintrag,
        lokal,
        geheim,
    }) = aktionen.last()
    else {
        panic!("kein WerteSetzen: {aktionen:?}");
    };
    assert_eq!(eintrag.als_str(), "dateien");
    assert_eq!(
        lokal.get("PFAD").map(String::as_str),
        Some("/var/lib/quiver")
    );
    assert!(geheim.is_empty(), "leeres Geheimnis überschreibt nichts");
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

#[test]
fn agent_koppeln_zeigt_token_und_endpunkt() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Agenten),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    // Der Endpunkt steht auf der Seite.
    assert!(h.query_by_label(ENDPUNKT).is_some());
    // Der gekoppelte Agent steht mit Namen da.
    assert!(h.query_by_label("pi").is_some());

    h.get_by_role_and_label(Role::Button, "Agent koppeln")
        .click();
    h.run();
    assert!(matches!(h.state().0.sicht.dialog, Dialog::Koppeln { .. }));

    // Namen tippen: das Feld hat den Fokus schon (erster Bild des Dialogs).
    h.get_by_role(Role::TextInput).type_text("hermes");
    h.run();
    h.get_by_role_and_label(Role::Button, "Koppeln").click();
    h.run();

    match &h.state().0.sicht.dialog {
        Dialog::Token { name, token } => {
            assert_eq!(name, "hermes");
            assert_eq!(token, TOKEN);
        }
        anderer => panic!("kein Token-Dialog: {anderer:?}"),
    }
    let aktionen = h.state().1.aktionen.borrow();
    assert!(aktionen.contains(&Aktion::AgentKoppeln {
        name: "hermes".into()
    }));
    drop(aktionen);

    // Token, Endpunkt und beide Schnipsel stehen im Dialog.
    assert!(h.query_by_label(TOKEN).is_some());
    assert!(h
        .query_by_label("Das Token wird nur einmal angezeigt.")
        .is_some());
    let pi = quiver_fenster::logik::schnipsel_pi(ENDPUNKT, TOKEN);
    assert!(h.query_by_label(&pi).is_some(), "pi-Schnipsel fehlt");
    let json = quiver_fenster::logik::schnipsel_json(ENDPUNKT, TOKEN);
    assert!(h.query_by_label(&json).is_some(), "JSON-Schnipsel fehlt");

    // Schließen räumt den Dialog weg.
    h.get_by_role_and_label(Role::Button, "Schließen").click();
    h.run();
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

#[test]
fn agent_entfernen_fragt_nach() {
    let mut h = fenster(
        modell(verbunden(), Vec::new()),
        Some(Seite::Agenten),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    // Seite und Dialog tragen beide „Entfernen"; erst der auf der Seite.
    h.get_all_by_role_and_label(Role::Button, "Entfernen")
        .next()
        .expect("Knopf Entfernen")
        .click();
    h.run();
    assert!(matches!(
        h.state().0.sicht.dialog,
        Dialog::AgentEntfernen { .. }
    ));
    h.get_all_by_role_and_label(Role::Button, "Entfernen")
        .last()
        .expect("Knopf im Dialog")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::AgentEntfernen { id: "a1".into() }));
}

/// Hand-Cursor auf allem Klickbaren (AGENTS.md §2.12) — nicht nur auf den
/// Knöpfen der Bausteine, sondern auf allem, was die Tools-Seite anbietet.
#[test]
fn hand_cursor_ueber_jedem_knopf_der_toolsseite() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    let anzahl = h.query_all(By::new().role(Role::Button)).count();
    assert!(anzahl >= 8, "nur {anzahl} Knöpfe gefunden");
    for i in 0..anzahl {
        let name = {
            let knoten = h
                .query_all(By::new().role(Role::Button))
                .nth(i)
                .expect("Knopf");
            let name = NodeT::accesskit_node(&knoten)
                .label()
                .unwrap_or_else(|| "(ohne Namen)".to_string());
            knoten.hover();
            name
        };
        h.run();
        assert_eq!(
            h.output().platform_output.cursor_icon,
            CursorIcon::PointingHand,
            "„{name}\": kein Hand-Cursor"
        );
    }
}

/// Wohin die Bildschirmfotos gehen. Ein relativer Pfad gilt ab dem
/// Wurzelverzeichnis des Repos — so tut `QUIVER_BILDER=docs/bilder` überall
/// dasselbe, auch wenn cargo im Paketordner testet.
fn bilder_ordner() -> Option<PathBuf> {
    Some(
        match std::env::var_os("QUIVER_BILDER").map(PathBuf::from)? {
            pfad if pfad.is_absolute() => pfad,
            pfad => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(pfad),
        },
    )
}

/// Alle Seiten in allen drei Themes: nichts stürzt ab, jede Seite hat ihren
/// Inhalt. Mit `QUIVER_BILDER` entstehen dabei die Bildschirmfotos
/// (`fenster-{dark,light}-{tools,agenten,verbindung}.png`).
#[test]
fn alle_seiten_in_allen_themes() {
    let ziel = bilder_ordner();
    if let Some(ziel) = &ziel {
        std::fs::create_dir_all(ziel).expect("Bilderordner");
    }
    for thema in Thema::ALLE {
        for (name, seite) in [
            ("verbindung", Seite::Verbindung),
            ("tools", Seite::Tools),
            ("agenten", Seite::Agenten),
        ] {
            // Die Verbindungsseite zeigt als Startseite das Formular.
            let daten = match seite {
                Seite::Verbindung => modell(Verbindung::NichtEingebunden, Vec::new()),
                _ => modell(verbunden(), eintraege()),
            };
            let mut h = fenster(daten, Some(seite), thema, gross());
            h.run();
            assert!(
                h.query_by_label("mcp-quiver").is_some(),
                "{name}/{}: Kopf fehlt",
                thema.name()
            );
            let titel = quiver_fenster::texte::t(
                quiver_fenster::texte::Sprache::De,
                seite.text_schluessel(),
            );
            assert!(
                h.query_by_role_and_label(Role::Button, &titel).is_some(),
                "{name}: Navigation fehlt"
            );
            if let Some(ziel) = &ziel {
                // Nur die beiden Themes, die die Doku zeigt.
                if matches!(thema, Thema::Dunkel | Thema::Hell) {
                    let bild = h.render().expect("zeichnen (wgpu)");
                    bild.save(ziel.join(format!("fenster-{}-{name}.png", thema.name())))
                        .expect("Bild schreibbar");
                }
            }
        }
    }
}

#[test]
fn werkzeugliste_klappt_auf_und_zu() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    // Zugeklappt: die Namen stehen nicht da.
    assert!(h.query_by_label("echo").is_none());
    h.get_by_role_and_label(Role::Button, "2 Tools anzeigen")
        .click();
    h.run();
    assert!(h.query_by_label("echo").is_some(), "Werkzeug fehlt");
    assert!(h.query_by_label("ping").is_some(), "Werkzeug fehlt");
    h.get_by_role_and_label(Role::Button, "Tools ausblenden")
        .click();
    h.run();
    assert!(h.query_by_label("echo").is_none(), "wieder zugeklappt");
}

#[test]
fn ohne_eintraege_steht_die_leere_flaeche() {
    let mut h = fenster(
        modell(verbunden(), Vec::new()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h.query_by_label("Noch keine Tools").is_some());
    assert!(h
        .query_by_label("Sobald ein Admin dir Tools zuweist, erscheinen sie hier.")
        .is_some());
}

#[test]
fn getrennt_zeigt_den_fehlertext() {
    let mut h = fenster(
        modell(
            Verbindung::Getrennt {
                server: "http://quiver.example:8080".into(),
                fehler: "Der Server antwortet nicht".into(),
            },
            Vec::new(),
        ),
        Some(Seite::Verbindung),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h.query_by_label("Der Server antwortet nicht").is_some());
    // Abmelden ist trotzdem möglich.
    h.get_by_role_and_label(Role::Button, "Gerät abmelden")
        .click();
    h.run();
    assert!(h.state().1.aktionen.borrow().contains(&Aktion::Abmelden));
}

fn temp_ordner() -> PathBuf {
    let stempel = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Systemzeit nach 1970")
        .as_nanos();
    let pfad = std::env::temp_dir().join(format!(
        "quiver-fenster-test-{}-{stempel}",
        std::process::id()
    ));
    std::fs::create_dir_all(&pfad).expect("Ordner");
    pfad
}

#[test]
fn theme_und_sprache_landen_im_profil() {
    let ordner = temp_ordner();
    let daten = modell(verbunden(), eintraege());
    let attrappe = Attrappe::neu(daten.clone());
    let sicht = Sicht::neu(daten, &Einstellungen::default());
    let fenster = Fenster::neu(attrappe.clone(), sicht, Some(ordner.clone()));
    let mut h = Harness::builder()
        .with_size(gross())
        .with_step_dt(1.0 / 30.0)
        .with_context_setup(quiver_oberflaeche::einrichten)
        .build_ui_state(|ui, (f, _): &mut Stand| f.bild(ui), (fenster, attrappe));
    h.run();
    // Der erste Bild schreibt den Standard.
    assert_eq!(Einstellungen::laden(&ordner).thema(), Thema::Dunkel);
    h.get_by_role_and_label(Role::RadioButton, "Mono").click();
    h.run();
    h.get_by_role_and_label(Role::RadioButton, "English")
        .click();
    h.run();
    let gespeichert = Einstellungen::laden(&ordner);
    assert_eq!(gespeichert.thema(), Thema::Mono);
    assert_eq!(gespeichert.sprache(), quiver_fenster::texte::Sprache::En);
    std::fs::remove_dir_all(&ordner).ok();
}

#[test]
fn fenster_zu_haelt_den_kern_an() {
    let daten = modell(verbunden(), eintraege());
    let attrappe = Attrappe::neu(daten.clone());
    let sicht = Sicht::neu(daten, &Einstellungen::default());
    let mut fenster = Fenster::neu(attrappe.clone(), sicht, None);
    assert!(!attrappe.beendet.get());
    eframe::App::on_exit(&mut fenster, None);
    assert!(attrappe.beendet.get(), "Kern nicht angehalten");
}
