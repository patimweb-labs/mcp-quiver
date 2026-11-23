// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tools: eine Karte je Eintrag — Titel, Beschreibung, Zustand als Abzeichen,
//! Angaben bearbeiten, Werkzeugliste aufklappen. Leer heißt leer.

use eframe::egui::{self, RichText};

use crate::ansichten::{abstand, hinweis, leer, seitenkopf, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::{Dialog, Sicht};
use crate::texte::{t, tf};
use crate::thema::{self, Ton};
use quiver_client::EintragSicht;
use quiver_protocol::EintragZustand;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    let sprache = sicht.sprache;
    bausteine::seite(ui, |ui| {
        hinweis(ui, sicht);
        seitenkopf(
            ui,
            sicht,
            &t(sprache, "nav.tools"),
            &t(sprache, "tools.sub"),
        );
        if sicht.modell.eintraege.is_empty() {
            leer(
                ui,
                sicht,
                &t(sprache, "tools.leer"),
                &t(sprache, "tools.leer_text"),
            );
            return;
        }
        for eintrag in sicht.modell.eintraege.clone() {
            karte(ui, sicht, &eintrag, aktionen);
            abstand(ui);
        }
    });
}

fn karte(ui: &mut egui::Ui, sicht: &mut Sicht, eintrag: &EintragSicht, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let ton = logik::zustand_ton(&eintrag.zustand);
    let stand = logik::zustand_text(sprache, &eintrag.zustand);
    let fehler = logik::fehlertext(&eintrag.zustand).map(str::to_string);
    let wartet = matches!(eintrag.zustand, EintragZustand::WartetAufWerte { .. });
    let unter = (!eintrag.beschreibung.is_empty()).then(|| eintrag.beschreibung.clone());
    let mut angaben = false;
    let mut bearbeiten = false;
    let mut aufklappen = false;
    let mut anmelden = false;
    let mut abmelden = false;
    let offen = sicht.offen.contains(&eintrag.id);

    bausteine::karte(ui, &f, &eintrag.titel, unter.as_deref(), |ui| {
        egui::Sides::new()
            .height(thema::SIZE_CONTROL_SM)
            .shrink_right()
            .truncate()
            .show(
                ui,
                |ui| {
                    bausteine::status(ui, &f, ton, &stand);
                },
                |ui| {
                    // Der Stift steht auf jeder Karte an derselben Stelle:
                    // ganz rechts.
                    if bausteine::icon_knopf(
                        ui,
                        &f,
                        Icon::Bearbeiten,
                        &t(sprache, "tools.bearbeiten"),
                        false,
                    )
                    .clicked()
                    {
                        bearbeiten = true;
                    }
                    if wartet
                        && bausteine::hauptknopf_mit(
                            ui,
                            &f,
                            Icon::Schluessel,
                            &t(sprache, "tools.wartet"),
                        )
                        .clicked()
                    {
                        angaben = true;
                    }
                },
            );

        if let Some(fehler) = fehler {
            ui.add_space(thema::SPACE_1);
            ui.add(egui::Label::new(text(&f, fehler, Ton::Fehler)).wrap());
        }
        // Die Anmeldung (Aufgabe 10): ein Tool kann eine Anmeldung des Users
        // brauchen. Sie wird hier im Fenster ausgelöst, nie von einem Agenten.
        if let Some(zustand) = &eintrag.anmeldung {
            ui.add_space(thema::SPACE_2);
            let zeile = logik::anmeldung_zeile(sprache, zustand);
            egui::Sides::new()
                .height(thema::SIZE_CONTROL_SM)
                .shrink_right()
                .truncate()
                .show(
                    ui,
                    |ui| {
                        bausteine::status(ui, &f, zeile.ton, &zeile.text);
                    },
                    |ui| match zeile.knopf {
                        logik::AnmeldeKnopf::Keiner => {}
                        logik::AnmeldeKnopf::Anmelden => {
                            if bausteine::hauptknopf_mit(
                                ui,
                                &f,
                                Icon::Schluessel,
                                &t(sprache, "anmeldung.anmelden"),
                            )
                            .clicked()
                            {
                                anmelden = true;
                            }
                        }
                        logik::AnmeldeKnopf::Erneut => {
                            if bausteine::hauptknopf_mit(
                                ui,
                                &f,
                                Icon::Neuladen,
                                &t(sprache, "anmeldung.erneut"),
                            )
                            .clicked()
                            {
                                anmelden = true;
                            }
                        }
                        logik::AnmeldeKnopf::Abmelden => {
                            if bausteine::knopf(
                                ui,
                                &f,
                                Icon::Trennen,
                                &t(sprache, "anmeldung.abmelden"),
                            )
                            .clicked()
                            {
                                abmelden = true;
                            }
                        }
                    },
                );
        }
        // Was fehlt, mit der Erklärung des Felds — nicht nur der Name.
        let fehlende = logik::fehlende_texte(&eintrag.fehlende);
        if !fehlende.is_empty() {
            ui.add_space(thema::SPACE_1);
            ui.add(
                egui::Label::new(
                    text(&f, tf(sprache, "tools.fehlt", &[&fehlende]), Ton::Warnung).small(),
                )
                .wrap(),
            );
        }

        if !eintrag.tools.is_empty() {
            ui.add_space(thema::SPACE_2);
            let anzahl = eintrag.tools.len();
            let beschriftung = if offen {
                t(sprache, "tools.verbergen")
            } else {
                tf(sprache, "tools.anzeigen", &[&anzahl])
            };
            if bausteine::icon_text_knopf(ui, &f, Icon::Satz, &beschriftung, offen).clicked() {
                aufklappen = true;
            }
            if offen {
                ui.add_space(thema::SPACE_2);
                egui::Frame::new()
                    .fill(f.bg_raised)
                    .inner_margin(egui::Margin::symmetric(
                        thema::SPACE_3 as i8,
                        thema::SPACE_2 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for werkzeug in &eintrag.tools {
                            ui.add(egui::Label::new(RichText::new(werkzeug).monospace()).wrap());
                        }
                    });
            }
        }
    });

    if angaben || bearbeiten {
        sicht.werte_dialog(eintrag);
    }
    if aufklappen {
        sicht.umschalten(&eintrag.id);
    }
    if anmelden {
        aktionen.push(Aktion::EintragAnmelden {
            eintrag: eintrag.id.clone(),
        });
    }
    if abmelden {
        // Abmelden beendet eine laufende Sitzung – deshalb erst die Rückfrage.
        sicht.dialog = Dialog::AnmeldungBeenden {
            eintrag: eintrag.id.clone(),
            titel: eintrag.titel.clone(),
        };
    }
}
