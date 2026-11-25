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
use quiver_client::protokoll::{Aufruf, Filter, LogFilter, Status, Zeile};
use quiver_client::{
    AgentSicht, AnmeldeZustand, Anmeldewege, EinrichtZustand, EintragSicht, Modell, Verbindung,
    Vorschau, VorschauArt, VorschauEintrag, Zusammenfassung,
};
use quiver_domain::{EintragId, FehlendesFeld, Feld, FeldArt, FeldLink, FeldOption, Quelle, Werte};
use quiver_fenster::app::{Aktion, Fenster, KernZugang};
use quiver_fenster::egui::accesskit::{Role, Toggled};
use quiver_fenster::egui::{self, CursorIcon, OutputCommand};
use quiver_fenster::modell::{Dialog, Meldung, Seite, Sicht};
use quiver_fenster::profil::Einstellungen;
use quiver_fenster::thema::Thema;
use quiver_protocol::EintragZustand;

/// Das Token, das die Attrappe beim Koppeln zurückgibt.
const TOKEN: &str = "qvr_7c1f0b2e4d5a";
const ENDPUNKT: &str = "http://127.0.0.1:8741/mcp";

/// Drei Aufrufe, drei Ergebnisse — so viele, wie die Protokollseite
/// unterscheiden muss (Aufgabe 17).
fn beispiel_aufrufe() -> Vec<Aufruf> {
    let zeit = quiver_client::protokoll::jetzt();
    let aufruf = |eintrag: &str,
                  tool: &str,
                  agent: &str,
                  status: Status,
                  fehler: Option<&str>,
                  argumente: &str,
                  vorschau: &str| Aufruf {
        zeit: zeit.clone(),
        agent: agent.to_string(),
        eintrag: EintragId::neu(eintrag),
        tool: tool.to_string(),
        dauer_ms: 12,
        status,
        fehler: fehler.map(str::to_string),
        argumente: argumente.to_string(),
        antwort_groesse: vorschau.chars().count(),
        vorschau: vorschau.to_string(),
    };
    vec![
        aufruf(
            "echo",
            "echo__echo",
            "pi",
            Status::Ok,
            None,
            r#"{"text":"hallo"}"#,
            "hallo",
        ),
        aufruf(
            "dateien",
            "dateien__lesen",
            "hermes",
            Status::Fehler,
            Some("Der Prozess wurde beendet"),
            r#"{"pfad":"/tmp"}"#,
            "",
        ),
        aufruf(
            "graph",
            "graph__kalender",
            "pi",
            Status::Abgelehnt,
            Some("Werkzeug graph__kalender wird nicht angeboten"),
            "{}",
            "",
        ),
    ]
}

/// Prozesszeilen der Einträge „Echo“ und „Dateien“ — mit Stufe und einer
/// Fehlerzeile, damit die Tool-Logs alle Fälle zeigen (Aufgabe 18).
fn beispiel_log() -> Vec<Zeile> {
    let zeit = quiver_client::protokoll::jetzt();
    vec![
        Zeile {
            zeit: zeit.clone(),
            eintrag: EintragId::neu("echo"),
            text: "INFO gestartet, Werkzeuge: echo, ping".into(),
        },
        Zeile {
            zeit: zeit.clone(),
            eintrag: EintragId::neu("echo"),
            text: "Aufruf echo {\"text\":\"hallo\"}".into(),
        },
        Zeile {
            zeit: zeit.clone(),
            eintrag: EintragId::neu("echo"),
            text: "WARN ping antwortet langsam".into(),
        },
        Zeile {
            zeit: zeit.clone(),
            eintrag: EintragId::neu("echo"),
            text: "ERROR echo: Verbindung abgebrochen".into(),
        },
        Zeile {
            zeit,
            eintrag: EintragId::neu("dateien"),
            text: "Dateien gestartet".into(),
        },
    ]
}

/// Der Kern im Test: merkt sich Aktionen, gibt ein festes Modell zurück.
#[derive(Clone)]
struct Attrappe {
    aktionen: Rc<RefCell<Vec<Aktion>>>,
    modell: Rc<RefCell<Modell>>,
    meldungen: Rc<RefCell<Vec<Meldung>>>,
    beendet: Rc<Cell<bool>>,
    /// Führt die Sicherung im Test Geheimnisse? Dann wartet der Import-Dialog
    /// auf ein Passwort (Aufgabe 15).
    sicherung_mit_geheimnissen: Rc<Cell<bool>>,
    /// Die Aufrufe und Prozesszeilen, die der Kern im Test liefert
    /// (Aufgabe 17).
    aufrufe: Rc<RefCell<Vec<Aufruf>>>,
    log_zeilen: Rc<RefCell<Vec<Zeile>>>,
    zaehler: Rc<Cell<u64>>,
}

impl Attrappe {
    fn neu(modell: Modell) -> Self {
        Self {
            aktionen: Rc::default(),
            modell: Rc::new(RefCell::new(modell)),
            meldungen: Rc::default(),
            beendet: Rc::default(),
            sicherung_mit_geheimnissen: Rc::default(),
            aufrufe: Rc::new(RefCell::new(beispiel_aufrufe())),
            log_zeilen: Rc::new(RefCell::new(beispiel_log())),
            zaehler: Rc::new(Cell::new(1)),
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
        // Eine geprüfte Sicherung kommt als Vorschau zurück (Aufgabe 15).
        if let Aktion::EinstellungenPruefen { pfad } = &aktion {
            self.meldungen
                .borrow_mut()
                .push(Meldung::EinstellungenVorschau {
                    pfad: pfad.clone(),
                    vorschau: sicherung_vorschau(self.sicherung_mit_geheimnissen.get()),
                });
        }
        if let Aktion::EinstellungenExportieren { .. } = &aktion {
            self.meldungen
                .borrow_mut()
                .push(Meldung::EinstellungenFertig {
                    importiert: false,
                    zusammenfassung: Zusammenfassung {
                        eintraege: 6,
                        werte: 9,
                        zugaenge: 2,
                        geheimnisse: 1,
                        uebersprungen: Vec::new(),
                    },
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

    fn protokoll(&self, filter: &Filter) -> Vec<Aufruf> {
        self.aufrufe
            .borrow()
            .iter()
            .rev()
            .filter(|aufruf| filter.passt(aufruf))
            .cloned()
            .collect()
    }

    fn prozess_log(&self, eintrag: &quiver_domain::EintragId, filter: &LogFilter) -> Vec<Zeile> {
        self.log_zeilen
            .borrow()
            .iter()
            .rev()
            .filter(|zeile| &zeile.eintrag == eintrag && filter.passt(zeile))
            .cloned()
            .collect()
    }

    fn protokoll_stand(&self) -> u64 {
        self.zaehler.get()
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
        link: None,
        ..Default::default()
    }
}

/// Ein Feld mit Art und Titel (Aufgabe 16).
fn feld_mit(name: &str, quelle: Quelle, art: FeldArt, titel: &str) -> Feld {
    let mut feld = feld(name, quelle, "");
    feld.art = art;
    feld.titel = Some(titel.to_string());
    feld
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
        anmeldung: None,
        einrichtung: None,
        zugaenge: None,
        am_geraet_aus: false,
        unzulaessig: None,
    }
}

/// Ein Eintrag wie Outline in der Aufgabe: `url` trägt die Adresse, `api_key`
/// bekommt den Hilfe-Link „Token in Outline erstellen" (Aufgabe 13).
fn link_eintrag() -> EintragSicht {
    let mut schluessel = feld("api_key", Quelle::Geheim, "Token aus Outline");
    schluessel.link = Some(FeldLink {
        text: "Token in Outline erstellen".into(),
        url: "${url}/settings/api-and-access".into(),
    });
    let mut eintrag = eintrag(
        "outline",
        "Outline",
        "Wiki lesen und schreiben.",
        EintragZustand::WartetAufWerte {
            felder: vec!["url".into()],
        },
        vec![feld("url", Quelle::Client, "Adresse der Wiki"), schluessel],
    );
    eintrag.fehlende = vec![FehlendesFeld {
        name: "url".into(),
        beschreibung: "Adresse der Wiki".into(),
        quelle: Quelle::Client,
    }];
    eintrag
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

/// Das Titelbild des Clients in README und Doku: die Tools laufen (grün),
/// **ein** Eintrag braucht noch Angaben, keiner steht auf Fehler. Die
/// Zustände aller drei kommen in [`eintraege`] vor; hier geht es allein um
/// das Bild, das ein neuer Leser zuerst sieht.
///
/// Titel und Beschreibungen sind Inhalt, nicht Oberfläche — im englischen
/// README sollen sie trotzdem englisch sein.
fn titel_eintraege(sprache: quiver_fenster::texte::Sprache) -> Vec<EintragSicht> {
    use quiver_fenster::texte::Sprache;

    let [(echo_titel, echo_text), (notizen_titel, notizen_text), (dateien_titel, dateien_text)] =
        match sprache {
            Sprache::De => [
                ("Echo", "Werkzeuge zum Ausprobieren der Verbindung."),
                ("Notizen", "Notizen lesen, schreiben und durchsuchen."),
                ("Dateien", "Liest Dateien aus einem Ordner."),
            ],
            Sprache::En => [
                ("Echo", "Tools for trying out the connection."),
                ("Notes", "Read, write and search notes."),
                ("Files", "Reads files from a folder."),
            ],
        };
    let pfad_text = match sprache {
        Sprache::De => "Ordner, den das Tool lesen darf",
        Sprache::En => "Folder the tool may read",
    };

    let mut echo = eintrag(
        "echo",
        echo_titel,
        echo_text,
        EintragZustand::Laeuft { tools: 2 },
        Vec::new(),
    );
    echo.tools = vec!["echo".into(), "ping".into()];

    let mut notizen = eintrag(
        "notizen",
        notizen_titel,
        notizen_text,
        EintragZustand::Laeuft { tools: 3 },
        Vec::new(),
    );
    notizen.tools = vec!["lesen".into(), "schreiben".into(), "suchen".into()];

    // Genau einer braucht Angaben — mehr wäre kein gutes Titelbild.
    let mut dateien = eintrag(
        "dateien",
        dateien_titel,
        dateien_text,
        EintragZustand::WartetAufWerte {
            felder: vec!["PFAD".into()],
        },
        vec![feld("PFAD", Quelle::Client, pfad_text)],
    );
    dateien.fehlende = vec![FehlendesFeld {
        name: "PFAD".into(),
        beschreibung: pfad_text.into(),
        quelle: Quelle::Client,
    }];

    vec![echo, notizen, dateien]
}

/// Ein Eintrag, der eine Anmeldung braucht, in einem bestimmten Zustand.
fn anmelde_eintrag(id: &str, titel: &str, anmeldung: AnmeldeZustand) -> EintragSicht {
    let mut eintrag = eintrag(
        id,
        titel,
        "Braucht eine Anmeldung im Browser.",
        EintragZustand::Laeuft { tools: 1 },
        Vec::new(),
    );
    eintrag.tools = vec!["lesen".into()];
    eintrag.anmeldung = Some(anmeldung);
    eintrag
}

/// Vier Einträge, vier Anmeldezustände – wie sie die Karte unterscheiden muss.
fn anmelde_zustaende() -> Vec<EintragSicht> {
    vec![
        anmelde_eintrag("postfach", "Postfach", AnmeldeZustand::Abgemeldet),
        anmelde_eintrag("kalender", "Kalender", AnmeldeZustand::Laeuft),
        anmelde_eintrag(
            "mail",
            "Mail",
            AnmeldeZustand::Angemeldet {
                konto: Some("test@beispiel.de".into()),
            },
        ),
        anmelde_eintrag(
            "freigaben",
            "Freigaben",
            AnmeldeZustand::Fehler {
                text: "Der Status endet mit Code 3".into(),
            },
        ),
    ]
}

/// Ein Eintrag mit Einrichtung, in einem bestimmten Zustand (Aufgabe 11).
fn einricht_eintrag(id: &str, titel: &str, einrichtung: EinrichtZustand) -> EintragSicht {
    let mut eintrag = eintrag(
        id,
        titel,
        "Verwaltet Zugänge in Profilen.",
        EintragZustand::Laeuft { tools: 1 },
        Vec::new(),
    );
    eintrag.tools = vec!["lesen".into()];
    eintrag.einrichtung = Some(einrichtung);
    eintrag
}

/// Drei Einträge, drei Einrichtungszustände – wie sie die Karte unterscheiden muss.
fn einricht_zustaende() -> Vec<EintragSicht> {
    vec![
        einricht_eintrag("outline", "Outline", EinrichtZustand::Bereit),
        einricht_eintrag("ssh", "SSH", EinrichtZustand::Laeuft),
        einricht_eintrag(
            "abap",
            "ABAP",
            EinrichtZustand::Fehler {
                text: "Die Einrichtung endet mit Code 3".into(),
            },
        ),
    ]
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
        abgelehnt: false,
        anmelde_grund: None,
        benutzer: None,
        server_url: None,
        anmeldewege: None,
        protokoll_fuer_agenten: false,
        tresor_fehler: None,
        profil_hinweis: None,
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

/// Lässt die Zeichnung zur Ruhe kommen. egui scrollt weich: nach
/// `scroll_to_me` wandert ein Steuerelement noch einige Bilder, und ein Klick
/// auf die alte Stelle träfe daneben.
fn zur_ruhe(h: &mut Harness<'static, Stand>) {
    for _ in 0..6 {
        h.run();
    }
}

/// Auch die neuen Knöpfe der Verbindungsseite tragen den Hand-Cursor
/// (AGENTS.md §2.12): „Weiter“, „Mit Microsoft anmelden“ und „Gerät abmelden“.
#[test]
fn hand_cursor_auf_der_verbindungsseite() {
    let mut h = verbindungsfenster(Verbindung::NichtEingebunden);
    h.run();
    h.state_mut().0.sicht.eingaben.server = "https://quiver.example".into();
    h.run();
    let knopf = h.get_by_role_and_label(Role::Button, "Weiter");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Weiter“"
    );

    let mut h = verbindungsfenster(Verbindung::NichtEingebunden);
    wege_setzen(&mut h, "https://quiver.example", true, false);
    h.run();
    let knopf = h.get_by_role_and_label(Role::Button, "Mit Microsoft anmelden");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Mit Microsoft anmelden“"
    );

    let mut h = verbindungsfenster(Verbindung::Verbunden {
        server: "https://quiver.example".into(),
        geraet: "Testrechner".into(),
    });
    h.run();
    let knopf = h.get_by_role_and_label(Role::Button, "Gerät abmelden");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Gerät abmelden“"
    );
}

/// Die vier Anmeldezustände in allen drei Themes und beiden Sprachen bei der
/// kleinsten Breite (Aufgabe 22; AGENTS.md §5: dark, light, mono / de, en).
#[test]
fn anmeldezustaende_in_allen_themes_und_sprachen() {
    use quiver_fenster::texte::Sprache;
    for thema in Thema::ALLE {
        for (sprache, weiter, browser, erneut) in [
            (
                Sprache::De,
                "Weiter",
                "Im Browser fortfahren …",
                "Erneut anmelden",
            ),
            (
                Sprache::En,
                "Continue",
                "Continue in the browser …",
                "Sign in again",
            ),
        ] {
            for (name, verbindung, label) in [
                ("leer", Verbindung::NichtEingebunden, weiter),
                (
                    "browser",
                    Verbindung::WartetAufBrowser {
                        server: "https://quiver.example".into(),
                        adresse: "https://quiver.example/auth/geraet/start".into(),
                    },
                    browser,
                ),
                (
                    "neu",
                    Verbindung::NeuAnmelden {
                        server: "https://quiver.example".into(),
                        text: "neu".into(),
                    },
                    erneut,
                ),
                (
                    "fehler",
                    Verbindung::AnmeldeFehler {
                        server: "https://quiver.example".into(),
                        text: "gesperrt".into(),
                    },
                    weiter,
                ),
            ] {
                let mut h = fenster(
                    modell(verbindung, Vec::new()),
                    Some(Seite::Verbindung),
                    thema,
                    (560.0, 460.0),
                );
                h.state_mut().0.sicht.sprache = sprache;
                h.run_steps(3);
                nichts_ragt_heraus(&h, 560.0, name);
                assert!(
                    h.query_all_by_label(label).count() >= 1,
                    "{name}/{}/{}: „{label}“ fehlt",
                    thema.name(),
                    sprache.code()
                );
            }
        }
    }
}

fn verbindungsfenster(verbindung: Verbindung) -> Harness<'static, Stand> {
    fenster(
        modell(verbindung, Vec::new()),
        Some(Seite::Verbindung),
        Thema::Dunkel,
        gross(),
    )
}

/// Die Anmeldewege in **beide** Modelle schreiben: in die Sicht und in die
/// Attrappe. Sonst überschreibt der nächste Zug das Modell der Sicht.
fn wege_setzen(h: &mut Harness<'static, Stand>, server: &str, entra: bool, code: bool) {
    let wege = Anmeldewege {
        server: server.to_string(),
        entra,
        code,
    };
    let (fenster, attrappe) = h.state_mut();
    {
        let mut modell = attrappe.modell.borrow_mut();
        modell.anmeldewege = Some(wege.clone());
        modell.server_url = Some(server.to_string());
    }
    fenster.sicht.modell.anmeldewege = Some(wege);
    fenster.sicht.modell.server_url = Some(server.to_string());
    fenster.sicht.eingaben.server = server.to_string();
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
    // Erst **nur** die Adresse — kein Codefeld, bevor der Server gefragt ist.
    assert_eq!(
        h.query_all(By::new().role(Role::TextInput)).count(),
        1,
        "nur die Server-Adresse"
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "Weiter").is_some(),
        "„Weiter“ fehlt"
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "Gerät einbinden")
            .is_none(),
        "der Code kommt erst nach „Weiter“"
    );
    // Die Navigation bietet alle drei Seiten an.
    for seite in ["Verbindung", "Tools", "Agenten"] {
        assert!(
            h.query_by_role_and_label(Role::Button, seite).is_some(),
            "{seite} fehlt in der Leiste"
        );
    }
}

#[test]
fn weiter_fragt_die_anmeldewege() {
    let mut h = verbindungsfenster(Verbindung::NichtEingebunden);
    h.run();
    h.state_mut().0.sicht.eingaben.server = "http://quiver.example:8080".into();
    h.run();
    h.get_by_role_and_label(Role::Button, "Weiter").click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::Anmeldewege {
                server: "http://quiver.example:8080".into(),
            }),
        "Weiter fragt den Server: {:?}",
        h.state().1.aktionen.borrow()
    );
}

#[test]
fn mit_entra_gibt_es_nur_microsoft_und_kein_codefeld() {
    let mut h = verbindungsfenster(Verbindung::NichtEingebunden);
    wege_setzen(&mut h, "https://quiver.example", true, false);
    h.run();
    assert_eq!(
        h.query_all(By::new().role(Role::TextInput)).count(),
        1,
        "nur die Adresse, kein Codefeld"
    );
    assert!(h
        .query_by_role_and_label(Role::Button, "Mit Microsoft anmelden")
        .is_some());
    h.get_by_role_and_label(Role::Button, "Mit Microsoft anmelden")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::AnmeldenMicrosoft {
            server: "https://quiver.example".into(),
        }));
}

#[test]
fn ohne_entra_kommt_das_codefeld_und_das_einbinden() {
    let mut h = verbindungsfenster(Verbindung::NichtEingebunden);
    wege_setzen(&mut h, "http://127.0.0.1:8080", false, true);
    h.state_mut().0.sicht.eingaben.code = "K7QM-3XPA".into();
    h.run();
    assert_eq!(
        h.query_all(By::new().role(Role::TextInput)).count(),
        2,
        "Adresse und Code"
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "Mit Microsoft anmelden")
            .is_none(),
        "nie beides"
    );
    h.get_by_role_and_label(Role::Button, "Gerät einbinden")
        .click();
    h.run();
    assert!(h.state().1.aktionen.borrow().contains(&Aktion::Einbinden {
        server: "http://127.0.0.1:8080".into(),
        code: "K7QM-3XPA".into(),
    }));
}

#[test]
fn ohne_beide_wege_sagt_die_seite_was_zu_tun_ist() {
    let mut h = verbindungsfenster(Verbindung::NichtEingebunden);
    wege_setzen(&mut h, "https://quiver.example", false, false);
    h.run();
    assert!(
        h.query_by_label("Dieser Server bietet weder Microsoft noch einen Einmal-Code an. Bitte an die Verwaltung wenden.")
            .is_some(),
        "klare Meldung fehlt"
    );
}

#[test]
fn im_browser_gibt_es_erneut_oeffnen_und_abbrechen() {
    let mut h = verbindungsfenster(Verbindung::WartetAufBrowser {
        server: "https://quiver.example".into(),
        adresse: "https://quiver.example/auth/geraet/start?zustand=abc".into(),
    });
    h.run_steps(2);
    assert!(h.query_by_label("Im Browser fortfahren …").is_some());
    h.get_by_role_and_label(Role::Button, "Browser erneut öffnen")
        .click();
    h.step();
    let geoeffnet = h.output().platform_output.commands.iter().any(|befehl| {
        matches!(befehl, OutputCommand::OpenUrl(url)
            if url.url == "https://quiver.example/auth/geraet/start?zustand=abc")
    });
    assert!(
        geoeffnet,
        "die Anmeldeseite muss erneut geöffnet werden: {:?}",
        h.output().platform_output.commands
    );
    h.get_by_role_and_label(Role::Button, "Abbrechen").click();
    h.run_steps(2);
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::AnmeldungAbbrechen));
}

#[test]
fn angemeldet_als_und_abmelden_mit_rueckfrage() {
    let mut modell = modell(
        Verbindung::Verbunden {
            server: "https://quiver.example".into(),
            geraet: "Testrechner".into(),
        },
        Vec::new(),
    );
    modell.benutzer = Some("test@example.test".into());
    let mut h = fenster(modell, Some(Seite::Verbindung), Thema::Dunkel, gross());
    h.run();
    assert!(
        h.query_by_label("Angemeldet als").is_some(),
        "die Anzeige fehlt"
    );
    assert!(h.query_by_label("test@example.test").is_some());
    h.get_by_role_and_label(Role::Button, "Gerät abmelden")
        .click();
    h.run();
    assert!(
        h.query_by_label("Gerät abmelden?").is_some(),
        "die Rückfrage fehlt"
    );
    assert!(
        h.query_by_label("Alle Agenten verlieren sofort alle Tools. Deine Angaben und die Server-Adresse bleiben; das Gerät wird auch am Server gelöscht.").is_some(),
        "die Folgen fehlen"
    );
    h.get_by_role_and_label(Role::Button, "Abmelden").click();
    h.run();
    assert!(h.state().1.aktionen.borrow().contains(&Aktion::Abmelden));
}

#[test]
fn neu_anmelden_bietet_erneut_anmelden_und_steht_im_tray() {
    let mut modell = modell(
        Verbindung::NeuAnmelden {
            server: "https://quiver.example".into(),
            text: "Die Anmeldung gilt nicht mehr. Geräte-Token und Angaben bleiben.".into(),
        },
        Vec::new(),
    );
    modell.anmeldewege = Some(Anmeldewege {
        server: "https://quiver.example".into(),
        entra: true,
        code: false,
    });
    let mut h = fenster(
        modell.clone(),
        Some(Seite::Verbindung),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(
        h.query_by_label("Die Anmeldung gilt nicht mehr. Geräte-Token und Angaben bleiben.")
            .is_some(),
        "der Hinweis fehlt"
    );
    h.get_by_role_and_label(Role::Button, "Erneut anmelden")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::AnmeldenMicrosoft {
            server: "https://quiver.example".into()
        }));
    assert_eq!(
        quiver_fenster::logik::tray_zeile(&modell, quiver_fenster::texte::Sprache::De),
        "Neu anmelden nötig"
    );
}

#[test]
fn ein_anmeldefehler_erklaert_und_laesst_die_adresse_stehen() {
    let mut h = verbindungsfenster(Verbindung::AnmeldeFehler {
        server: "https://quiver.example".into(),
        text: "Dieses Konto oder Gerät ist gesperrt. Bitte an die Verwaltung wenden.".into(),
    });
    h.run();
    assert!(h
        .query_by_label("Dieses Konto oder Gerät ist gesperrt. Bitte an die Verwaltung wenden.")
        .is_some());
    assert!(
        h.state().0.sicht.eingaben.server.is_empty()
            || h.state().0.sicht.eingaben.server == "https://quiver.example",
        "die Adresse bleibt vorbelegt"
    );
    // Und der Weg zurück steht offen: „Weiter“ ist da.
    assert!(h.query_by_role_and_label(Role::Button, "Weiter").is_some());
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
fn viele_felder_scrollen_und_speichern_bleibt_sichtbar() {
    // Ein Eintrag mit vielen Feldern (z. B. E-Mail) in einem kleinen Fenster:
    // der Dialog darf nicht oben und unten aus dem Fenster ragen.
    let (breite, hoehe) = (900.0, 560.0);
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        (breite, hoehe),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Braucht Angaben")
        .click();
    h.run();
    if let Dialog::Werte { eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
        let vorlage = eingaben[0].clone();
        for i in 0..12 {
            let mut feld = vorlage.clone();
            feld.name = format!("FELD_{i}");
            eingaben.push(feld);
        }
    } else {
        panic!("kein Angaben-Dialog");
    }
    h.run();
    h.run();
    let speichern = h.get_by_label("Speichern").rect();
    assert!(
        speichern.top() >= 0.0 && speichern.bottom() <= hoehe,
        "Speichern liegt außerhalb des Fensters: {speichern:?}"
    );
    let titel = h.get_by_label("Angaben für Dateien").rect();
    assert!(titel.top() >= 0.0, "der Titel ragt oben heraus: {titel:?}");
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

/// Ein Schalter-Feld wie `zertifikat_pruefen` im ABAP-Dialog.
fn schalter_eintrag() -> EintragSicht {
    let mut schalter = feld_mit(
        "zertifikat_pruefen",
        Quelle::Client,
        FeldArt::Schalter,
        "Zertifikat prüfen",
    );
    schalter.pflicht = false;
    schalter.standard = Some("an".into());
    eintrag(
        "abap",
        "ABAP",
        "SAP-System anbinden.",
        EintragZustand::Laeuft { tools: 1 },
        vec![schalter],
    )
}

#[test]
fn ein_schalter_ist_eine_checkbox_mit_hand_cursor() {
    let mut h = fenster(
        modell(verbunden(), vec![schalter_eintrag()]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Angaben bearbeiten")
        .click();
    h.run();

    // Der Standard `an` ist vorgehakt.
    let haken = h.get_by_role_and_label(Role::CheckBox, "Zertifikat prüfen");
    assert_eq!(
        haken.accesskit_node().toggled(),
        Some(Toggled::True),
        "der Standard an ist nicht vorgehakt"
    );
    haken.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf dem Schalter"
    );

    // Ein Klick setzt `aus`, ein zweiter wieder `an`.
    h.get_by_role_and_label(Role::CheckBox, "Zertifikat prüfen")
        .click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Werte { eingaben, .. } => assert_eq!(eingaben[0].wert, "aus"),
        anderer => panic!("kein Angaben-Dialog: {anderer:?}"),
    }
    h.get_by_role_and_label(Role::CheckBox, "Zertifikat prüfen")
        .click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Werte { eingaben, .. } => assert_eq!(eingaben[0].wert, "an"),
        anderer => panic!("kein Angaben-Dialog: {anderer:?}"),
    }
}

#[test]
fn ein_ordner_wird_ueber_den_dialog_gewaehlt() {
    for (groesse, name) in [(gross(), "gross"), ((560.0, 720.0), "schmal")] {
        let mut h = fenster(
            modell(
                verbunden(),
                vec![eintrag(
                    "drawio",
                    "draw.io",
                    "Diagramme.",
                    EintragZustand::Laeuft { tools: 1 },
                    vec![feld_mit(
                        "ordner",
                        Quelle::Client,
                        FeldArt::Ordner,
                        "Arbeitsordner",
                    )],
                )],
            ),
            Some(Seite::Tools),
            Thema::Dunkel,
            groesse,
        );
        h.run();
        h.get_by_role_and_label(Role::Button, "Angaben bearbeiten")
            .click();
        h.run();
        let knopf = h.get_by_role_and_label(Role::Button, "Ordner wählen");
        let feld = h.get_by_role_and_label(Role::TextInput, "Arbeitsordner");
        assert!(
            feld.rect().width() > 120.0 && feld.rect().right() <= knopf.rect().left(),
            "{name}: Feld {:?}, Knopf {:?}",
            feld.rect(),
            knopf.rect()
        );
        knopf.hover();
        h.run();
        assert_eq!(
            h.output().platform_output.cursor_icon,
            CursorIcon::PointingHand,
            "kein Hand-Cursor auf „Ordner wählen“"
        );
        // Der Pfad kommt im Test aus der Sicht, der echte Dialog bleibt zu.
        h.state_mut().0.sicht.datei = Some("/tmp/Diagramme".into());
        h.get_by_role_and_label(Role::Button, "Ordner wählen")
            .click();
        h.run();
        match &h.state().0.sicht.dialog {
            Dialog::Werte { eingaben, .. } => assert_eq!(eingaben[0].wert, "/tmp/Diagramme"),
            anderer => panic!("kein Angaben-Dialog: {anderer:?}"),
        }
        if let Ok(ziel) = std::env::var("ORDNER_BILD") {
            h.render()
                .unwrap()
                .save(format!("{ziel}/ordner-{name}.png"))
                .unwrap();
        }
    }
}

#[test]
fn eine_auswahl_hat_hand_cursor_und_setzt_den_wert() {
    let mut auswahl = feld_mit("farbe", Quelle::Client, FeldArt::Text, "Farbe");
    auswahl.art = FeldArt::Auswahl {
        optionen: vec![
            FeldOption {
                wert: "rot".into(),
                anzeige: "Rot".into(),
            },
            FeldOption {
                wert: "blau".into(),
                anzeige: "Blau".into(),
            },
        ],
    };
    auswahl.pflicht = false;
    let mut h = fenster(
        modell(
            verbunden(),
            vec![eintrag(
                "farben",
                "Farben",
                "Wählt eine Farbe.",
                EintragZustand::Laeuft { tools: 1 },
                vec![auswahl],
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Angaben bearbeiten")
        .click();
    h.run();

    h.get_by_role_and_label(Role::ComboBox, "Farbe").hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf der Auswahl"
    );
    h.get_by_role_and_label(Role::ComboBox, "Farbe").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Rot").click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Werte { eingaben, .. } => assert_eq!(eingaben[0].wert, "rot"),
        anderer => panic!("kein Angaben-Dialog: {anderer:?}"),
    }
}

/// Der ABAP-Fall: zwei Pflichtfelder ohne Wert, „Speichern“ bleibt zu und
/// nennt die Titel der fehlenden Felder.
fn pflicht_eintrag() -> EintragSicht {
    let benutzer = feld_mit("benutzer", Quelle::Client, FeldArt::Text, "SAP-Benutzer");
    let adresse = feld_mit("url", Quelle::Client, FeldArt::Url, "ADT-Adresse");
    eintrag(
        "abap",
        "ABAP",
        "SAP-System anbinden.",
        EintragZustand::WartetAufWerte {
            felder: vec!["benutzer".into(), "url".into()],
        },
        vec![benutzer, adresse],
    )
}

#[test]
fn fehlende_pflichtfelder_stehen_neben_speichern() {
    let mut h = fenster(
        modell(verbunden(), vec![pflicht_eintrag()]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Braucht Angaben")
        .click();
    h.run();

    assert!(
        h.query_by_label("Es fehlt: SAP-Benutzer, ADT-Adresse")
            .is_some(),
        "der Grund fehlt neben Speichern"
    );
    assert!(
        h.get_by_role_and_label(Role::Button, "Speichern")
            .accesskit_node()
            .is_disabled(),
        "ohne Pflichtfelder muss Speichern gesperrt sein"
    );

    // Ausfüllen: der Grund verschwindet, Speichern geht.
    if let Dialog::Werte { eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
        eingaben[0].wert = "SAPUSER".into();
        eingaben[1].wert = "https://sap.example".into();
    }
    h.run();
    assert!(
        h.query_by_label("Es fehlt: SAP-Benutzer, ADT-Adresse")
            .is_none(),
        "mit Werten ist der Grund weg"
    );
    assert!(!h
        .get_by_role_and_label(Role::Button, "Speichern")
        .accesskit_node()
        .is_disabled());

    // Eine ungültige Zahl wird beim Speichern genannt.
    if let Dialog::Werte { eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
        eingaben[0].wert = "12a".into();
        eingaben[0].art = FeldArt::Zahl;
    }
    h.run();
    assert!(
        h.query_by_label("Ungültige Zahl: SAP-Benutzer").is_some(),
        "die ungültige Zahl wird nicht genannt"
    );
}

#[test]
fn die_feldliste_hat_eine_sichtbare_scrollleiste() {
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
    if let Dialog::Werte { eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
        let vorlage = eingaben[0].clone();
        for i in 0..12 {
            let mut feld = vorlage.clone();
            feld.name = format!("FELD_{i}");
            eingaben.push(feld);
        }
    } else {
        panic!("kein Angaben-Dialog");
    }
    h.run();
    h.run();
    assert!(
        h.query_all(By::new().role(Role::ScrollBar)).count() >= 1,
        "die Scrollleiste der Feldliste ist nicht sichtbar"
    );
}

#[test]
fn angaben_zeigen_den_hilfe_link() {
    let mut h = fenster(
        modell(verbunden(), vec![link_eintrag()]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Braucht Angaben")
        .click();
    h.run();
    assert!(matches!(h.state().0.sicht.dialog, Dialog::Werte { .. }));

    // Ohne Adresse: der Knopf steht da, ist aber gesperrt und sagt, was fehlt.
    let knopf = h.get_by_role_and_label(Role::Button, "Token in Outline erstellen");
    assert!(
        knopf.accesskit_node().is_disabled(),
        "ohne ${{url}} darf der Link nicht aufgehen"
    );
    knopf.hover();
    h.run_steps(20);
    assert!(
        h.query_by_label("Erst url eintragen").is_some(),
        "der gesperrte Knopf nennt das fehlende Feld"
    );

    // Adresse eintragen: der Knopf wird aktiv, ohne Speichern.
    if let Dialog::Werte { eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
        eingaben[0].wert = "https://wiki.example.de".into();
    }
    h.run();
    let knopf = h.get_by_role_and_label(Role::Button, "Token in Outline erstellen");
    assert!(
        !knopf.accesskit_node().is_disabled(),
        "mit Adresse ist der Link aktiv"
    );
    // Hand-Cursor auf dem Knopf (AGENTS.md §2.12).
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf dem Hilfe-Link"
    );

    // Der Klick öffnet den Standardbrowser mit der aufgelösten Adresse.
    h.get_by_role_and_label(Role::Button, "Token in Outline erstellen")
        .click();
    h.step();
    let geoeffnet = h.output().platform_output.commands.iter().any(|befehl| {
        matches!(befehl, OutputCommand::OpenUrl(url) if url.url == "https://wiki.example.de/settings/api-and-access")
    });
    assert!(
        geoeffnet,
        "die Adresse muss geöffnet werden: {:?}",
        h.output().platform_output.commands
    );
}

/// Der Angaben-Dialog mit Hilfe-Link (`fenster-{dark,light}-angaben.png`).
#[test]
fn bildschirmfoto_der_angaben() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in BILDER_THEMEN {
        for groesse in GROESSEN {
            let mut h = fenster(
                modell(verbunden(), vec![link_eintrag()]),
                Some(Seite::Tools),
                thema,
                groesse.masse,
            );
            zur_ruhe(&mut h);
            h.get_by_role_and_label(Role::Button, "Braucht Angaben")
                .click();
            zur_ruhe(&mut h);
            // Mit Adresse: der Link steht aktiv neben dem Token-Feld.
            if let Dialog::Werte { eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
                eingaben[0].wert = "https://wiki.example.de".into();
            }
            zur_ruhe(&mut h);
            assert!(
                h.query_by_label("Token in Outline erstellen").is_some(),
                "{}: der Link-Knopf fehlt",
                thema.name()
            );
            nichts_ragt_heraus(&h, groesse.masse.0, "angaben");
            foto(&mut h, &ziel, "angaben", thema, groesse.px);
        }
    }
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
    // Der Dialog beginnt bei der ersten freien Art – „pi" ist ja vergeben.
    match &h.state().0.sicht.dialog {
        Dialog::Koppeln { art, .. } => assert_eq!(
            art,
            &quiver_fenster::logik::AgentArt::ClaudeCode,
            "pi ist gekoppelt, also die nächste freie Art"
        ),
        anderer => panic!("kein Koppeln-Dialog: {anderer:?}"),
    }
    // Die schon gekoppelte Art steht ausgegraut mit Hinweis da; ein Klick
    // darauf ändert die Auswahl nicht.
    assert!(h.query_by_label("schon gekoppelt").is_some());
    h.get_by_role_and_label(Role::RadioButton, "pi").click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Koppeln { art, .. } => assert_eq!(
            art,
            &quiver_fenster::logik::AgentArt::ClaudeCode,
            "eine ausgegraute Art ist nicht wählbar"
        ),
        anderer => panic!("kein Koppeln-Dialog: {anderer:?}"),
    }

    // Hermes wählen und koppeln.
    h.get_by_role_and_label(Role::RadioButton, "Hermes").click();
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

    // Token, Endpunkt und der Schnipsel für die gewählte Art stehen im Dialog.
    assert!(h.query_all_by_label(TOKEN).count() >= 1);
    assert!(h
        .query_by_label("Das Token wird nur einmal angezeigt.")
        .is_some());
    let hermes = quiver_fenster::logik::schnipsel_hermes(ENDPUNKT);
    assert!(
        h.query_by_label(&hermes).is_some(),
        "Hermes-Schnipsel fehlt"
    );
    assert!(
        h.query_by_label("hermes mcp test quiver").is_some(),
        "Prüfbefehl fehlt"
    );

    // Schließen räumt den Dialog weg.
    h.get_by_role_and_label(Role::Button, "Schließen").click();
    h.run();
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

#[test]
fn andere_art_hat_ein_namensfeld_und_meldet_doppelte() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Agenten),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Agent koppeln")
        .click();
    h.run();
    // Feste Arten haben kein Namensfeld.
    assert_eq!(h.query_all(By::new().role(Role::TextInput)).count(), 0);

    h.get_by_role_and_label(Role::RadioButton, "Andere").click();
    h.run();
    assert_eq!(
        h.query_all(By::new().role(Role::TextInput)).count(),
        1,
        "„Andere“ bekommt ein Namensfeld"
    );

    // Ein Name, den es schon gibt, wird vor dem Absenden gezeigt.
    h.get_by_role(Role::TextInput).type_text("  PI ");
    h.run();
    assert!(
        h.query_by_label("Agent PI ist schon gekoppelt.").is_some(),
        "doppelter Name muss sichtbar sein"
    );

    // Ein eigener Name geht durch.
    if let Dialog::Koppeln { name, .. } = &mut h.state_mut().0.sicht.dialog {
        *name = "mein-agent".into();
    }
    h.run();
    assert!(h.query_by_label("Agent PI ist schon gekoppelt.").is_none());
    h.get_by_role_and_label(Role::Button, "Koppeln").click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::AgentKoppeln {
            name: "mein-agent".into()
        }));
    assert!(matches!(
        &h.state().0.sicht.dialog,
        Dialog::Token { name, .. } if name == "mein-agent"
    ));
    let json = quiver_fenster::logik::schnipsel_json(ENDPUNKT);
    assert!(h.query_by_label(&json).is_some(), "allgemeines JSON fehlt");
}

#[test]
fn token_erneuern_fragt_nach() {
    let mut h = fenster(
        modell(verbunden(), Vec::new()),
        Some(Seite::Agenten),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    let knopf = h.get_by_role_and_label(Role::Button, "Token erneuern");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Token erneuern“"
    );
    h.get_by_role_and_label(Role::Button, "Token erneuern")
        .click();
    h.run();
    assert!(
        matches!(&h.state().0.sicht.dialog, Dialog::TokenErneuern { id, name } if id == "a1" && name == "pi"),
        "Rückfrage vor dem neuen Token: {:?}",
        h.state().0.sicht.dialog
    );
    assert!(
        !h.state()
            .1
            .aktionen
            .borrow()
            .iter()
            .any(|a| matches!(a, Aktion::AgentTokenErneuern { .. })),
        "ohne Bestätigung kein neues Token"
    );
    // Der Knopf im Dialog bestätigt.
    h.get_all_by_role_and_label(Role::Button, "Token erneuern")
        .last()
        .expect("Knopf im Dialog")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::AgentTokenErneuern { id: "a1".into() }));
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

// ------------------------------------------------- Einstellungen (Aufgabe 15)

/// Der Pfad, den der Datei-Dialog im Test liefert: er steht in der Sicht,
/// damit kein nativer Dialog aufgeht.
const SICHERUNG: &str = "/tmp/quiver-einstellungen-test.json";

/// Eine Vorschau, wie sie der Kern nach dem Prüfen liefert.
fn sicherung_vorschau(hat_geheimnisse: bool) -> Vorschau {
    Vorschau {
        eintraege: vec![
            VorschauEintrag {
                id: "echo".into(),
                titel: "Echo".into(),
                art: VorschauArt::Neu,
                neue: 2,
                geaenderte: 0,
                zugaenge: 0,
                am_geraet_aus: false,
            },
            VorschauEintrag {
                id: "dateien".into(),
                titel: "Dateien".into(),
                art: VorschauArt::Aendert,
                neue: 1,
                geaenderte: 2,
                zugaenge: 1,
                am_geraet_aus: false,
            },
            VorschauEintrag {
                id: "weg".into(),
                titel: "weg".into(),
                art: VorschauArt::Uebersprungen,
                neue: 0,
                geaenderte: 0,
                zugaenge: 0,
                am_geraet_aus: false,
            },
        ],
        hat_geheimnisse,
    }
}

/// Die beiden Knöpfe stehen in der Kopfzeile der Tools-Seite.
#[test]
fn exportieren_und_importieren_stehen_auf_der_toolsseite() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    for name in ["Exportieren", "Importieren"] {
        let knopf = h
            .query_by_role_and_label(Role::Button, name)
            .unwrap_or_else(|| panic!("{name} fehlt in der Kopfzeile"));
        knopf.hover();
        h.run();
        assert_eq!(
            h.output().platform_output.cursor_icon,
            CursorIcon::PointingHand,
            "„{name}“: kein Hand-Cursor"
        );
    }
}

/// Der Export-Dialog: die Passwortfelder gibt es erst mit dem Schalter
/// „Geheimnisse mitnehmen“.
#[test]
fn export_dialog_zeigt_die_passwortfelder_erst_mit_dem_schalter() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.state_mut().0.sicht.datei = Some(PathBuf::from(SICHERUNG));
    h.get_by_role_and_label(Role::Button, "Exportieren").click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Export {
            pfad, geheimnisse, ..
        } => {
            assert_eq!(pfad, &PathBuf::from(SICHERUNG));
            assert!(!geheimnisse, "der Schalter beginnt aus");
        }
        anderer => panic!("kein Export-Dialog: {anderer:?}"),
    }
    assert_eq!(
        h.query_all(By::new().role(Role::PasswordInput)).count(),
        0,
        "ohne Geheimnisse braucht es kein Passwort"
    );

    // Schalter an: zwei Passwortfelder kommen dazu.
    h.get_by_role_and_label(Role::CheckBox, "Geheimnisse mitnehmen")
        .click();
    h.run();
    assert!(matches!(
        h.state().0.sicht.dialog,
        Dialog::Export {
            geheimnisse: true,
            ..
        }
    ));
    assert_eq!(
        h.query_all(By::new().role(Role::PasswordInput)).count(),
        2,
        "Passwort und Wiederholung"
    );

    // Zu kurz: „Sichern“ bleibt gesperrt.
    if let Dialog::Export { passwort, .. } = &mut h.state_mut().0.sicht.dialog {
        *passwort = "kurz".into();
    }
    h.run();
    assert!(h.query_by_label("Mindestens 8 Zeichen.").is_some());
    assert!(
        h.get_by_role_and_label(Role::Button, "Sichern")
            .accesskit_node()
            .is_disabled(),
        "ein zu kurzes Passwort lässt sich nicht sichern"
    );

    // Lang genug, aber nicht gleich: der Hinweis nennt es.
    if let Dialog::Export {
        passwort,
        wiederholung,
        ..
    } = &mut h.state_mut().0.sicht.dialog
    {
        *passwort = "ein-langes-passwort".into();
        *wiederholung = "etwas-anderes".into();
    }
    h.run();
    assert!(h
        .query_by_label("Die Passwörter stimmen nicht überein.")
        .is_some());

    // Gleich: es geht weiter, mit dem Passwort.
    if let Dialog::Export { wiederholung, .. } = &mut h.state_mut().0.sicht.dialog {
        *wiederholung = "ein-langes-passwort".into();
    }
    h.run();
    h.get_by_role_and_label(Role::Button, "Sichern").click();
    h.run();
    let aktionen = h.state().1.aktionen.borrow();
    let Some(Aktion::EinstellungenExportieren { pfad, passwort }) = aktionen.last() else {
        panic!("kein Export: {aktionen:?}");
    };
    assert_eq!(pfad, &PathBuf::from(SICHERUNG));
    assert_eq!(passwort.as_deref(), Some("ein-langes-passwort"));
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

/// Der Import-Dialog zeigt die Vorschau; das Passwort fragt er nur, wenn die
/// Datei Geheimnisse führt.
#[test]
fn import_dialog_zeigt_die_vorschau() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.state_mut().0.sicht.datei = Some(PathBuf::from(SICHERUNG));
    h.get_by_role_and_label(Role::Button, "Importieren").click();
    h.run();
    h.run();
    assert!(
        matches!(&h.state().0.sicht.dialog, Dialog::Import { pfad, .. } if pfad == &PathBuf::from(SICHERUNG)),
        "kein Import-Dialog: {:?}",
        h.state().0.sicht.dialog
    );
    // Je Eintrag: was neu ist, was sich ändert, was übersprungen wird. (Die
    // Titel stehen auch auf den Karten der Seite – deshalb wird gezählt.)
    for zeile in ["Echo", "Dateien", "weg"] {
        assert!(
            h.query_all(By::new().label(zeile)).count() >= 1,
            "{zeile} fehlt"
        );
    }
    assert!(h.query_by_label("neu · 2 neu").is_some());
    assert!(h
        .query_by_label("ändert · 1 neu · 2 geändert · 1 Zugänge")
        .is_some());
    assert!(h
        .query_by_label("übersprungen – auf diesem Gerät nicht vorhanden")
        .is_some());
    assert_eq!(
        h.query_all(By::new().role(Role::PasswordInput)).count(),
        0,
        "eine offene Sicherung braucht kein Passwort"
    );

    h.get_by_role_and_label(Role::Button, "Übernehmen").click();
    h.run();
    let aktionen = h.state().1.aktionen.borrow();
    let Some(Aktion::EinstellungenImportieren { pfad, passwort }) = aktionen.last() else {
        panic!("kein Import: {aktionen:?}");
    };
    assert_eq!(pfad, &PathBuf::from(SICHERUNG));
    assert_eq!(passwort, &None);
}

/// Eine Sicherung mit Geheimnissen: ein Passwortfeld, und „Übernehmen“ wartet
/// auf mindestens acht Zeichen.
#[test]
fn import_dialog_fragt_nach_dem_passwort() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.state().1.sicherung_mit_geheimnissen.set(true);
    h.run();
    h.state_mut().0.sicht.datei = Some(PathBuf::from(SICHERUNG));
    h.get_by_role_and_label(Role::Button, "Importieren").click();
    h.run();
    h.run();
    assert!(matches!(h.state().0.sicht.dialog, Dialog::Import { .. }));
    assert!(h
        .query_by_label(
            "Diese Sicherung enthält Geheimnisse. Dafür wird das Passwort von damals gebraucht."
        )
        .is_some());
    assert_eq!(
        h.query_all(By::new().role(Role::PasswordInput)).count(),
        1,
        "ein Passwortfeld"
    );
    assert!(
        h.get_by_role_and_label(Role::Button, "Übernehmen")
            .accesskit_node()
            .is_disabled(),
        "ohne Passwort wird nichts übernommen"
    );

    if let Dialog::Import { passwort, .. } = &mut h.state_mut().0.sicht.dialog {
        *passwort = "ein-langes-passwort".into();
    }
    h.run();
    h.get_by_role_and_label(Role::Button, "Übernehmen").click();
    h.run();
    let aktionen = h.state().1.aktionen.borrow();
    let Some(Aktion::EinstellungenImportieren { passwort, .. }) = aktionen.last() else {
        panic!("kein Import: {aktionen:?}");
    };
    assert_eq!(passwort.as_deref(), Some("ein-langes-passwort"));
}

/// Nach dem Export steht das Ergebnis als Hinweis oben auf der Seite.
#[test]
fn das_ergebnis_steht_oben_auf_der_seite() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.state_mut().0.sicht.datei = Some(PathBuf::from(SICHERUNG));
    h.get_by_role_and_label(Role::Button, "Exportieren").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Sichern").click();
    h.run();
    h.run();
    assert!(
        h.query_by_label("6 Einträge exportiert (mit Geheimnissen)")
            .is_some(),
        "der Hinweis fehlt: {:?}",
        h.state().0.sicht.ergebnis
    );
}

/// Wohin die Bildschirmfotos gehen. Ein relativer Pfad gilt ab dem
/// Wurzelverzeichnis des Repos — so tut `QUIVER_BILDER=docs/bilder` überall
/// dasselbe, auch wenn cargo im Paketordner testet. Ein leerer Wert heißt
/// „keine Bilder“; sonst landeten sie im Wurzelverzeichnis.
fn bilder_ordner() -> Option<PathBuf> {
    let gesetzt = std::env::var_os("QUIVER_BILDER").map(PathBuf::from)?;
    if gesetzt.as_os_str().is_empty() {
        return None;
    }
    Some(match gesetzt {
        pfad if pfad.is_absolute() => pfad,
        pfad => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(pfad),
    })
}

/// Die drei Größen der Abnahme (Aufgabe 18): Minimum, mittel, breit.
#[derive(Debug, Clone, Copy)]
struct Groesse {
    /// Breite im Dateinamen.
    px: u32,
    masse: (f32, f32),
}

const GROESSEN: [Groesse; 4] = [
    Groesse {
        px: 560,
        masse: (560.0, 460.0),
    },
    Groesse {
        px: 800,
        masse: (800.0, 600.0),
    },
    Groesse {
        px: 1200,
        masse: (1200.0, 820.0),
    },
    Groesse {
        px: 1600,
        masse: (1600.0, 1000.0),
    },
];

/// Die beiden Themes, die die Doku zeigt (Bilder entstehen nur für sie).
const BILDER_THEMEN: [Thema; 2] = [Thema::Dunkel, Thema::Hell];

/// Ein Bildschirmfoto: `fenster-<thema>-<seite>-<breite>.png` (Aufgabe 18).
fn foto(
    h: &mut Harness<'static, Stand>,
    ziel: &std::path::Path,
    seite: &str,
    thema: Thema,
    px: u32,
) {
    foto_in(
        h,
        ziel,
        seite,
        thema,
        px,
        quiver_fenster::texte::Sprache::De,
    );
}

/// Dasselbe Bild in einer bestimmten Sprache. `README.de.md` zeigt die
/// deutschen Bilder, `README.md` die englischen (der Client schaltet die
/// Sprache um); die englische Fassung heißt `…-en.png`.
fn foto_in(
    h: &mut Harness<'static, Stand>,
    ziel: &std::path::Path,
    seite: &str,
    thema: Thema,
    px: u32,
    sprache: quiver_fenster::texte::Sprache,
) {
    h.state_mut().0.sicht.sprache = sprache;
    // Der Zeiger steht nach einem Klick über einem Knopf; fürs Bild wandert er
    // aus dem Fenster, damit er nichts verdeckt.
    h.event(egui::Event::PointerMoved(egui::pos2(-100.0, -100.0)));
    h.run();
    let sprache = match sprache {
        quiver_fenster::texte::Sprache::De => "",
        quiver_fenster::texte::Sprache::En => "-en",
    };
    let bild = h.render().expect("zeichnen (wgpu)");
    bild.save(ziel.join(format!(
        "fenster-{}-{seite}-{px}{sprache}.png",
        thema.name()
    )))
    .expect("Bild schreibbar");
}

/// Kein Steuerelement ragt seitlich aus dem Fenster (Aufgabe 18). Geprüft
/// wird über die Rechtecke der Widgets; nach unten darf Inhalt laufen — das
/// ist der Scrollbereich der Seite.
#[track_caller]
fn nichts_ragt_heraus(h: &Harness<'static, Stand>, breite: f32, seite: &str) {
    let mut draussen = Vec::new();
    for knoten in h.root().children_recursive() {
        let node = knoten.accesskit_node();
        if node.is_hidden() || node.raw_bounds().is_none() {
            continue;
        }
        let rect = knoten.rect();
        let name = node.label().unwrap_or_else(|| format!("{:?}", node.role()));
        if rect.left() < -0.5 || rect.right() > breite + 0.5 || rect.top() < -0.5 {
            draussen.push(format!("{name}: {rect:?}"));
        }
    }
    assert!(
        draussen.is_empty(),
        "{seite} bei {breite} px: {} Steuerelemente ragen hinaus:\n{:#?}",
        draussen.len(),
        draussen
    );
}

/// Alle Seiten in allen drei Themes und in den drei Größen der Abnahme:
/// nichts stürzt ab, nichts ragt seitlich hinaus, jede Seite hat ihren
/// Inhalt. Mit `QUIVER_BILDER` entstehen dabei die Bildschirmfotos in `dark`
/// und `light`.
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
            ("protokoll", Seite::Protokoll),
        ] {
            // Die Verbindungsseite zeigt als Startseite das Formular.
            for groesse in GROESSEN {
                let daten = match seite {
                    Seite::Verbindung => modell(Verbindung::NichtEingebunden, Vec::new()),
                    _ => modell(verbunden(), eintraege()),
                };
                let mut h = fenster(daten, Some(seite), thema, groesse.masse);
                zur_ruhe(&mut h);
                nichts_ragt_heraus(&h, groesse.masse.0, name);
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
                if let (Some(ziel), true) = (&ziel, BILDER_THEMEN.contains(&thema)) {
                    if name == "tools" {
                        // Titelbild: dieselbe Seite, aber mit dem Modell, das
                        // die Tools laufen zeigt (kein Fehler, höchstens ein
                        // Eintrag mit fehlenden Angaben).
                        let mut h = fenster(
                            modell(
                                verbunden(),
                                titel_eintraege(quiver_fenster::texte::Sprache::De),
                            ),
                            Some(seite),
                            thema,
                            groesse.masse,
                        );
                        zur_ruhe(&mut h);
                        foto(&mut h, ziel, name, thema, groesse.px);
                    } else {
                        foto(&mut h, ziel, name, thema, groesse.px);
                    }
                }
            }
        }
    }
}

/// Die englischen Fassungen der Bilder, die `README.md` zeigt: dasselbe
/// Fenster, nur über den Sprachschalter auf Englisch (`README.de.md` zeigt die
/// deutschen). Nur diese Seiten und Größen, damit nicht das halbe Verzeichnis
/// doppelt entsteht.
#[test]
fn bildschirmfoto_auf_englisch() {
    use quiver_fenster::texte::Sprache;

    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");

    // Titelbild (hell und dunkel), dieselbe Seite wie im deutschen README.
    for thema in BILDER_THEMEN {
        let mut h = fenster(
            modell(
                verbunden(),
                titel_eintraege(quiver_fenster::texte::Sprache::En),
            ),
            Some(Seite::Tools),
            thema,
            (1200.0, 820.0),
        );
        zur_ruhe(&mut h);
        foto_in(&mut h, &ziel, "tools", thema, 1200, Sprache::En);
    }

    // Koppeln-Dialog, 800 px, dunkel.
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Agenten),
        Thema::Dunkel,
        (800.0, 600.0),
    );
    zur_ruhe(&mut h);
    h.state_mut().0.sicht.sprache = Sprache::En;
    zur_ruhe(&mut h);
    h.get_by_role_and_label(Role::Button, "Pair agent").click();
    zur_ruhe(&mut h);
    assert!(matches!(h.state().0.sicht.dialog, Dialog::Koppeln { .. }));
    foto_in(&mut h, &ziel, "koppeln", Thema::Dunkel, 800, Sprache::En);

    // Protokollseite, 800 px, dunkel.
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Protokoll),
        Thema::Dunkel,
        (800.0, 600.0),
    );
    zur_ruhe(&mut h);
    foto_in(&mut h, &ziel, "protokoll", Thema::Dunkel, 800, Sprache::En);
}

/// Die Verbindungsseite mit Microsoft-Weg und der Zustand „neu anmelden“
/// (`fenster-<thema>-verbindung-entra-<breite>.png`, …). Beide entstehen aus
/// dem Modell, nicht aus einer Sonderzeichnung.
#[test]
fn bildschirmfoto_der_server_anmeldung() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in BILDER_THEMEN {
        for groesse in [
            Groesse {
                px: 560,
                masse: (560.0, 460.0),
            },
            Groesse {
                px: 800,
                masse: (800.0, 600.0),
            },
            Groesse {
                px: 1200,
                masse: (1200.0, 820.0),
            },
            Groesse {
                px: 1600,
                masse: (1600.0, 1000.0),
            },
        ] {
            // Schritt 2: der Server bietet Entra an.
            let mut daten = modell(Verbindung::NichtEingebunden, Vec::new());
            daten.server_url = Some("https://quiver.firma.de".into());
            daten.anmeldewege = Some(Anmeldewege {
                server: "https://quiver.firma.de".into(),
                entra: true,
                code: false,
            });
            let mut h = fenster(daten, Some(Seite::Verbindung), thema, groesse.masse);
            zur_ruhe(&mut h);
            foto(&mut h, &ziel, "verbindung-entra", thema, groesse.px);

            // „Neu anmelden“: alle Tools aus, Token und Angaben bleiben.
            let mut daten = modell(
                Verbindung::NeuAnmelden {
                    server: "https://quiver.firma.de".into(),
                    text: "Die Anmeldung gilt nicht mehr. Geräte-Token und Angaben bleiben.".into(),
                },
                Vec::new(),
            );
            daten.server_url = Some("https://quiver.firma.de".into());
            daten.anmeldewege = Some(Anmeldewege {
                server: "https://quiver.firma.de".into(),
                entra: true,
                code: false,
            });
            let mut h = fenster(daten, Some(Seite::Verbindung), thema, groesse.masse);
            zur_ruhe(&mut h);
            foto(&mut h, &ziel, "neu-anmelden", thema, groesse.px);
        }
    }
}

/// Der Koppeln-Dialog mit der Art-Auswahl — ein eigenes Bild, weil er sonst
/// nie zu sehen wäre (`fenster-<thema>-koppeln-<breite>.png`).
#[test]
fn bildschirmfoto_des_koppelns() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in BILDER_THEMEN {
        for groesse in GROESSEN {
            let mut h = fenster(
                modell(verbunden(), eintraege()),
                Some(Seite::Agenten),
                thema,
                groesse.masse,
            );
            zur_ruhe(&mut h);
            h.get_by_role_and_label(Role::Button, "Agent koppeln")
                .click();
            zur_ruhe(&mut h);
            assert!(matches!(h.state().0.sicht.dialog, Dialog::Koppeln { .. }));
            nichts_ragt_heraus(&h, groesse.masse.0, "koppeln");
            foto(&mut h, &ziel, "koppeln", thema, groesse.px);
        }
    }
}

/// Die Anmeldung auf der Tools-Karte: alle vier Zustände in einem Bild
/// (`fenster-<thema>-anmeldung-<breite>.png`).
#[test]
fn bildschirmfoto_der_anmeldung() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in BILDER_THEMEN {
        for groesse in GROESSEN {
            let mut h = fenster(
                modell(verbunden(), anmelde_zustaende()),
                Some(Seite::Tools),
                thema,
                groesse.masse,
            );
            zur_ruhe(&mut h);
            // Jeder Zustand ist zu sehen, bevor das Bild entsteht.
            assert!(h.query_by_label("Anmeldung nötig").is_some());
            assert!(h.query_by_label("Anmeldung im Browser läuft …").is_some());
            assert!(h
                .query_by_label("Angemeldet als test@beispiel.de")
                .is_some());
            assert!(h.query_by_label("Der Status endet mit Code 3").is_some());
            nichts_ragt_heraus(&h, groesse.masse.0, "anmeldung");
            foto(&mut h, &ziel, "anmeldung", thema, groesse.px);
        }
    }
}

#[test]
fn abgemeldet_zeigt_hinweis_und_anmelden() {
    let mut h = fenster(
        modell(
            verbunden(),
            vec![anmelde_eintrag(
                "postfach",
                "Postfach",
                AnmeldeZustand::Abgemeldet,
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(
        h.query_by_label("Anmeldung nötig").is_some(),
        "der Hinweis fehlt"
    );
    // Hand-Cursor auf dem Knopf, der die Anmeldung auslöst (AGENTS.md §2.12).
    let knopf = h.get_by_role_and_label(Role::Button, "Anmelden");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Anmelden“"
    );
    h.get_by_role_and_label(Role::Button, "Anmelden").click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::EintragAnmelden {
                eintrag: EintragId::neu("postfach")
            }),
        "der Klick muss die Anmeldung auslösen: {:?}",
        h.state().1.aktionen.borrow()
    );
    // Unbekannt ist derselbe Fall wie abgemeldet.
    let mut h = fenster(
        modell(
            verbunden(),
            vec![anmelde_eintrag(
                "postfach",
                "Postfach",
                AnmeldeZustand::Unbekannt,
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h.query_by_label("Anmeldung nötig").is_some());
    assert!(h
        .query_by_role_and_label(Role::Button, "Anmelden")
        .is_some());
}

#[test]
fn laufende_anmeldung_hat_keinen_knopf() {
    let mut h = fenster(
        modell(
            verbunden(),
            vec![anmelde_eintrag(
                "kalender",
                "Kalender",
                AnmeldeZustand::Laeuft,
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h.query_by_label("Anmeldung im Browser läuft …").is_some());
    for knopf in ["Anmelden", "Abmelden", "Erneut versuchen"] {
        assert!(
            h.query_by_role_and_label(Role::Button, knopf).is_none(),
            "„{knopf}“ darf während der Anmeldung nicht da sein"
        );
    }
}

#[test]
fn angemeldet_zeigt_das_konto_und_fragt_vor_dem_abmelden() {
    let mut h = fenster(
        modell(
            verbunden(),
            vec![anmelde_eintrag(
                "mail",
                "Mail",
                AnmeldeZustand::Angemeldet {
                    konto: Some("test@beispiel.de".into()),
                },
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h
        .query_by_label("Angemeldet als test@beispiel.de")
        .is_some());
    let knopf = h.get_by_role_and_label(Role::Button, "Abmelden");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Abmelden“"
    );
    h.get_by_role_and_label(Role::Button, "Abmelden").click();
    h.run();
    assert!(
        matches!(
            &h.state().0.sicht.dialog,
            Dialog::AnmeldungBeenden { eintrag, .. } if eintrag.als_str() == "mail"
        ),
        "vor dem Abmelden kommt eine Rückfrage: {:?}",
        h.state().0.sicht.dialog
    );
    assert!(
        !h.state()
            .1
            .aktionen
            .borrow()
            .iter()
            .any(|a| matches!(a, Aktion::EintragAbmelden { .. })),
        "ohne Bestätigung wird nicht abgemeldet"
    );
    // Der Knopf im Dialog bestätigt.
    h.get_all_by_role_and_label(Role::Button, "Abmelden")
        .last()
        .expect("Knopf im Dialog")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::EintragAbmelden {
            eintrag: EintragId::neu("mail")
        }));
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

#[test]
fn anmeldefehler_zeigt_text_und_erneut_versuchen() {
    let mut h = fenster(
        modell(
            verbunden(),
            vec![anmelde_eintrag(
                "freigaben",
                "Freigaben",
                AnmeldeZustand::Fehler {
                    text: "Der Status endet mit Code 3".into(),
                },
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h.query_by_label("Der Status endet mit Code 3").is_some());
    h.get_by_role_and_label(Role::Button, "Erneut versuchen")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::EintragAnmelden {
            eintrag: EintragId::neu("freigaben")
        }));
}

/// Die Einrichtung auf der Tools-Karte: alle drei Zustände in einem Bild
/// (`fenster-{dark,light}-einrichten.png`). Alle drei Themes werden gezeichnet,
/// gespeichert werden nur die beiden, die die Doku zeigt.
#[test]
fn bildschirmfoto_der_einrichtung() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in Thema::ALLE {
        for groesse in GROESSEN {
            let mut h = fenster(
                modell(verbunden(), einricht_zustaende()),
                Some(Seite::Tools),
                thema,
                groesse.masse,
            );
            zur_ruhe(&mut h);
            // Jeder Zustand ist zu sehen, bevor das Bild entsteht.
            assert!(
                h.query_by_label("Einrichtung im Browser").is_some(),
                "{}: „Bereit“ fehlt",
                thema.name()
            );
            assert!(
                h.query_by_label("Einrichtung im Browser geöffnet …")
                    .is_some(),
                "{}: „Läuft“ fehlt",
                thema.name()
            );
            assert!(
                h.query_by_label("Die Einrichtung endet mit Code 3")
                    .is_some(),
                "{}: der Fehlertext fehlt",
                thema.name()
            );
            nichts_ragt_heraus(&h, groesse.masse.0, "einrichten");
            if BILDER_THEMEN.contains(&thema) {
                foto(&mut h, &ziel, "einrichten", thema, groesse.px);
            }
        }
    }
}

#[test]
fn bereit_zeigt_den_knopf_einrichten() {
    let mut h = fenster(
        modell(
            verbunden(),
            vec![einricht_eintrag(
                "outline",
                "Outline",
                EinrichtZustand::Bereit,
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h.query_by_label("Einrichtung im Browser").is_some());
    // Hand-Cursor auf dem Knopf, der die Seite im Browser öffnet
    // (AGENTS.md §2.12).
    let knopf = h.get_by_role_and_label(Role::Button, "Einrichten");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Einrichten“"
    );
    h.get_by_role_and_label(Role::Button, "Einrichten").click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::EintragEinrichten {
                eintrag: EintragId::neu("outline")
            }),
        "der Klick muss die Einrichtung auslösen: {:?}",
        h.state().1.aktionen.borrow()
    );
}

#[test]
fn laufende_einrichtung_laesst_sich_abbrechen() {
    let mut h = fenster(
        modell(
            verbunden(),
            vec![einricht_eintrag("ssh", "SSH", EinrichtZustand::Laeuft)],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h
        .query_by_label("Einrichtung im Browser geöffnet …")
        .is_some());
    for knopf in ["Einrichten", "Erneut versuchen"] {
        assert!(
            h.query_by_role_and_label(Role::Button, knopf).is_none(),
            "„{knopf}“ darf während der Einrichtung nicht da sein"
        );
    }
    // „Abbrechen“ beendet die Einrichtung (Paket 3).
    h.get_by_role_and_label(Role::Button, "Abbrechen").click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::EinrichtungAbbrechen {
                eintrag: EintragId::neu("ssh")
            }),
        "{:?}",
        h.state().1.aktionen.borrow()
    );
}

#[test]
fn einrichtungsfehler_zeigt_text_und_erneut_versuchen() {
    let mut h = fenster(
        modell(
            verbunden(),
            vec![einricht_eintrag(
                "abap",
                "ABAP",
                EinrichtZustand::Fehler {
                    text: "Die Einrichtung endet mit Code 3".into(),
                },
            )],
        ),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h
        .query_by_label("Die Einrichtung endet mit Code 3")
        .is_some());
    h.get_by_role_and_label(Role::Button, "Erneut versuchen")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::EintragEinrichten {
            eintrag: EintragId::neu("abap")
        }));
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
    // Abmelden ist trotzdem möglich — mit Rückfrage.
    h.get_by_role_and_label(Role::Button, "Gerät abmelden")
        .click();
    h.run();
    h.get_all_by_role_and_label(Role::Button, "Abmelden")
        .last()
        .expect("Knopf im Dialog")
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

/// Der Autostart steht als Häkchen auf der Verbindungsseite und schreibt in
/// das Modell — im Test ohne Schalter, damit nichts im System landet.
#[test]
fn autostart_ist_ein_haken_auf_der_verbindungsseite() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Verbindung),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(!h.state().0.sicht.autostart, "Vorgabe: kein Autostart");
    let beschriftung = "Beim Anmelden im Hintergrund starten";
    let haken = h.get_by_role_and_label(Role::CheckBox, beschriftung);
    haken.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf dem Häkchen"
    );
    h.get_by_role_and_label(Role::CheckBox, beschriftung)
        .click();
    h.run();
    assert!(h.state().0.sicht.autostart, "Häkchen setzt das Modell");
    // Noch einmal klicken nimmt es zurück.
    h.get_by_role_and_label(Role::CheckBox, beschriftung)
        .click();
    h.run();
    assert!(!h.state().0.sicht.autostart);
}

// ------------------------------------------------------- Mehrere Zugänge (14)

/// Die Felder einer Zugangsliste wie bei quiver-ssh (Aufgabe 14).
fn zugang_felder() -> Vec<Feld> {
    let mut port = feld("port", Quelle::Client, "Port, Standard 22");
    port.pflicht = false;
    let mut anmelde = feld("anmelde_passwort", Quelle::Geheim, "Passwort der Anmeldung");
    anmelde.pflicht = false;
    let mut sudo = feld("sudo_passwort", Quelle::Geheim, "Passwort für sudo");
    sudo.pflicht = false;
    vec![
        feld("benutzer", Quelle::Client, "Benutzer für die Anmeldung"),
        feld("host", Quelle::Client, "Hostname oder Adresse"),
        port,
        anmelde,
        sudo,
    ]
}

/// Ein Eintrag mit Zugangsliste: `zugaenge` sind (Name, Werte, gesetzte
/// Geheimfelder).
fn zugang_eintrag(zugaenge: Vec<(&str, Werte, Vec<&str>)>) -> EintragSicht {
    let mut eintrag = eintrag(
        "ssh",
        "SSH",
        "Verbindungen zu Servern.",
        EintragZustand::Laeuft { tools: 1 },
        Vec::new(),
    );
    eintrag.tools = vec!["verbinden".into()];
    eintrag.zugaenge = Some(quiver_client::ZugaengeSicht {
        titel: "Verbindungen".into(),
        einzahl: "Verbindung".into(),
        felder: zugang_felder(),
        liste: zugaenge
            .into_iter()
            .map(|(name, werte, geheim)| quiver_client::ZugangSicht {
                name: name.into(),
                werte,
                geheim_gesetzt: geheim.into_iter().map(str::to_string).collect(),
            })
            .collect(),
    });
    eintrag
}

fn zwei_zugaenge() -> EintragSicht {
    zugang_eintrag(vec![
        (
            "db-01",
            Werte::from([
                ("benutzer".to_string(), "admin".to_string()),
                ("host".to_string(), "db-01.example".to_string()),
            ]),
            vec!["anmelde_passwort"],
        ),
        (
            "web-01",
            Werte::from([
                ("benutzer".to_string(), "root".to_string()),
                ("host".to_string(), "web-01.example".to_string()),
                ("port".to_string(), "22".to_string()),
            ]),
            vec!["anmelde_passwort", "sudo_passwort"],
        ),
    ])
}

#[test]
fn zugaenge_zeigen_titel_anzahl_und_liste() {
    let mut h = fenster(
        modell(verbunden(), vec![zwei_zugaenge()]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    // Die Zeile trägt Überschrift und Anzahl, daneben der Knopf zum Anlegen.
    assert!(h.query_by_label("Verbindungen (2)").is_some());
    assert!(h.query_by_label("Verbindung hinzufügen").is_some());
    // Zugeklappt steht kein Zugang da.
    assert!(h.query_by_label("web-01").is_none());

    h.get_by_role_and_label(Role::Button, "Verbindungen (2)")
        .click();
    h.run();
    assert!(h.query_by_label("web-01").is_some(), "Zugang fehlt");
    assert!(h.query_by_label("db-01").is_some(), "Zugang fehlt");
    // Name und ein bis zwei wichtige Werte, ohne Geheimes.
    assert!(h.query_by_label("root@web-01.example").is_some());
    assert!(h.query_by_label("admin@db-01.example").is_some());
    assert!(
        h.query_by_label("••••").is_none(),
        "Geheimes steht nie auf der Karte"
    );

    // Jeder Zugang hat „Bearbeiten“ und „Löschen“; dazu kommt der Stift der
    // Karte, deshalb zählt der Test die Knöpfe.
    assert_eq!(
        h.query_all_by_label("Bearbeiten").count(),
        2,
        "je Zeile ein „Bearbeiten“ (der Stift der Karte heißt anders)"
    );
    assert_eq!(
        h.query_all_by_label("Löschen").count(),
        2,
        "je Zeile ein „Löschen“"
    );
    // Hand-Cursor auf dem Knopf, der die Liste aufklappt (AGENTS.md §2.12).
    let knopf = h.get_by_role_and_label(Role::Button, "Verbindungen (2)");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf der Zugangsliste"
    );
}

#[test]
fn leere_zugangsliste_sagt_das_und_der_eintrag_laeuft() {
    let mut h = fenster(
        modell(verbunden(), vec![zugang_eintrag(Vec::new())]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    // Ein Eintrag mit `mehrfach`, aber ohne Zugang, läuft trotzdem.
    assert!(h.query_by_label("Läuft · 1 Tools").is_some());
    assert!(h.query_by_label("Verbindungen (0)").is_some());
    h.get_by_role_and_label(Role::Button, "Verbindungen (0)")
        .click();
    h.run();
    assert!(h.query_by_label("Noch nichts angelegt.").is_some());
    assert!(h.query_by_label("Verbindung hinzufügen").is_some());
}

#[test]
fn zugang_anlegen_schickt_namen_und_werte() {
    let mut h = fenster(
        modell(verbunden(), vec![zwei_zugaenge()]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Verbindung hinzufügen")
        .click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Zugang {
            name,
            neu,
            eingaben,
            ..
        } => {
            assert!(*neu, "hinzufügen legt an");
            assert!(name.is_empty());
            assert_eq!(eingaben.len(), 5, "ein Feld je Feld der Liste");
            assert!(eingaben[3].geheim, "anmelde_passwort ist geheim");
            assert!(!eingaben[3].gesetzt, "noch nicht gesetzt");
        }
        anderer => panic!("kein Zugangs-Dialog: {anderer:?}"),
    }
    // Ein ungültiger Name wird vor dem Absenden gezeigt.
    if let Dialog::Zugang { name, .. } = &mut h.state_mut().0.sicht.dialog {
        *name = "web 02".into();
    }
    h.run();
    assert!(
        h.query_by_label("Nur Buchstaben (A–Z, a–z), 0–9, - und _, höchstens 40 Zeichen.")
            .is_some(),
        "ungültiger Name muss sichtbar sein"
    );
    // Großbuchstaben sind erlaubt (SAP-Systeme heißen z. B. BL4_900).
    if let Dialog::Zugang { name, .. } = &mut h.state_mut().0.sicht.dialog {
        *name = "BL4_900".into();
    }
    h.run();
    assert!(
        h.query_by_label("Nur Buchstaben (A–Z, a–z), 0–9, - und _, höchstens 40 Zeichen.")
            .is_none(),
        "BL4_900 ist ein gültiger Name"
    );
    // Ausfüllen wie im Fenster und speichern.
    if let Dialog::Zugang { name, eingaben, .. } = &mut h.state_mut().0.sicht.dialog {
        *name = "web-02".into();
        eingaben[0].wert = "root".into();
        eingaben[1].wert = "web-02.example".into();
        eingaben[2].wert = "22".into();
        eingaben[3].wert = "geheim".into();
    }
    h.run();
    assert!(
        h.query_by_label("Nur Buchstaben (A–Z, a–z), 0–9, - und _, höchstens 40 Zeichen.")
            .is_none(),
        "mit gültigem Namen ist der Hinweis weg"
    );
    // Der Dialog setzt sich neu zusammen; erst dann trifft der Klick.
    h.run();
    h.run();
    h.get_by_role_and_label(Role::Button, "Speichern").click();
    h.run();
    let aktionen = h.state().1.aktionen.borrow();
    let Some(Aktion::ZugangSpeichern {
        eintrag,
        name,
        lokal,
        geheim,
    }) = aktionen.last()
    else {
        panic!("kein ZugangSpeichern: {aktionen:?}");
    };
    assert_eq!(eintrag.als_str(), "ssh");
    assert_eq!(name, "web-02");
    assert_eq!(
        lokal.get("host").map(String::as_str),
        Some("web-02.example")
    );
    assert_eq!(
        geheim.get("anmelde_passwort").map(String::as_str),
        Some("geheim"),
        "das Geheimnis geht in den Tresor, nicht ins Profil"
    );
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

#[test]
fn zugang_bearbeiten_hat_festen_namen_und_loescht_ein_geheimnis() {
    let mut h = fenster(
        modell(verbunden(), vec![zwei_zugaenge()]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Verbindungen (2)")
        .click();
    h.run();
    // „Bearbeiten“ gibt es nur in den Zeilen; die erste ist db-01.
    h.get_all_by_role_and_label(Role::Button, "Bearbeiten")
        .next()
        .expect("Knopf der ersten Zeile")
        .click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Zugang {
            name,
            neu,
            eingaben,
            ..
        } => {
            assert!(!*neu, "bearbeiten legt nichts an");
            assert_eq!(name, "db-01", "der Name steht fest");
            assert_eq!(eingaben[0].wert, "admin", "Werte sind vorbelegt");
            assert!(eingaben[3].gesetzt, "das Geheimnis ist gesetzt");
            assert_eq!(eingaben[3].wert, "", "Geheimes wird nie angezeigt");
        }
        anderer => panic!("kein Zugangs-Dialog: {anderer:?}"),
    }
    assert!(
        h.query_by_label("Passwort der Anmeldung · Gesetzt – leer lassen behält es.")
            .is_some(),
        "der Hinweis am Geheimfeld fehlt"
    );
    // „Löschen“ am Geheimfeld entfernt genau dieses Geheimnis.
    let knopf = h
        .get_all_by_role_and_label(Role::Button, "Geheimnis löschen")
        .next()
        .expect("Knopf am Geheimfeld");
    // Die Feldliste scrollt (Höhe wie in der Verwaltung) – erst hinscrollen.
    knopf.scroll_to_me();
    zur_ruhe(&mut h);
    let knopf = h
        .get_all_by_role_and_label(Role::Button, "Geheimnis löschen")
        .next()
        .expect("Knopf am Geheimfeld");
    knopf.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf „Geheimnis löschen“"
    );
    h.get_all_by_role_and_label(Role::Button, "Geheimnis löschen")
        .next()
        .expect("Knopf am Geheimfeld")
        .click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::ZugangGeheimLoeschen {
                eintrag: EintragId::neu("ssh"),
                name: "db-01".into(),
                feld: "anmelde_passwort".into(),
            }),
        "der Klick muss das Geheimnis löschen: {:?}",
        h.state().1.aktionen.borrow()
    );
    // Der Dialog bleibt offen, das Feld gilt als nicht mehr gesetzt.
    match &h.state().0.sicht.dialog {
        Dialog::Zugang { eingaben, .. } => {
            assert!(!eingaben[3].gesetzt);
            assert!(!eingaben[4].gesetzt);
        }
        anderer => panic!("der Dialog ist zugegangen: {anderer:?}"),
    }
}

#[test]
fn zugang_loeschen_fragt_nach() {
    let mut h = fenster(
        modell(verbunden(), vec![zwei_zugaenge()]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Verbindungen (2)")
        .click();
    h.run();
    h.get_all_by_role_and_label(Role::Button, "Löschen")
        .next()
        .expect("Knopf der ersten Zeile")
        .click();
    h.run();
    assert!(
        matches!(
            &h.state().0.sicht.dialog,
            Dialog::ZugangEntfernen { name, .. } if name == "db-01"
        ),
        "vor dem Löschen kommt eine Rückfrage: {:?}",
        h.state().0.sicht.dialog
    );
    assert!(
        !h.state()
            .1
            .aktionen
            .borrow()
            .iter()
            .any(|a| matches!(a, Aktion::ZugangLoeschen { .. })),
        "ohne Bestätigung wird nichts gelöscht"
    );
    h.get_all_by_role_and_label(Role::Button, "Löschen")
        .last()
        .expect("Knopf im Dialog")
        .click();
    h.run();
    assert!(h
        .state()
        .1
        .aktionen
        .borrow()
        .contains(&Aktion::ZugangLoeschen {
            eintrag: EintragId::neu("ssh"),
            name: "db-01".into(),
        }));
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

/// Die Zugangsliste und der Dialog (`fenster-{dark,light}-zugaenge.png`).
#[test]
fn bildschirmfoto_der_zugaenge() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in BILDER_THEMEN {
        for groesse in GROESSEN {
            let mut h = fenster(
                modell(verbunden(), vec![zwei_zugaenge()]),
                Some(Seite::Tools),
                thema,
                groesse.masse,
            );
            zur_ruhe(&mut h);
            h.get_by_role_and_label(Role::Button, "Verbindungen (2)")
                .click();
            zur_ruhe(&mut h);
            h.get_all_by_role_and_label(Role::Button, "Bearbeiten")
                .next()
                .expect("Knopf der ersten Zeile")
                .click();
            zur_ruhe(&mut h);
            assert!(
                h.query_by_label("Passwort der Anmeldung · Gesetzt – leer lassen behält es.")
                    .is_some(),
                "{}/{}: der Dialog zeigt den Geheimnis-Hinweis nicht: {:?}",
                thema.name(),
                groesse.px,
                h.state().0.sicht.dialog
            );
            nichts_ragt_heraus(&h, groesse.masse.0, "zugaenge");
            foto(&mut h, &ziel, "zugaenge", thema, groesse.px);
        }
    }
}

// --------------------------------------------------- Protokoll (Aufgabe 17)

/// Die Protokollseite mit den drei Beispielaufrufen.
fn protokoll_fenster(thema: Thema) -> Harness<'static, Stand> {
    fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Protokoll),
        thema,
        gross(),
    )
}

#[test]
fn die_protokollseite_ist_erreichbar_und_zeigt_die_aufrufe() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Protokoll").click();
    h.run();
    assert_eq!(h.state().0.sicht.seite, Seite::Protokoll);
    // Die Spalten stehen da (Agent und Tool stehen auch an den Filtern).
    for spalte in ["Zeit", "Agent", "Tool", "Dauer", "Status"] {
        assert!(
            h.query_all(By::new().value(spalte)).count() > 0,
            "{spalte} fehlt"
        );
    }
    // Je Aufruf eine Zeile, mit Werkzeug und Ergebnis.
    let zeilen = h.query_all(By::new().role(Role::Button)).count();
    assert!(zeilen >= 3, "die Aufrufe fehlen ({zeilen} Knöpfe)");
    // Das Werkzeug steht als „Eintrag › Tool“ in der Spalte.
    assert!(h.query_by_label("echo › echo").is_some());
    assert!(h.query_by_label("dateien › lesen").is_some());
    // Die Ergebnisspalte als Abzeichen.
    assert!(h.query_by_label("OK").is_some());
    assert!(h.query_by_label("Fehler").is_some());
    assert!(h.query_by_label("Abgelehnt").is_some());
}

#[test]
fn die_filter_der_protokollseite_wirken() {
    let mut h = protokoll_fenster(Thema::Dunkel);
    h.run();
    assert_eq!(h.state().0.sicht.aufrufe.len(), 3);

    // Eintrag wählen: nur dessen Aufrufe bleiben.
    h.state_mut().0.sicht.protokoll_eingaben.eintrag = "echo".into();
    h.run();
    let aufrufe = h.state().0.sicht.aufrufe.clone();
    assert_eq!(aufrufe.len(), 1, "{aufrufe:?}");
    assert_eq!(aufrufe[0].tool, "echo__echo");
    assert!(h.query_by_label("dateien › lesen").is_none());

    // Agent wählen: nur „hermes“.
    h.state_mut().0.sicht.protokoll_eingaben.eintrag.clear();
    h.state_mut().0.sicht.protokoll_eingaben.agent = "hermes".into();
    h.run();
    assert_eq!(h.state().0.sicht.aufrufe.len(), 1);
    assert_eq!(h.state().0.sicht.aufrufe[0].agent, "hermes");

    // Nur Fehler: der eine Fehler und die Ablehnung.
    h.state_mut().0.sicht.protokoll_eingaben.agent.clear();
    h.state_mut().0.sicht.protokoll_eingaben.nur_fehler = true;
    h.run();
    assert_eq!(
        h.state().0.sicht.aufrufe.len(),
        2,
        "Fehler und Ablehnung zählen beide"
    );

    // Werkzeug wählen.
    h.state_mut().0.sicht.protokoll_eingaben.nur_fehler = false;
    h.state_mut().0.sicht.protokoll_eingaben.tool = "graph__kalender".into();
    h.run();
    assert_eq!(h.state().0.sicht.aufrufe.len(), 1);
    assert_eq!(h.state().0.sicht.aufrufe[0].status, Status::Abgelehnt);
}

#[test]
fn ein_klick_auf_einen_aufruf_oeffnet_die_einzelheiten() {
    let mut h = protokoll_fenster(Thema::Dunkel);
    h.run();
    // Die Zeile „dateien“ trägt den Fehlertext.
    let aufruf = h
        .state()
        .0
        .sicht
        .aufrufe
        .iter()
        .find(|aufruf| aufruf.tool == "dateien__lesen")
        .cloned()
        .expect("Aufruf im Modell");
    let name = quiver_fenster::logik::aufruf_zeile(quiver_fenster::texte::Sprache::De, &aufruf);
    h.get_by_role_and_label(Role::Button, &name).click();
    h.run();
    match &h.state().0.sicht.dialog {
        Dialog::Aufruf { aufruf } => {
            assert_eq!(aufruf.tool, "dateien__lesen");
            assert_eq!(aufruf.fehler.as_deref(), Some("Der Prozess wurde beendet"));
        }
        anderer => panic!("kein Aufruf-Dialog: {anderer:?}"),
    }
    // Der Dialog zeigt Argumente, Fehlertext und Vorschau.
    assert!(h.query_by_label("Argumente").is_some());
    assert!(h.query_by_label("Fehlertext").is_some());
    assert!(h.query_by_label("Der Prozess wurde beendet").is_some());
    assert!(h.query_by_label("hallo").is_none(), "anderer Aufruf");
    // Schließen räumt den Dialog weg.
    h.get_by_role_and_label(Role::Button, "Schließen").click();
    h.run();
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

#[test]
fn hand_cursor_auf_alles_klickbarem_der_protokollseite() {
    let mut h = protokoll_fenster(Thema::Dunkel);
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
            "„{name}“: kein Hand-Cursor"
        );
    }
}

/// „nur Fehler“ steht in derselben Zeile wie die Filter, nicht in einer
/// eigenen (Aufgabe 18).
#[test]
fn die_checkbox_nur_fehler_steht_in_der_filterzeile() {
    let mut h = protokoll_fenster(Thema::Dunkel);
    zur_ruhe(&mut h);
    let haken = h.get_by_role_and_label(Role::CheckBox, "nur Fehler").rect();
    let auswahl = h
        .get_all_by_role_and_label(Role::ComboBox, "Alle")
        .next()
        .expect("Filter Eintrag")
        .rect();
    assert!(
        auswahl.center().y >= haken.top() && auswahl.center().y <= haken.bottom(),
        "die Checkbox steht nicht auf der Höhe der Filter: {haken:?} / {auswahl:?}"
    );
    assert!(
        haken.left() > auswahl.right(),
        "die Checkbox folgt den Auswahlen in derselben Zeile: {haken:?} / {auswahl:?}"
    );
}

/// Ein Klick auf eine Zeile öffnet das Detail; darüber liegt der Hand-Cursor
/// (Aufgabe 18).
#[test]
fn zeilen_hover_hat_einen_hand_cursor() {
    let mut h = protokoll_fenster(Thema::Dunkel);
    zur_ruhe(&mut h);
    let aufruf = h.state().0.sicht.aufrufe[0].clone();
    let name = quiver_fenster::logik::aufruf_zeile(quiver_fenster::texte::Sprache::De, &aufruf);
    h.get_by_role_and_label(Role::Button, &name).hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf der Zeile"
    );
}

/// Unter 700 px fallen Agent und Dauer aus der Tabelle; sie stehen im Detail
/// (Aufgabe 18).
#[test]
fn unter_700_px_fehlen_agent_und_dauer() {
    for (breite, erwartet) in [(560.0, false), (1200.0, true)] {
        let mut h = fenster(
            modell(verbunden(), eintraege()),
            Some(Seite::Protokoll),
            Thema::Dunkel,
            (breite, 600.0),
        );
        zur_ruhe(&mut h);
        // „Agent“ steht auch an der Filterleiste; die Spalte kommt dazu.
        assert_eq!(
            h.query_all(By::new().role(Role::Label).value("Agent"))
                .count(),
            if erwartet { 2 } else { 1 },
            "Agentspalte bei {breite} px"
        );
        assert_eq!(
            h.query_all(By::new().role(Role::Label).value("Dauer"))
                .count(),
            usize::from(erwartet),
            "Dauerspalte bei {breite} px"
        );

        // Im Detail stehen beide Angaben immer.
        let aufruf = h.state().0.sicht.aufrufe[0].clone();
        let name = quiver_fenster::logik::aufruf_zeile(quiver_fenster::texte::Sprache::De, &aufruf);
        h.get_by_role_and_label(Role::Button, &name).hover();
        h.run();
        assert_eq!(
            h.output().platform_output.cursor_icon,
            CursorIcon::PointingHand,
            "kein Hand-Cursor auf der Zeile bei {breite} px"
        );
        h.get_by_role_and_label(Role::Button, &name).click();
        zur_ruhe(&mut h);
        assert!(
            matches!(h.state().0.sicht.dialog, Dialog::Aufruf { .. }),
            "der Klick öffnet das Detail bei {breite} px"
        );
        assert!(
            h.query_all(By::new().role(Role::Label).value("Dauer"))
                .count()
                >= 1,
            "die Dauer fehlt im Detail bei {breite} px"
        );
        assert!(
            h.query_all(By::new().role(Role::Label).value("Agent"))
                .count()
                >= 2,
            "der Agent fehlt im Detail bei {breite} px"
        );
    }
}

/// Die Fehlerzeile der Tool-Logs steht in der Fehlerfarbe (Aufgabe 18) —
/// geprüft über die reine Rechnung, die die Ansicht benutzt.
#[test]
fn fehlerzeilen_der_tool_logs_sind_fehler() {
    use quiver_fenster::logik::{log_stufe, log_ton, LogStufe};
    use quiver_fenster::thema::Ton;
    assert_eq!(log_ton("ERROR: Verbindung weg"), Ton::Fehler);
    assert_eq!(log_ton("warning: keine Antwort"), Ton::Warnung);
    assert_eq!(log_ton("gestartet, Werkzeuge: echo"), Ton::Normal);
    assert_eq!(
        log_stufe(quiver_fenster::texte::Sprache::De, "panic: kaputt"),
        Some("FEHLER".to_string())
    );
    assert_eq!(
        quiver_fenster::logik::log_stufe_art("gestartet"),
        None::<LogStufe>
    );
}

#[test]
fn protokoll_leeren_fragt_nach_und_schickt_die_aktion() {
    let mut h = protokoll_fenster(Thema::Dunkel);
    h.run();
    h.get_by_role_and_label(Role::Button, "Protokoll leeren")
        .click();
    h.run();
    assert_eq!(h.state().0.sicht.dialog, Dialog::ProtokollLeeren);
    assert!(
        !h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::ProtokollLeeren),
        "ohne Bestätigung wird nichts gelöscht"
    );
    h.get_all_by_role_and_label(Role::Button, "Protokoll leeren")
        .last()
        .expect("Knopf im Dialog")
        .click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::ProtokollLeeren),
        "der bestätigte Klick leert das Protokoll"
    );
    assert_eq!(h.state().0.sicht.dialog, Dialog::Keiner);
}

#[test]
fn der_reiter_tool_logs_zeigt_die_zeilen_in_monoschrift() {
    let mut h = protokoll_fenster(Thema::Dunkel);
    h.run();
    h.get_by_role_and_label(Role::Button, "Tool-Logs").click();
    h.run();
    assert_eq!(
        h.state().0.sicht.protokoll_reiter,
        quiver_fenster::modell::ProtokollReiter::ToolLogs
    );
    // Der erste Eintrag ist vorgewählt; seine zwei Zeilen stehen da.
    assert_eq!(h.state().0.sicht.log_eintrag, Some(EintragId::neu("echo")));
    assert!(h
        .query_by_label("INFO gestartet, Werkzeuge: echo, ping")
        .is_some());
    assert!(
        h.query_by_label("Dateien gestartet").is_none(),
        "anderer Eintrag"
    );
}

#[test]
fn der_schalter_der_agentenseite_ist_eine_checkbox() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Agenten),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    let haken =
        h.get_by_role_and_label(Role::CheckBox, "Agenten dürfen Inhalte im Protokoll sehen");
    assert_eq!(
        haken.accesskit_node().toggled(),
        Some(Toggled::False),
        "Standard ist aus"
    );
    haken.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf dem Schalter"
    );
    h.get_by_role_and_label(Role::CheckBox, "Agenten dürfen Inhalte im Protokoll sehen")
        .click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::ProtokollFuerAgenten { an: true }),
        "der Klick schaltet den Schalter ein: {:?}",
        h.state().1.aktionen.borrow()
    );
}

/// Die Protokollseite in beiden Themes (`fenster-{dark,light}-protokoll.png`).
#[test]
fn bildschirmfoto_des_protokolls() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in BILDER_THEMEN {
        for groesse in GROESSEN {
            // Erst die Aufrufe, dann die Tool-Logs — beide Reiter der Seite.
            let mut h = fenster(
                modell(verbunden(), eintraege()),
                Some(Seite::Protokoll),
                thema,
                groesse.masse,
            );
            zur_ruhe(&mut h);
            // Der neueste Aufruf steht oben; im kleinsten Fenster ist nur er
            // zu sehen.
            assert!(
                h.query_by_label("Abgelehnt").is_some(),
                "{} bei {} px: die Liste fehlt",
                thema.name(),
                groesse.px
            );
            nichts_ragt_heraus(&h, groesse.masse.0, "protokoll");
            foto(&mut h, &ziel, "protokoll", thema, groesse.px);

            h.get_by_role_and_label(Role::Button, "Tool-Logs").click();
            zur_ruhe(&mut h);
            assert!(
                h.query_by_label("INFO gestartet, Werkzeuge: echo, ping")
                    .is_some(),
                "{}: die Prozesszeilen fehlen",
                thema.name()
            );
            nichts_ragt_heraus(&h, groesse.masse.0, "tool-logs");
            foto(&mut h, &ziel, "tool-logs", thema, groesse.px);
        }
    }
}

/// Ein am Gerät abgeschalteter Eintrag, wie ihn der Kern liefert (Aufgabe 19):
/// Zustand `AmGeraetAus`, keine Werkzeuge, die Wahl im Modell.
fn abgeschaltet(mut eintrag: EintragSicht) -> EintragSicht {
    eintrag.am_geraet_aus = true;
    eintrag.zustand = EintragZustand::AmGeraetAus;
    eintrag.tools.clear();
    eintrag
}

#[test]
fn der_schalter_auf_der_karte_schaltet_ab_und_wieder_ein() {
    let mut h = fenster(
        modell(verbunden(), eintraege()),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();

    // Eine Karte je Eintrag, jede mit dem Schalter „Auf diesem Gerät aktiv“.
    assert_eq!(
        h.query_all_by_label("Auf diesem Gerät aktiv").count(),
        3,
        "je Karte ein Schalter"
    );
    // Standard an – auch für neu zugewiesene Einträge.
    let schalter = h
        .query_all(
            By::new()
                .role(Role::CheckBox)
                .label("Auf diesem Gerät aktiv"),
        )
        .next()
        .expect("der erste Schalter");
    assert_eq!(
        schalter.accesskit_node().toggled(),
        Some(Toggled::True),
        "der Schalter steht standardmäßig auf an"
    );
    // Hand-Cursor auf dem Schalter (AGENTS.md §2.12).
    schalter.hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        CursorIcon::PointingHand,
        "kein Hand-Cursor auf dem Schalter"
    );

    // Mit der Tastatur: Fokus auf den Schalter, Leertaste schaltet ihn (der
    // Schalter ist keine reine Maus-Bedienung, AGENTS.md §5).
    h.query_all(
        By::new()
            .role(Role::CheckBox)
            .label("Auf diesem Gerät aktiv"),
    )
    .next()
    .expect("der erste Schalter")
    .focus();
    h.run();
    assert!(
        h.get_all_by_role_and_label(Role::CheckBox, "Auf diesem Gerät aktiv")
            .next()
            .expect("der erste Schalter")
            .is_focused(),
        "der Fokus liegt auf dem Schalter"
    );
    h.key_press(egui::Key::Space);
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::EintragSchalten {
                eintrag: EintragId::neu("echo"),
                an: false,
            }),
        "die Leertaste muss abschalten: {:?}",
        h.state().1.aktionen.borrow()
    );

    // Der erste Schalter gehört zur Karte „Echo“.
    h.query_all(
        By::new()
            .role(Role::CheckBox)
            .label("Auf diesem Gerät aktiv"),
    )
    .next()
    .expect("der erste Schalter")
    .click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::EintragSchalten {
                eintrag: EintragId::neu("echo"),
                an: false,
            }),
        "der Klick muss das Abschalten auslösen: {:?}",
        h.state().1.aktionen.borrow()
    );
}

#[test]
fn eine_abgeschaltete_karte_ist_gedaempft_und_gesperrt() {
    let mut aus = abgeschaltet(anmelde_eintrag(
        "postfach",
        "Postfach",
        AnmeldeZustand::Angemeldet {
            konto: Some("test@beispiel.de".into()),
        },
    ));
    aus.einrichtung = Some(EinrichtZustand::Bereit);
    let mut h = fenster(
        modell(verbunden(), vec![aus]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();

    // Das Abzeichen sagt, was gilt – und der Schalter steht auf aus.
    assert!(
        h.query_by_label("Abgeschaltet").is_some(),
        "Abzeichen fehlt"
    );
    let schalter = h.get_by_role_and_label(Role::CheckBox, "Auf diesem Gerät aktiv");
    assert_eq!(schalter.accesskit_node().toggled(), Some(Toggled::False));
    // Der Hinweis erklärt, was „aus“ bedeutet.
    assert!(
        h.query_by_label("Auf diesem Gerät abgeschaltet – einschalten, um es wieder zu starten.")
            .is_some(),
        "der Hinweis fehlt"
    );

    // Anmelden und Einrichten sind gesperrt, mit Hinweis am Zeiger.
    for knopf in ["Abmelden", "Einrichten"] {
        let knoten = h.get_by_role_and_label(Role::Button, knopf);
        assert!(
            knoten.accesskit_node().is_disabled(),
            "„{knopf}“ muss gesperrt sein"
        );
        knoten.click();
        h.run();
        assert!(
            !h.state().1.aktionen.borrow().iter().any(|aktion| matches!(
                aktion,
                Aktion::EintragAnmelden { .. }
                    | Aktion::EintragAbmelden { .. }
                    | Aktion::EintragEinrichten { .. }
            )),
            "ein gesperrter Knopf löst nichts aus: {:?}",
            h.state().1.aktionen.borrow()
        );
    }

    // Der Filter „nur aktive“ lässt die Karte verschwinden.
    let mut liste = eintraege();
    liste.push(abgeschaltet(anmelde_eintrag(
        "postfach",
        "Postfach",
        AnmeldeZustand::Abgemeldet,
    )));
    let mut h = fenster(
        modell(verbunden(), liste),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    assert!(h.query_by_label("Postfach").is_some());
    h.get_by_role_and_label(Role::CheckBox, "Nur aktive")
        .click();
    h.run();
    assert!(
        h.state().0.sicht.nur_aktive,
        "der Filter ist an: {:?}",
        h.state().0.sicht.nur_aktive
    );
    assert!(
        h.query_by_label("Postfach").is_none(),
        "die abgeschaltete Karte ist ausgeblendet"
    );
    assert!(h.query_by_label("Echo").is_some(), "die aktive bleibt");
}

/// Schmal (560 px): der Schalter, das Abzeichen und die gesperrten Knöpfe
/// ragen nicht aus dem Fenster (Aufgabe 18 gilt weiter, Aufgabe 19).
#[test]
fn die_abschaltung_ragt_auch_schmal_nicht_heraus() {
    let mut aus = abgeschaltet(anmelde_eintrag(
        "postfach",
        "Postfach",
        AnmeldeZustand::Abgemeldet,
    ));
    aus.einrichtung = Some(EinrichtZustand::Bereit);
    let mut liste = eintraege();
    liste.push(aus);
    for (breite, hoehe) in [(560.0, 460.0), (800.0, 600.0)] {
        let mut h = fenster(
            modell(verbunden(), liste.clone()),
            Some(Seite::Tools),
            Thema::Dunkel,
            (breite, hoehe),
        );
        zur_ruhe(&mut h);
        nichts_ragt_heraus(&h, breite, "tools-abgeschaltet");
        assert!(h.query_by_label("Abgeschaltet").is_some());
    }
}

/// Ein Bild der abgeschalteten Karte in beiden Themes und drei Größen.
#[test]
fn bildschirmfoto_der_abschaltung() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    let mut aus = abgeschaltet(anmelde_eintrag(
        "postfach",
        "Postfach",
        AnmeldeZustand::Angemeldet {
            konto: Some("test@beispiel.de".into()),
        },
    ));
    aus.einrichtung = Some(EinrichtZustand::Bereit);
    // Die abgeschaltete Karte steht vorn: so ist sie auch im schmalen Bild
    // ganz zu sehen, und der Filter „nur aktive“ steht darüber.
    let mut liste = vec![aus];
    liste.extend(eintraege());
    for thema in BILDER_THEMEN {
        for groesse in GROESSEN {
            let mut h = fenster(
                modell(verbunden(), liste.clone()),
                Some(Seite::Tools),
                thema,
                groesse.masse,
            );
            zur_ruhe(&mut h);
            assert!(h.query_by_label("Abgeschaltet").is_some());
            nichts_ragt_heraus(&h, groesse.masse.0, "abschaltung");
            foto(&mut h, &ziel, "abschaltung", thema, groesse.px);
        }
    }
}

/// Eine unbrauchbare Kennung steht auf der Karte mit Grund (Paket 3) – der
/// Eintrag startet nicht, und der User sieht warum.
#[test]
fn eine_unbrauchbare_kennung_ist_benannt() {
    let mut kaputt = eintrag(
        "quiver",
        "Reserviert",
        "",
        EintragZustand::Startet,
        Vec::new(),
    );
    kaputt.unzulaessig =
        Some("Die Kennung quiver ist für die Werkzeuge des Clients selbst reserviert".into());
    let mut h = fenster(
        modell(verbunden(), vec![kaputt]),
        Some(Seite::Tools),
        Thema::Dunkel,
        gross(),
    );
    h.run();
    let erwartet = "Diese Kennung taugt nicht als Werkzeug-Präfix: Die Kennung quiver ist für die \
                    Werkzeuge des Clients selbst reserviert. Der Eintrag startet deshalb nicht – \
                    bitte in der Verwaltung ändern.";
    assert!(
        h.query_by_label(erwartet).is_some(),
        "der Grund steht auf der Karte"
    );
}

/// Ist der Tresor nicht lesbar, sagt es der Hinweis oben – mit „Neu beginnen“
/// hinter einer Rückfrage (Paket 3).
#[test]
fn ein_kaputter_tresor_bietet_neu_beginnen_an() {
    let mut stand = modell(verbunden(), eintraege());
    stand.tresor_fehler = Some("tresor.json ist beschädigt".into());
    let mut h = fenster(stand, Some(Seite::Tools), Thema::Dunkel, gross());
    h.run();
    assert!(
        h.query_by_label("Tresor nicht lesbar: tresor.json ist beschädigt")
            .is_some(),
        "der Hinweis steht oben"
    );
    // „Neu beginnen“ öffnet die Rückfrage, nicht die Tat.
    h.get_by_role_and_label(Role::Button, "Neu beginnen")
        .click();
    h.run();
    assert_eq!(
        h.state().0.sicht.dialog,
        Dialog::TresorNeuBeginnen,
        "erst die Rückfrage"
    );
    assert!(
        h.query_by_label("Tresor neu beginnen").is_some(),
        "der Dialog erklärt, was bleibt"
    );
    // Bestätigt wird die Aktion abgegeben – der Knopf im Dialog (der Hinweis
    // oben trägt denselben Namen).
    let mut knoepfe: Vec<_> = h
        .query_all_by_role_and_label(Role::Button, "Neu beginnen")
        .collect();
    knoepfe.pop().expect("der Knopf im Dialog").click();
    h.run();
    assert!(
        h.state()
            .1
            .aktionen
            .borrow()
            .contains(&Aktion::TresorNeuBeginnen),
        "{:?}",
        h.state().1.aktionen.borrow()
    );
}
