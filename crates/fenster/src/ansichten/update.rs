// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Update-Hinweis über der Seite (Aufgabe 34): „Version X ist verfügbar“
//! und ein Link auf die Änderungen.
//!
//! Mehr nicht: quiver lädt und installiert nichts. Eingespielt wird die neue
//! Fassung durch die Geräteverwaltung (Intune) oder von Hand aus den Paketen
//! des Releases (`docs/intune.md`). Die Sichtbarkeit folgt allein aus dem
//! Modell ([`UpdateLage`]); die Ansicht rechnet nicht.

use eframe::egui;

use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::modell::Sicht;
use crate::texte::t;
use crate::thema::{self, Ton};
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
        UpdateLage::Fehler { text } => (Ton::Fehler, text.clone(), None),
    };

    ui.add_space(thema::SPACE_2);
    bausteine::kasten(ui, &f, |ui| {
        ui.horizontal_wrapped(|ui| {
            let icon = match ton {
                Ton::Fehler => Icon::Hinweis,
                _ => Icon::Paket,
            };
            bausteine::icon(ui, &f, icon, ton);
            ui.add(egui::Label::new(bausteine::text(&f, &text, ton)).wrap());
        });
        ui.add_space(thema::SPACE_1);
        ui.horizontal(|ui| {
            // Der Link führt zur Veröffentlichung; geöffnet wird er vom
            // System, nicht von quiver.
            if let Some(link) = aenderungen {
                bausteine::icon(ui, &f, Icon::Extern, Ton::Leise);
                ui.add(egui::Hyperlink::from_label_and_url(
                    bausteine::text(&f, t(sprache, "update.aenderungen"), Ton::Leise).small(),
                    link,
                ));
            }
            // Fragen darf man immer wieder; installieren kann man hier nicht.
            if bausteine::knopf(ui, &f, Icon::Neuladen, &t(sprache, "update.pruefen")).clicked() {
                aktionen.push(Aktion::UpdatePruefen);
            }
        });
    });
    ui.add_space(thema::SPACE_2);
    aktionen
}
