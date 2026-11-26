// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Update-Hinweis über der Seite (Aufgabe 34, erweitert in Aufgabe 39):
//! „Version X ist verfügbar“, der Grund, wenn es nur beim Hinweis bleibt, und
//! – wenn ein passendes Paket vorliegt – „Jetzt aktualisieren“ und „Später“.
//!
//! Während des Tauschs steht hier derselbe Fortschritt wie beim Laden der
//! Programme (Ring und Text). Die Sichtbarkeit folgt allein aus dem Modell
//! ([`UpdateLage`]); die Ansicht rechnet nicht und schreibt keinen Text selbst.

use eframe::egui;

use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::Sicht;
use crate::texte::t;
use crate::thema::{self, Ton};
use quiver_client::update::Angebot;
use quiver_client::UpdateLage;

pub fn zeigen(ui: &mut egui::Ui, sicht: &Sicht) -> Vec<Aktion> {
    let mut aktionen = Vec::new();
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let lage = sicht.modell.update.clone();
    let (ton, text, aenderungen) = match &lage {
        UpdateLage::Unbekannt | UpdateLage::Keins => return aktionen,
        UpdateLage::Verfuegbar {
            hinweis,
            aenderungen,
            ..
        } => (Ton::Leise, hinweis.clone(), aenderungen.clone()),
        UpdateLage::Arbeit { .. } | UpdateLage::Getauscht { .. } => {
            (Ton::Leise, String::new(), None)
        }
        UpdateLage::Fehler { text } => (Ton::Fehler, text.clone(), None),
    };

    ui.add_space(thema::SPACE_2);
    bausteine::kasten(ui, &f, |ui| {
        match &lage {
            // Läuft: Ring und Text wie beim Laden der Programme.
            UpdateLage::Arbeit { schritt, anteil } => {
                let text = logik::update_schritt_text(sprache, *schritt, *anteil);
                if ui.style().animation_time > 0.0 {
                    bausteine::status_dreht(ui, &f, Ton::Leise, &text);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                } else {
                    bausteine::status(ui, &f, Ton::Leise, &text);
                }
            }
            UpdateLage::Getauscht { version } => {
                let text = logik::update_getauscht_text(sprache, version);
                bausteine::status_dreht(ui, &f, Ton::Gut, &text);
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
            _ => {
                ui.horizontal_wrapped(|ui| {
                    let icon = match ton {
                        Ton::Fehler => Icon::Hinweis,
                        _ => Icon::Paket,
                    };
                    bausteine::icon(ui, &f, icon, ton);
                    ui.add(egui::Label::new(bausteine::text(&f, &text, ton)).wrap());
                });
                ui.add_space(thema::SPACE_1);
                if let UpdateLage::Verfuegbar { angebot, .. } = &lage {
                    zeigen_angebot(ui, sicht, angebot, &mut aktionen);
                }
                ui.horizontal(|ui| {
                    // Der Link führt zur Veröffentlichung; geöffnet wird er
                    // vom System, nicht von quiver.
                    if let Some(link) = aenderungen.clone() {
                        bausteine::icon(ui, &f, Icon::Extern, Ton::Leise);
                        ui.add(egui::Hyperlink::from_label_and_url(
                            bausteine::text(&f, t(sprache, "update.aenderungen"), Ton::Leise)
                                .small(),
                            link,
                        ));
                    }
                    if bausteine::knopf(ui, &f, Icon::Neuladen, &t(sprache, "update.pruefen"))
                        .clicked()
                    {
                        aktionen.push(Aktion::UpdatePruefen);
                    }
                });
            }
        }
    });
    ui.add_space(thema::SPACE_2);
    aktionen
}

/// Der Knopf „Jetzt aktualisieren“ und der Grund, wenn es keinen gibt.
fn zeigen_angebot(ui: &mut egui::Ui, sicht: &Sicht, angebot: &Angebot, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    match angebot {
        Angebot::Bereit { paket } => {
            ui.horizontal(|ui| {
                if bausteine::knopf(ui, &f, Icon::Herunterladen, &t(sprache, "update.jetzt"))
                    .clicked()
                {
                    aktionen.push(Aktion::UpdateInstallieren);
                }
                if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "update.spaeter"))
                    .clicked()
                {
                    aktionen.push(Aktion::UpdateSpaeter);
                }
                // Was geholt würde, steht daneben – klein und leise.
                ui.add(egui::Label::new(
                    bausteine::text(&f, &paket.name, Ton::Blass).small(),
                ));
            });
        }
        Angebot::Keins { grund } => {
            let text = logik::angebot_grund_text(sprache, *grund);
            ui.add(egui::Label::new(bausteine::text(&f, &text, Ton::Leise).small()).wrap());
        }
    }
}
