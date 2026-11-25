// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Verbindung: Server-Adresse, Anmeldeweg (Entra oder Einmal-Code), der
//! Browser-Lauscher und das Abmelden.
//!
//! Die Seite folgt dem Modell (AGENTS.md §2.1): erst **nur** die Adresse, nach
//! „Weiter“ **entweder** „Mit Microsoft anmelden“ **oder** das Code-Feld —
//! nie beides. Während der Anmeldung steht „Im Browser fortfahren …“ mit
//! „Browser erneut öffnen“ und „Abbrechen“, danach „Angemeldet als …“.

use eframe::egui;

use crate::ansichten::{abstand, hinweis, seitenkopf, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::{Dialog, Sicht};
use crate::texte::t;
use crate::thema::{self, Ton};
use quiver_client::{Anmeldewege, Verbindung};

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    bausteine::seite(ui, |ui| {
        hinweis(ui, sicht, aktionen);
        seitenkopf(
            ui,
            sicht,
            &t(sicht.sprache, "nav.verbindung"),
            &t(sicht.sprache, "verbindung.sub"),
        );
        match sicht.modell.verbindung.clone() {
            Verbindung::NichtEingebunden => anmelden(ui, sicht, aktionen),
            Verbindung::WartetAufBrowser { server, adresse } => {
                im_browser(ui, sicht, &server, &adresse, aktionen)
            }
            Verbindung::Verbindet { server } => verbinden(ui, sicht, &server, aktionen),
            Verbindung::Verbunden { .. } | Verbindung::Getrennt { .. } => {
                status(ui, sicht, aktionen)
            }
            Verbindung::NeuAnmelden { server, text } => {
                neu_anmelden(ui, sicht, &server, &text, aktionen)
            }
            Verbindung::AnmeldeFehler { server, text } => {
                anmeldefehler(ui, sicht, &server, &text, aktionen)
            }
        }
        abstand(ui);
        hintergrund(ui, sicht);
        abstand(ui);
        let f = *sicht.thema.farben();
        ui.add(
            egui::Label::new(text(&f, t(sicht.sprache, "verbindung.fussnote"), Ton::Blass).small())
                .wrap(),
        );
    });
}

/// Hintergrundbetrieb: was das Schließen des Fensters bedeutet und ob quiver
/// beim Anmelden startet. Der Haken zeigt den Stand des Systems — die eine
/// Wahrheit liegt in der Autostart-Datei, nicht im Fenster.
fn hintergrund(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut an = sicht.autostart;
    bausteine::karte(
        ui,
        &f,
        &t(sprache, "autostart.titel"),
        Some(&t(sprache, "autostart.sub")),
        |ui| {
            ui.add_space(thema::SPACE_3);
            let haken = bausteine::haken(ui, &mut an, &t(sprache, "autostart.haken"));
            if haken.changed() {
                sicht.autostart = an;
            }
            ui.add_space(thema::SPACE_2);
            ui.add(
                egui::Label::new(text(&f, t(sprache, "autostart.hinweis"), Ton::Blass).small())
                    .wrap(),
            );
        },
    );
}

/// Der eingegebene Server, ohne Leerzeichen und ohne Schrägstrich am Ende.
fn eingabe_server(sicht: &Sicht) -> String {
    sicht
        .eingaben
        .server
        .trim()
        .trim_end_matches('/')
        .to_string()
}

/// Die Wege, die der Server für genau diese Adresse genannt hat.
fn wege_fuer<'a>(sicht: &'a Sicht, server: &str) -> Option<&'a Anmeldewege> {
    sicht
        .modell
        .anmeldewege
        .as_ref()
        .filter(|wege| wege.server == server)
}

/// Schritt 1 und 2: Adresse, dann **entweder** Microsoft **oder** Code.
fn anmelden(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let server = eingabe_server(sicht);
    let wege = wege_fuer(sicht, &server).cloned();
    let mut weiter = false;
    let mut microsoft = false;
    let mut einbinden = false;
    let mut andere_adresse = false;
    let mut merker = sicht.modell.anmelde_grund;

    bausteine::karte(
        ui,
        &f,
        &t(sprache, "verbindung.anmelden_titel"),
        Some(&t(sprache, "verbindung.anmelden_sub")),
        |ui| {
            ui.add_space(thema::SPACE_2);
            // Die Adresse steht immer da — vorbelegt mit der zuletzt
            // benutzten, auch nach dem Abmelden.
            let adresse = bausteine::feld(
                ui,
                &f,
                &t(sprache, "verbindung.server"),
                Some(&t(sprache, "verbindung.server_hinweis")),
                |ui| {
                    ui.add(
                        bausteine::eingabefeld(&mut sicht.eingaben.server)
                            .hint_text("https://quiver.firma.de")
                            .desired_width(bausteine::feldbreite(ui)),
                    )
                },
            );
            ui.add_space(thema::SPACE_2);
            match &wege {
                None => {
                    let knopf = bausteine::hauptknopf_mit(
                        ui,
                        &f,
                        Icon::Stecker,
                        &t(sprache, "verbindung.weiter"),
                    )
                    .clicked();
                    let enter =
                        adresse.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    weiter = knopf || enter;
                }
                Some(wege) if wege.entra => {
                    microsoft = bausteine::hauptknopf_mit(
                        ui,
                        &f,
                        Icon::Extern,
                        &t(sprache, "verbindung.mit_microsoft"),
                    )
                    .clicked();
                    ui.add_space(thema::SPACE_2);
                    ui.add(
                        egui::Label::new(
                            text(&f, t(sprache, "verbindung.entra_hinweis"), Ton::Blass).small(),
                        )
                        .wrap(),
                    );
                }
                Some(wege) if wege.code => {
                    let code = bausteine::feld(
                        ui,
                        &f,
                        &t(sprache, "verbindung.code"),
                        Some(&t(sprache, "verbindung.code_hinweis")),
                        |ui| {
                            ui.add(
                                bausteine::eingabefeld(&mut sicht.eingaben.code)
                                    .hint_text("K7QM-3XPA")
                                    .desired_width(bausteine::feldbreite(ui)),
                            )
                        },
                    );
                    ui.add_space(thema::SPACE_2);
                    einbinden = bausteine::hauptknopf_mit(
                        ui,
                        &f,
                        Icon::Stecker,
                        &t(sprache, "verbindung.einbinden"),
                    )
                    .clicked();
                    if code.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        einbinden = true;
                    }
                }
                Some(_) => {
                    ui.add(
                        egui::Label::new(text(&f, t(sprache, "verbindung.kein_weg"), Ton::Fehler))
                            .wrap(),
                    );
                }
            }
            if wege.is_some() {
                ui.add_space(thema::SPACE_3);
                andere_adresse = bausteine::knopf(
                    ui,
                    &f,
                    Icon::Bearbeiten,
                    &t(sprache, "verbindung.adresse_aendern"),
                )
                .clicked();
            }
            // Ein Ablehnungsgrund gehört hierher, nicht in eine allgemeine
            // Zeile oben: der User soll wissen, was zu tun ist.
            if let Some(grund) = merker.take() {
                ui.add_space(thema::SPACE_3);
                ui.add(
                    egui::Label::new(text(&f, logik::grund_text(sprache, grund), Ton::Fehler))
                        .wrap(),
                );
            }
        },
    );

    if andere_adresse {
        sicht.eingaben.server.clear();
        sicht.eingaben.code.clear();
    }
    if weiter && !server.is_empty() {
        aktionen.push(Aktion::Anmeldewege {
            server: server.clone(),
        });
    }
    if microsoft {
        aktionen.push(Aktion::AnmeldenMicrosoft {
            server: server.clone(),
        });
    }
    if einbinden {
        let code = sicht.eingaben.code.trim().to_string();
        if !code.is_empty() {
            aktionen.push(Aktion::Einbinden { server, code });
            sicht.eingaben.code.clear();
        }
    }
}

/// Der Browser ist offen: warten, erneut öffnen oder abbrechen.
fn im_browser(
    ui: &mut egui::Ui,
    sicht: &mut Sicht,
    server: &str,
    adresse: &str,
    aktionen: &mut Vec<Aktion>,
) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut erneut = false;
    let mut abbrechen = false;
    bausteine::karte(ui, &f, &t(sprache, "verbindung.status_titel"), None, |ui| {
        ui.horizontal(|ui| {
            bausteine::status(
                ui,
                &f,
                Ton::Warnung,
                &t(sprache, "verbindung.im_browser_kurz"),
            );
            ui.spinner();
        });
        ui.add_space(thema::SPACE_2);
        ui.add(
            egui::Label::new(text(
                &f,
                crate::texte::tf(sprache, "verbindung.im_browser_text", &[&server]),
                Ton::Leise,
            ))
            .wrap(),
        );
        ui.add_space(thema::SPACE_4);
        ui.horizontal(|ui| {
            erneut = bausteine::knopf(
                ui,
                &f,
                Icon::Extern,
                &t(sprache, "verbindung.browser_erneut"),
            )
            .clicked();
            abbrechen = bausteine::knopf(
                ui,
                &f,
                Icon::Schliessen,
                &t(sprache, "verbindung.abbrechen"),
            )
            .clicked();
        });
    });
    if erneut {
        aktionen.push(Aktion::BrowserOeffnen(adresse.to_string()));
    }
    if abbrechen {
        aktionen.push(Aktion::AnmeldungAbbrechen);
    }
}

/// Die Anmeldung läuft, aber der Browser ist noch nicht offen.
fn verbinden(ui: &mut egui::Ui, sicht: &mut Sicht, server: &str, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut abbrechen = false;
    bausteine::karte(ui, &f, &t(sprache, "verbindung.status_titel"), None, |ui| {
        ui.horizontal(|ui| {
            bausteine::status(
                ui,
                &f,
                Ton::Warnung,
                &t(sprache, "verbindung.verbindet_kurz"),
            );
            ui.spinner();
        });
        ui.add_space(thema::SPACE_2);
        ui.add(
            egui::Label::new(text(
                &f,
                crate::texte::tf(sprache, "verbindung.verbindet", &[&server]),
                Ton::Leise,
            ))
            .wrap(),
        );
        ui.add_space(thema::SPACE_4);
        abbrechen = bausteine::knopf(
            ui,
            &f,
            Icon::Schliessen,
            &t(sprache, "verbindung.abbrechen"),
        )
        .clicked();
    });
    if abbrechen {
        aktionen.push(Aktion::AnmeldungAbbrechen);
    }
}

/// Verbunden oder getrennt: Wer ist angemeldet, welches Gerät, welcher
/// Endpunkt — und „Abmelden“ mit Rückfrage.
fn status(ui: &mut egui::Ui, sicht: &mut Sicht, _aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let verbindung = sicht.modell.verbindung.clone();
    let (ton, stand) = logik::verbindung_status(sprache, &verbindung);
    let mut abmelden = false;
    bausteine::karte(ui, &f, &t(sprache, "verbindung.status_titel"), None, |ui| {
        ui.horizontal(|ui| {
            bausteine::status(ui, &f, ton, &stand);
            if matches!(verbindung, Verbindung::Verbindet { .. }) {
                ui.spinner();
            }
        });
        ui.add_space(thema::SPACE_3);
        let mut zeilen: Vec<(String, String, bool)> = Vec::new();
        if let Some(server) = verbindung.server() {
            zeilen.push((t(sprache, "verbindung.server"), server.to_string(), true));
        }
        if let Verbindung::Verbunden { geraet, .. } = &verbindung {
            // „Angemeldet als …“: der Benutzer, sonst wenigstens das Gerät.
            let wer = sicht
                .modell
                .benutzer
                .clone()
                .filter(|wert| !wert.trim().is_empty())
                .unwrap_or_else(|| geraet.clone());
            if !wer.is_empty() {
                zeilen.push((t(sprache, "verbindung.angemeldet_als"), wer, false));
            }
            if !geraet.is_empty() {
                zeilen.push((t(sprache, "verbindung.geraet"), geraet.clone(), false));
            }
        }
        zeilen.push((
            t(sprache, "agenten.endpunkt"),
            sicht.modell.endpunkt.clone(),
            true,
        ));
        bausteine::fakten(ui, &f, &zeilen);

        if let Verbindung::Getrennt { fehler, .. } = &verbindung {
            ui.add_space(thema::SPACE_2);
            ui.add(egui::Label::new(text(&f, fehler, Ton::Fehler)).wrap());
            ui.add(
                egui::Label::new(text(
                    &f,
                    t(sprache, "verbindung.getrennt_hinweis"),
                    Ton::Leise,
                ))
                .wrap(),
            );
        }

        ui.add_space(thema::SPACE_4);
        abmelden =
            bausteine::knopf(ui, &f, Icon::Trennen, &t(sprache, "verbindung.trennen")).clicked();
        ui.add_space(thema::SPACE_1);
        ui.add(
            egui::Label::new(
                text(&f, t(sprache, "verbindung.trennen_hinweis"), Ton::Blass).small(),
            )
            .wrap(),
        );
    });
    if abmelden {
        sicht.dialog = Dialog::Abmelden {
            server: verbindung.server().unwrap_or_default().to_string(),
            benutzer: sicht.modell.benutzer.clone(),
        };
    }
}

/// Der Server verlangt eine neue Anmeldung: Token und Angaben bleiben.
fn neu_anmelden(
    ui: &mut egui::Ui,
    sicht: &mut Sicht,
    server: &str,
    meldung: &str,
    aktionen: &mut Vec<Aktion>,
) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let entra = wege_fuer(sicht, server).is_some_and(|wege| wege.entra);
    let mut erneut = false;
    let mut weiter = false;
    bausteine::karte(
        ui,
        &f,
        &t(sprache, "verbindung.neu_anmelden_titel"),
        Some(&t(sprache, "verbindung.neu_anmelden_sub")),
        |ui| {
            ui.add_space(thema::SPACE_2);
            ui.horizontal(|ui| {
                bausteine::status(
                    ui,
                    &f,
                    Ton::Warnung,
                    &t(sprache, "verbindung.neu_anmelden_kurz"),
                );
            });
            ui.add_space(thema::SPACE_2);
            ui.add(egui::Label::new(text(&f, meldung, Ton::Leise)).wrap());
            ui.add_space(thema::SPACE_4);
            erneut = bausteine::hauptknopf_mit(
                ui,
                &f,
                Icon::Stecker,
                &t(sprache, "verbindung.erneut_anmelden"),
            )
            .clicked();
            if !entra {
                weiter = bausteine::knopf(
                    ui,
                    &f,
                    Icon::Bearbeiten,
                    &t(sprache, "verbindung.anmeldeweg_waehlen"),
                )
                .clicked();
            }
        },
    );
    if erneut {
        if entra {
            aktionen.push(Aktion::AnmeldenMicrosoft {
                server: server.to_string(),
            });
        } else {
            aktionen.push(Aktion::Anmeldewege {
                server: server.to_string(),
            });
        }
    }
    if weiter {
        aktionen.push(Aktion::Anmeldewege {
            server: server.to_string(),
        });
    }
}

/// Die Anmeldung ist gescheitert. Der Text sagt, was zu tun ist; die Adresse
/// bleibt vorbelegt und kann geändert werden.
fn anmeldefehler(
    ui: &mut egui::Ui,
    sicht: &mut Sicht,
    server: &str,
    meldung: &str,
    aktionen: &mut Vec<Aktion>,
) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    bausteine::karte(
        ui,
        &f,
        &t(sprache, "verbindung.anmeldefehler_titel"),
        None,
        |ui| {
            ui.add_space(thema::SPACE_2);
            ui.horizontal(|ui| {
                bausteine::icon(ui, &f, Icon::Hinweis, Ton::Fehler);
                ui.add(egui::Label::new(text(&f, meldung, Ton::Fehler)).wrap());
            });
            ui.add_space(thema::SPACE_2);
            ui.add(
                egui::Label::new(text(
                    &f,
                    crate::texte::tf(sprache, "verbindung.anmeldefehler_text", &[&server]),
                    Ton::Leise,
                ))
                .wrap(),
            );
        },
    );
    abstand(ui);
    anmelden(ui, sicht, aktionen);
}
