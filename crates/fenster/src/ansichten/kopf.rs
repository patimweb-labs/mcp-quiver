// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kopfzeile: Wortmarke und Verbindungsstatus links, Farbschema und Sprache
//! rechts — derselbe Aufbau wie in der Admin-Oberfläche (`Kopf.vue`). Das
//! Muster selbst steht in `bausteine::kopf`, damit beide Welten dasselbe Maß
//! benutzen (AGENTS.md §2.11).
//!
//! Unter `breakpoint.formular` ist für zwei Segmentreihen kein Platz: dann
//! öffnet ein Zeichen beide Auswahlen als Menü (Aufgabe 18).

use eframe::egui;

use crate::bausteine::{self, segmentauswahl, Icon};
use crate::logik;
use crate::modell::Sicht;
use crate::texte::{self, Sprache};
use crate::thema::{self, Thema};

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let (ton, stand) = logik::verbindung_status(sicht.sprache, &sicht.modell.verbindung);
    let farbschemen: Vec<(Thema, String)> = Thema::ALLE
        .iter()
        .map(|wert| (*wert, texte::t(sicht.sprache, wert.text_schluessel())))
        .collect();
    let sprachen: Vec<(Sprache, String)> = Sprache::ALLE
        .iter()
        .map(|wert| (*wert, wert.anzeige().to_string()))
        .collect();
    let mut neues_thema = sicht.thema;
    let mut neue_sprache = sicht.sprache;
    let kompakt = bausteine::eng(ui);

    bausteine::kopf(
        ui,
        &f,
        |ui| {
            bausteine::marke(ui, &f, sicht.thema);
            ui.add_space(thema::SPACE_3);
            // Im schmalen Fenster wird der Stand gekürzt, statt umzubrechen.
            if kompakt {
                bausteine::status_gekuerzt(ui, &f, ton, &stand);
            } else {
                bausteine::status(ui, &f, ton, &stand);
            }
        },
        |ui| {
            // Rechts steht zuerst, was am weitesten rechts steht: egui füllt
            // diese Seite von rechts nach links. Also erst die Sprache,
            // daneben das Farbschema — wie „Abmelden" in der Verwaltung.
            ui.spacing_mut().item_spacing.x = thema::SPACE_3;
            if kompakt {
                let antwort = bausteine::icon_knopf(
                    ui,
                    &f,
                    Icon::Einstellungen,
                    &texte::t(sprache, "kopf.ansicht"),
                    false,
                );
                egui::Popup::menu(&antwort).show(|ui| {
                    ui.set_min_width(thema::SPACE_8 * 3.0);
                    ui.label(bausteine::text(
                        &f,
                        texte::t(sprache, "kopf.farbschema"),
                        thema::Ton::Leise,
                    ));
                    for (wert, name) in &farbschemen {
                        if ui
                            .selectable_label(neues_thema == *wert, name)
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            neues_thema = *wert;
                        }
                    }
                    ui.separator();
                    ui.label(bausteine::text(
                        &f,
                        texte::t(sprache, "kopf.sprache"),
                        thema::Ton::Leise,
                    ));
                    for (wert, name) in &sprachen {
                        if ui
                            .selectable_label(neue_sprache == *wert, name)
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            neue_sprache = *wert;
                        }
                    }
                });
            } else {
                segmentauswahl(ui, &f, &mut neue_sprache, &sprachen);
                segmentauswahl(ui, &f, &mut neues_thema, &farbschemen);
            }
        },
    );

    sicht.thema = neues_thema;
    sicht.sprache = neue_sprache;
}
