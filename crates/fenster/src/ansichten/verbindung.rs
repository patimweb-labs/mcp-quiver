// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Verbindung: Server-Adresse und Einmal-Code einlösen, danach den Stand
//! zeigen und das Gerät wieder abmelden.

use eframe::egui;

use crate::ansichten::{abstand, hinweis, seitenkopf, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::Sicht;
use crate::texte::t;
use crate::thema::{self, Ton};
use quiver_client::Verbindung;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    bausteine::seite(ui, |ui| {
        hinweis(ui, sicht);
        seitenkopf(
            ui,
            sicht,
            &t(sicht.sprache, "nav.verbindung"),
            &t(sicht.sprache, "verbindung.sub"),
        );
        match sicht.modell.verbindung.clone() {
            Verbindung::NichtEingebunden => einbinden(ui, sicht, aktionen),
            andere => status(ui, sicht, &andere, aktionen),
        }
        abstand(ui);
        ui.add(
            egui::Label::new(text(&f, t(sicht.sprache, "verbindung.fussnote"), Ton::Blass).small())
                .wrap(),
        );
    });
}

/// Das Formular zum Einbinden — die Startseite, solange kein Server da ist.
fn einbinden(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut abschicken = false;
    bausteine::karte(
        ui,
        &f,
        &t(sprache, "verbindung.einbinden"),
        Some(&t(sprache, "verbindung.einbinden.sub")),
        |ui| {
            ui.add_space(thema::SPACE_2);
            bausteine::feld(
                ui,
                &f,
                &t(sprache, "verbindung.server"),
                Some(&t(sprache, "verbindung.server_hinweis")),
                |ui| {
                    ui.add(
                        bausteine::eingabefeld(&mut sicht.eingaben.server)
                            .hint_text("http://quiver.example:8080")
                            .desired_width(bausteine::feldbreite(ui)),
                    )
                },
            );
            let code = bausteine::feld(
                ui,
                &f,
                &t(sprache, "verbindung.code"),
                Some(&t(sprache, "verbindung.code_hinweis")),
                |ui| {
                    ui.add(
                        bausteine::eingabefeld(&mut sicht.eingaben.code)
                            .hint_text("K7QM-3XPA")
                            .desired_width(bausteine::feldbreite(ui)),
                    )
                },
            );
            ui.add_space(thema::SPACE_2);
            abschicken = bausteine::hauptknopf_mit(
                ui,
                &f,
                Icon::Stecker,
                &t(sprache, "verbindung.einbinden"),
            )
            .clicked();
            // Die Eingabetaste im Codefeld schickt ab — wie im Formular.
            if code.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                abschicken = true;
            }
        },
    );
    let server = sicht.eingaben.server.trim().to_string();
    if abschicken && !server.is_empty() && !sicht.eingaben.code.trim().is_empty() {
        aktionen.push(Aktion::Einbinden {
            server,
            code: sicht.eingaben.code.trim().to_string(),
        });
        sicht.eingaben.code.clear();
    }
}

/// Stand und Abmelden, sobald einmal eingebunden wurde.
fn status(
    ui: &mut egui::Ui,
    sicht: &mut Sicht,
    verbindung: &Verbindung,
    aktionen: &mut Vec<Aktion>,
) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let (ton, stand) = logik::verbindung_status(sprache, verbindung);
    let mut abmelden = false;
    bausteine::karte(ui, &f, &t(sprache, "verbindung.status_titel"), None, |ui| {
        ui.horizontal(|ui| {
            bausteine::status(ui, &f, ton, &stand);
            if matches!(verbindung, Verbindung::Verbindet { .. }) {
                ui.spinner();
            }
        });
        ui.add_space(thema::SPACE_3);
        let mut zeilen: Vec<(String, String, bool)> = Vec::new();
        if let Some(server) = server_von(verbindung) {
            zeilen.push((t(sprache, "verbindung.server"), server, true));
        }
        if let Verbindung::Verbunden { geraet, .. } = verbindung {
            if !geraet.is_empty() {
                zeilen.push((t(sprache, "verbindung.geraet"), geraet.clone(), false));
            }
        }
        zeilen.push((
            t(sprache, "agenten.endpunkt"),
            sicht.modell.endpunkt.clone(),
            true,
        ));
        bausteine::fakten(ui, &f, &zeilen);

        if let Verbindung::Getrennt { fehler, .. } = verbindung {
            ui.add_space(thema::SPACE_2);
            ui.add(egui::Label::new(text(&f, fehler, Ton::Fehler)).wrap());
            ui.add(
                egui::Label::new(text(
                    &f,
                    t(sprache, "verbindung.getrennt_hinweis"),
                    Ton::Leise,
                ))
                .wrap(),
            );
        }

        ui.add_space(thema::SPACE_4);
        abmelden =
            bausteine::knopf(ui, &f, Icon::Trennen, &t(sprache, "verbindung.trennen")).clicked();
        ui.add_space(thema::SPACE_1);
        ui.add(
            egui::Label::new(
                text(&f, t(sprache, "verbindung.trennen_hinweis"), Ton::Blass).small(),
            )
            .wrap(),
        );
    });
    if abmelden {
        aktionen.push(Aktion::Abmelden);
    }
}

fn server_von(verbindung: &Verbindung) -> Option<String> {
    match verbindung {
        Verbindung::NichtEingebunden => None,
        Verbindung::Verbindet { server }
        | Verbindung::Verbunden { server, .. }
        | Verbindung::Getrennt { server, .. } => Some(server.clone()),
    }
}
