// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tools: eine Karte je Eintrag — Titel, Beschreibung, Zustand als Abzeichen,
//! Angaben bearbeiten, Werkzeugliste aufklappen. Leer heißt leer.

use eframe::egui::{self, RichText};

use crate::ansichten::{abstand, hinweis, leer, seitenkopf, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::Sicht;
use crate::texte::{t, tf};
use crate::thema::{self, Ton};
use quiver_client::EintragSicht;
use quiver_protocol::EintragZustand;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, _aktionen: &mut Vec<Aktion>) {
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
            karte(ui, sicht, &eintrag);
            abstand(ui);
        }
    });
}

fn karte(ui: &mut egui::Ui, sicht: &mut Sicht, eintrag: &EintragSicht) {
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
}
