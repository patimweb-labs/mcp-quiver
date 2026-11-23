// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Ansichten. Jede ist eine Funktion `(ui, sicht, aktionen)`: sie liest
//! das Modell, zeichnet und legt Aktionen ab — rechnen tut sie nicht.

pub mod agenten;
pub mod dialog;
pub mod kopf;
pub mod seitenleiste;
pub mod tools;
pub mod verbindung;

use eframe::egui::{self, RichText};

use crate::bausteine::{self, Icon};
use crate::modell::Sicht;
use crate::thema::Ton;

pub use crate::bausteine::text;

/// Überschrift einer Seite mit Unterzeile.
pub fn seitenkopf(ui: &mut egui::Ui, sicht: &Sicht, titel: &str, unter: &str) {
    bausteine::seitenkopf(ui, sicht.thema.farben(), titel, unter);
}

/// Eine leere Fläche mit Titel und Erklärung.
pub fn leer(ui: &mut egui::Ui, sicht: &Sicht, titel: &str, erklaerung: &str) {
    bausteine::leer(ui, sicht.thema.farben(), titel, erklaerung);
}

/// Der Hinweis des Kerns (Fehler, Rückmeldung), oben auf jeder Seite.
pub fn hinweis(ui: &mut egui::Ui, sicht: &Sicht) {
    let Some(text) = &sicht.hinweis else {
        return;
    };
    let f = sicht.thema.farben();
    bausteine::kasten(ui, f, |ui| {
        ui.horizontal_top(|ui| {
            bausteine::icon(ui, f, Icon::Hinweis, Ton::Fehler);
            ui.add(egui::Label::new(bausteine::text(f, text, Ton::Fehler)).wrap());
        });
    });
    ui.add_space(crate::thema::SPACE_4);
}

/// Ein Abschnitt mit Abstand darunter — Karten stehen nie aneinander.
pub fn abstand(ui: &mut egui::Ui) {
    ui.add_space(crate::thema::SPACE_4);
}

/// Ein Titel in der fetten Schrift (Karteninhalt, Dialoge).
pub fn fett(inhalt: &str) -> RichText {
    RichText::new(inhalt).font(egui::FontId::new(
        crate::thema::FONT_SIZE_MD,
        crate::thema::fett(),
    ))
}
