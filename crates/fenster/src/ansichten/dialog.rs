// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der eine offene Dialog. Was er zeigt, folgt aus [`Dialog`] im Modell;
//! welchen Text er trägt, aus den Texten.

use eframe::egui::{self, RichText};

use crate::ansichten::{agenten, fett, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::modell::{Dialog, Sicht};
use crate::texte::{t, tf};
use crate::thema::{self, Ton};

/// Breite der Dialoge: breit genug für Adresse und Schnipsel, schmal genug
/// für ein kleines Fenster.
const BREITE: f32 = thema::SPACE_8 * 9.0;

pub fn zeigen(ctx: &egui::Context, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    if sicht.dialog == Dialog::Keiner {
        // Der nächste Dialog bekommt den Fokus wieder ins erste Feld.
        ctx.data_mut(|d| d.remove::<bool>(egui::Id::unique("dialog-fokus")));
        return;
    }
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let endpunkt = sicht.modell.endpunkt.clone();
    let breite = BREITE.min(ctx.content_rect().width() - thema::SPACE_8);
    let mut bestaetigen = false;
    let mut abbrechen = false;

    egui::Modal::new(egui::Id::unique("dialog")).show(ctx, |ui| {
        ui.set_width(breite);
        match &mut sicht.dialog {
            Dialog::Keiner => {}
            Dialog::Werte {
                titel, eingaben, ..
            } => {
                ui.label(RichText::new(tf(sprache, "tools.werte_titel", &[titel])).heading());
                ui.add(
                    egui::Label::new(text(&f, t(sprache, "tools.werte_sub"), Ton::Leise)).wrap(),
                );
                ui.add_space(thema::SPACE_3);
                for eingabe in eingaben.iter_mut() {
                    let name = eingabe.name.clone();
                    let geheim = eingabe.geheim;
                    let gesetzt = eingabe.gesetzt;
                    let hinweis = (!eingabe.beschreibung.is_empty()).then(|| {
                        if eingabe.pflicht {
                            format!("{} · {}", eingabe.beschreibung, t(sprache, "tools.pflicht"))
                        } else {
                            eingabe.beschreibung.clone()
                        }
                    });
                    let wert = &mut eingabe.wert;
                    bausteine::feld(ui, &f, &name, hinweis.as_deref(), |ui| {
                        let mut feld = egui::TextEdit::singleline(wert)
                            .desired_width(bausteine::feldbreite(ui));
                        if geheim {
                            feld = feld.password(true);
                            if gesetzt {
                                // Gesetzt heißt: das Feld bleibt leer, der
                                // Hinweis sagt, dass schon etwas da ist.
                                feld = feld.hint_text("••••");
                            }
                        }
                        ui.add(feld)
                    });
                }
                ui.add_space(thema::SPACE_3);
                ui.horizontal(|ui| {
                    if bausteine::hauptknopf_mit(
                        ui,
                        &f,
                        Icon::Haken,
                        &t(sprache, "tools.speichern"),
                    )
                    .clicked()
                    {
                        bestaetigen = true;
                    }
                    if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                        .clicked()
                    {
                        abbrechen = true;
                    }
                });
            }
            Dialog::Koppeln { name } => {
                ui.label(RichText::new(t(sprache, "agenten.koppeln")).heading());
                ui.add(
                    egui::Label::new(text(&f, t(sprache, "agenten.koppeln_text"), Ton::Leise))
                        .wrap(),
                );
                ui.add_space(thema::SPACE_3);
                let feld = bausteine::feld(ui, &f, &t(sprache, "agenten.name"), None, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text("pi")
                            .desired_width(f32::INFINITY),
                    )
                });
                feld.request_focus_once(ui);
                if feld.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    bestaetigen = true;
                }
                ui.add_space(thema::SPACE_3);
                ui.horizontal(|ui| {
                    let leer = name.trim().is_empty();
                    let geklickt = ui
                        .add_enabled_ui(!leer, |ui| {
                            bausteine::hauptknopf_mit(
                                ui,
                                &f,
                                Icon::Plus,
                                &t(sprache, "agenten.koppeln_jetzt"),
                            )
                        })
                        .inner
                        .clicked();
                    if geklickt {
                        bestaetigen = true;
                    }
                    if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                        .clicked()
                    {
                        abbrechen = true;
                    }
                });
            }
            Dialog::Token { name, token } => {
                ui.label(RichText::new(tf(sprache, "agenten.token_titel", &[name])).heading());
                ui.add(
                    egui::Label::new(text(&f, t(sprache, "agenten.hinweis"), Ton::Warnung)).wrap(),
                );
                ui.add_space(thema::SPACE_3);
                ui.label(text(&f, t(sprache, "agenten.endpunkt"), Ton::Leise).small());
                bausteine::kopierbar(ui, &f, &endpunkt, &t(sprache, "allg.kopieren"));
                ui.add_space(thema::SPACE_2);
                ui.label(text(&f, t(sprache, "agenten.token"), Ton::Leise).small());
                bausteine::kopierbar(ui, &f, token, &t(sprache, "allg.kopieren"));
                ui.add_space(thema::SPACE_3);
                for (titel, schnipsel) in agenten::schnipsel(sprache, &endpunkt, token) {
                    ui.label(fett(&titel));
                    bausteine::kopierbar(ui, &f, &schnipsel, &t(sprache, "allg.kopieren"));
                    ui.add_space(thema::SPACE_2);
                }
                ui.add_space(thema::SPACE_1);
                if bausteine::hauptknopf(ui, &f, &t(sprache, "allg.schliessen")).clicked() {
                    bestaetigen = true;
                }
            }
            Dialog::AgentEntfernen { name, .. } => {
                ui.label(RichText::new(t(sprache, "agenten.entfernen_titel")).heading());
                ui.add(
                    egui::Label::new(text(
                        &f,
                        tf(sprache, "agenten.entfernen_text", &[name]),
                        Ton::Leise,
                    ))
                    .wrap(),
                );
                ui.add_space(thema::SPACE_3);
                ui.horizontal(|ui| {
                    if bausteine::knopf(ui, &f, Icon::Papierkorb, &t(sprache, "agenten.entfernen"))
                        .clicked()
                    {
                        bestaetigen = true;
                    }
                    if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                        .clicked()
                    {
                        abbrechen = true;
                    }
                });
            }
        }
        // Escape schließt jeden Dialog.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            abbrechen = true;
        }
    });

    if bestaetigen {
        aktionen.extend(sicht.dialog_bestaetigen());
    } else if abbrechen {
        sicht.dialog = Dialog::Keiner;
    }
}

/// Fokus beim ersten Bild des Dialogs ins erste Feld.
trait FokusEinmal {
    fn request_focus_once(&self, ui: &egui::Ui);
}

impl FokusEinmal for egui::Response {
    fn request_focus_once(&self, ui: &egui::Ui) {
        let id = egui::Id::unique("dialog-fokus");
        let schon = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !schon {
            self.request_focus();
            ui.ctx().data_mut(|d| d.insert_temp(id, true));
        }
    }
}
