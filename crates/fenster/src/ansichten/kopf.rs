// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kopfzeile: Wortmarke und Verbindungsstatus links, Farbschema und Sprache
//! rechts — derselbe Aufbau wie in der Admin-Oberfläche (`Kopf.vue`). Das
//! Muster selbst steht in `bausteine::kopf`, damit beide Welten dasselbe Maß
//! benutzen (AGENTS.md §2.11).

use eframe::egui;

use crate::bausteine::{self, segmentauswahl};
use crate::logik;
use crate::modell::Sicht;
use crate::texte::{self, Sprache};
use crate::thema::{self, Thema};

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

    bausteine::kopf(
        ui,
        &f,
        |ui| {
            bausteine::marke(ui, &f);
            ui.add_space(thema::SPACE_3);
            bausteine::status(ui, &f, ton, &stand);
        },
        |ui| {
            // Rechts steht zuerst, was am weitesten rechts steht: egui füllt
            // diese Seite von rechts nach links. Also erst die Sprache,
            // daneben das Farbschema — wie „Abmelden" in der Verwaltung.
            ui.spacing_mut().item_spacing.x = thema::SPACE_3;
            segmentauswahl(ui, &f, &mut neue_sprache, &sprachen);
            segmentauswahl(ui, &f, &mut neues_thema, &farbschemen);
        },
    );

    sicht.thema = neues_thema;
    sicht.sprache = neue_sprache;
}
