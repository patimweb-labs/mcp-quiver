// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kopfzeile: Wortmarke und Verbindungsstatus links, Theme und Sprache
//! rechts — derselbe Aufbau wie in der Admin-Oberfläche (`Kopf.vue`).

use eframe::egui;

use crate::bausteine;
use crate::logik;
use crate::modell::Sicht;
use crate::texte::{self, Sprache};
use crate::thema::{self, Thema};

/// Eine Auswahl auf der rechten Seite der Kopfzeile. Dort wächst egui nach
/// links, also werden die Möglichkeiten umgekehrt gesetzt — so stehen sie in
/// der Reihenfolge der Liste.
fn auswahl_rechts<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    wert: &mut T,
    moeglichkeiten: &[(T, String)],
) -> bool {
    let umgekehrt: Vec<(T, String)> = moeglichkeiten
        .iter()
        .rev()
        .map(|(wert, name)| (*wert, name.clone()))
        .collect();
    bausteine::auswahl(ui, wert, &umgekehrt)
}

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
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

    egui::Panel::top("kopf")
        .frame(
            egui::Frame::new()
                .fill(f.bg_panel)
                .inner_margin(egui::Margin::symmetric(
                    thema::SPACE_4 as i8,
                    thema::SPACE_2 as i8,
                )),
        )
        .show(ui, |ui| {
            egui::Sides::new()
                .height(thema::SIZE_CONTROL_SM)
                .shrink_left()
                .truncate()
                .show(
                    ui,
                    |ui| {
                        bausteine::marke(ui, &f);
                        ui.add_space(thema::SPACE_3);
                        bausteine::status(ui, &f, ton, &stand);
                    },
                    |ui| {
                        // Rechts außen die Sprache, daneben das Farbschema.
                        auswahl_rechts(ui, &mut neue_sprache, &sprachen);
                        ui.add_space(thema::SPACE_2);
                        auswahl_rechts(ui, &mut neues_thema, &farbschemen);
                    },
                );
        });

    sicht.thema = neues_thema;
    sicht.sprache = neue_sprache;
}
