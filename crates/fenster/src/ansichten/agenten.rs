// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Agenten: der Endpunkt für alle Agenten, die gekoppelten Agenten mit ihrem
//! Stand und „Agent koppeln" (Token, Endpunkt, fertige Schnipsel).

use eframe::egui::{self, RichText};

use crate::ansichten::{abstand, hinweis, leer, seitenkopf, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::modell::{Dialog, Sicht};
use crate::texte::{t, Sprache};
use crate::thema::{self, Ton};

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut koppeln = false;
    let mut entfernen: Option<(String, String)> = None;
    let mut erneuern: Option<(String, String)> = None;
    let mut schalter: Option<bool> = None;
    let gekoppelte: Vec<String> = sicht
        .modell
        .agenten
        .iter()
        .map(|a| a.name.clone())
        .collect();

    bausteine::seite(ui, |ui| {
        hinweis(ui, sicht);
        seitenkopf(
            ui,
            sicht,
            &t(sprache, "nav.agenten"),
            &t(sprache, "agenten.sub"),
        );

        // Werkzeuge der Seite: die Hauptaktion steht vorn (wie in der Verwaltung).
        koppeln =
            bausteine::hauptknopf_mit(ui, &f, Icon::Plus, &t(sprache, "agenten.koppeln")).clicked();
        abstand(ui);

        bausteine::karte(
            ui,
            &f,
            &t(sprache, "agenten.endpunkt_titel"),
            Some(&t(sprache, "agenten.endpunkt_sub")),
            |ui| {
                bausteine::kopierbar(ui, &f, &sicht.modell.endpunkt, &t(sprache, "allg.kopieren"));
            },
        );
        abstand(ui);

        bausteine::karte(
            ui,
            &f,
            &t(sprache, "agenten.titel"),
            Some(&t(sprache, "agenten.titel_sub")),
            |ui| {
                if sicht.modell.agenten.is_empty() {
                    leer(
                        ui,
                        sicht,
                        &t(sprache, "agenten.leer"),
                        &t(sprache, "agenten.leer_text"),
                    );
                    return;
                }
                for (i, agent) in sicht.modell.agenten.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(thema::SPACE_2);
                    }
                    let (ton, stand) = if agent.verbunden {
                        (Ton::Gut, t(sprache, "agenten.verbunden"))
                    } else {
                        (Ton::Blass, t(sprache, "verbindung.getrennt"))
                    };
                    let mut weg = false;
                    let mut neu = false;
                    egui::Sides::new()
                        .height(thema::SIZE_CONTROL_SM)
                        .shrink_right()
                        .truncate()
                        .show(
                            ui,
                            |ui| {
                                // Punkt, Name und Stand in einer Zeile — so
                                // stehen die Knöpfe rechts auf derselben Höhe.
                                ui.horizontal(|ui| {
                                    bausteine::status_klein(ui, &f, ton, &stand);
                                    ui.add(
                                        egui::Label::new(RichText::new(&agent.name).font(
                                            egui::FontId::new(thema::FONT_SIZE_MD, thema::fett()),
                                        ))
                                        .truncate(),
                                    )
                                    .on_hover_text(&agent.name);
                                    ui.label(text(&f, &stand, Ton::Leise).small());
                                });
                            },
                            |ui| {
                                ui.horizontal(|ui| {
                                    neu = bausteine::knopf(
                                        ui,
                                        &f,
                                        Icon::Neuladen,
                                        &t(sprache, "agenten.token_erneuern"),
                                    )
                                    .clicked();
                                    weg = bausteine::knopf(
                                        ui,
                                        &f,
                                        Icon::Papierkorb,
                                        &t(sprache, "agenten.entfernen"),
                                    )
                                    .clicked();
                                });
                            },
                        );
                    if neu {
                        erneuern = Some((agent.id.clone(), agent.name.clone()));
                    }
                    if weg {
                        entfernen = Some((agent.id.clone(), agent.name.clone()));
                    }
                }
            },
        );
        abstand(ui);

        // Was Agenten vom Protokoll sehen dürfen (Aufgabe 17). Standard: aus.
        let mut an = sicht.modell.protokoll_fuer_agenten;
        bausteine::karte(
            ui,
            &f,
            &t(sprache, "agenten.protokoll_titel"),
            Some(&t(sprache, "agenten.protokoll_sub")),
            |ui| {
                ui.add_space(thema::SPACE_2);
                if bausteine::haken(ui, &mut an, &t(sprache, "agenten.protokoll_haken")).changed() {
                    schalter = Some(an);
                }
            },
        );
    });

    if let Some(an) = schalter {
        aktionen.push(Aktion::ProtokollFuerAgenten { an });
    }

    if koppeln {
        sicht.dialog = Dialog::Koppeln {
            art: crate::logik::erste_freie(&gekoppelte),
            name: String::new(),
        };
    }
    if let Some((id, name)) = erneuern {
        sicht.dialog = Dialog::TokenErneuern { id, name };
    }
    if let Some((id, name)) = entfernen {
        sicht.dialog = Dialog::AgentEntfernen { id, name };
    }
}

/// Was ein Agent zum Anschließen braucht, als Text zum Kopieren. Reine
/// Rechnung — sie steht in [`crate::logik`].
pub fn schnipsel(
    sprache: Sprache,
    art: &crate::logik::AgentArt,
    endpunkt: &str,
    token: &str,
) -> Vec<crate::logik::Schnipsel> {
    crate::logik::schnipsel(sprache, art, endpunkt, token)
}
