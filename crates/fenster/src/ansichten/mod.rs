// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Ansichten. Jede ist eine Funktion `(ui, sicht, aktionen)`: sie liest
//! das Modell, zeichnet und legt Aktionen ab — rechnen tut sie nicht.

pub mod agenten;
pub mod dialog;
pub mod kopf;
pub mod protokoll;
pub mod seitenleiste;
pub mod tools;
pub mod update;
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

/// Eine leere Fläche mit mittig stehender Zeile (Protokoll, Aufgabe 18).
pub fn leer_mitte(ui: &mut egui::Ui, sicht: &Sicht, titel: &str, erklaerung: &str) {
    bausteine::leer_mitte(ui, sicht.thema.farben(), titel, erklaerung);
}

/// Der Hinweis des Kerns (Fehler, Rückmeldung), oben auf jeder Seite.
///
/// Ist der Tresor nicht lesbar (Paket 3), steht daneben „Neu beginnen“: der
/// Weg heraus führt über eine Rückfrage, nie über ein stilles Überschreiben.
pub fn hinweis(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<crate::app::Aktion>) {
    let Some(text) = sicht.hinweis.clone() else {
        return;
    };
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let tresor_kaputt = sicht.modell.tresor_fehler.is_some();
    bausteine::kasten(ui, &f, |ui| {
        ui.horizontal(|ui| {
            bausteine::icon(ui, &f, Icon::Hinweis, Ton::Fehler);
            ui.add(egui::Label::new(bausteine::text(&f, &text, Ton::Fehler)).wrap());
            if tresor_kaputt
                && bausteine::knopf(
                    ui,
                    &f,
                    Icon::Neuladen,
                    &crate::texte::t(sprache, "tresor.neu_beginnen"),
                )
                .clicked()
            {
                sicht.dialog = crate::modell::Dialog::TresorNeuBeginnen;
            }
        });
    });
    ui.add_space(crate::thema::SPACE_4);
    let _ = aktionen;
}

/// Das Ergebnis der letzten Aktion (Export oder Import, Aufgabe 15), oben auf
/// jeder Seite. Es steht in der Grundfarbe „gut“ — es ist eine Rückmeldung,
/// kein Fehler.
pub fn ergebnis(ui: &mut egui::Ui, sicht: &Sicht) {
    let Some(text) = &sicht.ergebnis else {
        return;
    };
    let f = sicht.thema.farben();
    bausteine::kasten(ui, f, |ui| {
        ui.horizontal(|ui| {
            bausteine::icon(ui, f, Icon::Haken, Ton::Gut);
            ui.add(egui::Label::new(bausteine::text(f, text, Ton::Gut)).wrap());
        });
    });
    ui.add_space(crate::thema::SPACE_4);
}

/// Ein Abschnitt mit Abstand darunter — Karten stehen nie aneinander. Die
/// 8 px plus den Abstand zwischen zwei Elementen (8 px) ergeben die 16 px,
/// die `.seite` in der Verwaltung zwischen zwei Karten setzt.
pub fn abstand(ui: &mut egui::Ui) {
    ui.add_space(crate::thema::SPACE_2);
}

/// Ein Titel in der fetten Schrift (Karteninhalt, Dialoge).
pub fn fett(inhalt: &str) -> RichText {
    RichText::new(inhalt).font(egui::FontId::new(
        crate::thema::FONT_SIZE_MD,
        crate::thema::fett(),
    ))
}
