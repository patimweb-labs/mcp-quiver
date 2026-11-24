// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Navigation links: eine Zeile je Seite, mit Zeichen und Name. Die Ansichten
//! wissen nichts von der Leiste; gewählt wird über das Modell (`Sicht.seite`).
//! Das Muster steht in `bausteine::seitenleiste` — dieselben Maße wie in der
//! Verwaltung (`Seitenleiste.vue`).

use eframe::egui;

use crate::bausteine::{self, NavEintrag};
use crate::modell::{Seite, Sicht};
use crate::texte;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let gewaehlt = sicht.seite;
    let namen: Vec<String> = Seite::ALLE
        .iter()
        .map(|seite| texte::t(sprache, seite.text_schluessel()))
        .collect();
    let eintraege: Vec<NavEintrag<'_>> = Seite::ALLE
        .iter()
        .zip(&namen)
        .map(|(seite, name)| NavEintrag {
            icon: seite.icon(),
            name,
            gewaehlt: *seite == gewaehlt,
        })
        .collect();
    let titel = texte::t(sprache, "nav.titel");

    if let Some(index) = bausteine::seitenleiste(ui, &f, &titel, &eintraege) {
        sicht.seite = Seite::ALLE[index];
    }
}
