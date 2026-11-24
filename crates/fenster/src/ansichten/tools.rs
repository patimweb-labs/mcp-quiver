// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tools: eine Karte je Eintrag — Titel, Beschreibung, Zustand als Abzeichen,
//! Angaben bearbeiten, Werkzeugliste aufklappen. Leer heißt leer.

use eframe::egui::{self, RichText};

use crate::ansichten::{abstand, ergebnis, hinweis, leer, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::{Dialog, Sicht};
use crate::texte::{t, tf};
use crate::thema::{self, Ton};
use quiver_client::EintragSicht;
use quiver_protocol::EintragZustand;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    let mut exportieren = false;
    let mut importieren = false;
    bausteine::seite(ui, |ui| {
        hinweis(ui, sicht);
        ergebnis(ui, sicht);
        kopfzeile(ui, sicht, &mut exportieren, &mut importieren);
        if sicht.modell.eintraege.is_empty() {
            leer(
                ui,
                sicht,
                &t(sicht.sprache, "tools.leer"),
                &t(sicht.sprache, "tools.leer_text"),
            );
            return;
        }
        for eintrag in sicht.modell.eintraege.clone() {
            karte(ui, sicht, &eintrag, aktionen);
            abstand(ui);
        }
    });
    if exportieren {
        // Der Pfad kommt zuerst: erst dann fragt der Dialog nach dem Passwort.
        if let Some(pfad) = sicht.datei_waehlen(true) {
            sicht.dialog = Dialog::Export {
                pfad,
                geheimnisse: false,
                passwort: String::new(),
                wiederholung: String::new(),
            };
        }
    }
    if importieren {
        if let Some(pfad) = sicht.datei_waehlen(false) {
            // Der Kern liest und prüft; die Vorschau kommt als Meldung zurück
            // und öffnet den Dialog ([`crate::modell::Sicht::melden`]).
            aktionen.push(Aktion::EinstellungenPruefen { pfad });
        }
    }
}

/// Die Kopfzeile der Seite: Titel und Unterzeile links, „Exportieren“ und
/// „Importieren“ rechts (Aufgabe 15).
fn kopfzeile(ui: &mut egui::Ui, sicht: &Sicht, exportieren: &mut bool, importieren: &mut bool) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    ui.add_space(thema::SPACE_4);
    egui::Sides::new().shrink_left().truncate().show(
        ui,
        |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = thema::SPACE_1;
                ui.label(
                    RichText::new(t(sprache, "nav.tools"))
                        .font(egui::FontId::new(thema::FONT_SIZE_2XL, thema::fett())),
                );
                ui.label(text(&f, t(sprache, "tools.sub"), Ton::Leise));
            });
        },
        |ui| {
            if bausteine::knopf(
                ui,
                &f,
                Icon::Herunterladen,
                &t(sprache, "einstellungen.exportieren"),
            )
            .clicked()
            {
                *exportieren = true;
            }
            if bausteine::knopf(
                ui,
                &f,
                Icon::Hochladen,
                &t(sprache, "einstellungen.importieren"),
            )
            .clicked()
            {
                *importieren = true;
            }
        },
    );
    ui.add_space(thema::SPACE_5);
}

fn karte(ui: &mut egui::Ui, sicht: &mut Sicht, eintrag: &EintragSicht, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let ton = logik::zustand_ton(&eintrag.zustand);
    let stand = logik::zustand_text(sprache, &eintrag.zustand);
    let fehler = logik::fehlertext(&eintrag.zustand).map(str::to_string);
    let wartet = matches!(eintrag.zustand, EintragZustand::WartetAufWerte { .. });
    let unter = (!eintrag.beschreibung.is_empty()).then(|| eintrag.beschreibung.clone());
    let mut angaben = false;
    let mut bearbeiten = false;
    let mut aufklappen = false;
    let mut anmelden = false;
    let mut abmelden = false;
    let mut einrichten = false;
    // Zugänge (Aufgabe 14): aufklappen, anlegen, bearbeiten, löschen.
    let mut zugaenge_aufklappen = false;
    let mut zugang_neu = false;
    let mut zugang_bearbeiten: Option<String> = None;
    let mut zugang_loeschen: Option<String> = None;
    let offen = sicht.offen.contains(&eintrag.id);
    let zugaenge_offen = sicht.zugaenge_offen.contains(&eintrag.id);

    bausteine::karte(ui, &f, &eintrag.titel, unter.as_deref(), |ui| {
        egui::Sides::new()
            .height(thema::SIZE_CONTROL_SM)
            .shrink_right()
            .truncate()
            .show(
                ui,
                |ui| {
                    bausteine::status(ui, &f, ton, &stand);
                },
                |ui| {
                    // Der Stift steht auf jeder Karte an derselben Stelle:
                    // ganz rechts.
                    if bausteine::icon_knopf(
                        ui,
                        &f,
                        Icon::Bearbeiten,
                        &t(sprache, "tools.bearbeiten"),
                        false,
                    )
                    .clicked()
                    {
                        bearbeiten = true;
                    }
                    if wartet
                        && bausteine::hauptknopf_mit(
                            ui,
                            &f,
                            Icon::Schluessel,
                            &t(sprache, "tools.wartet"),
                        )
                        .clicked()
                    {
                        angaben = true;
                    }
                },
            );

        if let Some(fehler) = fehler {
            ui.add_space(thema::SPACE_1);
            ui.add(egui::Label::new(text(&f, fehler, Ton::Fehler)).wrap());
        }
        // Die Anmeldung (Aufgabe 10): ein Tool kann eine Anmeldung des Users
        // brauchen. Sie wird hier im Fenster ausgelöst, nie von einem Agenten.
        if let Some(zustand) = &eintrag.anmeldung {
            ui.add_space(thema::SPACE_2);
            let zeile = logik::anmeldung_zeile(sprache, zustand);
            egui::Sides::new()
                .height(thema::SIZE_CONTROL_SM)
                .shrink_right()
                .truncate()
                .show(
                    ui,
                    |ui| {
                        bausteine::status(ui, &f, zeile.ton, &zeile.text);
                    },
                    |ui| match zeile.knopf {
                        logik::AnmeldeKnopf::Keiner => {}
                        logik::AnmeldeKnopf::Anmelden => {
                            if bausteine::hauptknopf_mit(
                                ui,
                                &f,
                                Icon::Schluessel,
                                &t(sprache, "anmeldung.anmelden"),
                            )
                            .clicked()
                            {
                                anmelden = true;
                            }
                        }
                        logik::AnmeldeKnopf::Erneut => {
                            if bausteine::hauptknopf_mit(
                                ui,
                                &f,
                                Icon::Neuladen,
                                &t(sprache, "anmeldung.erneut"),
                            )
                            .clicked()
                            {
                                anmelden = true;
                            }
                        }
                        logik::AnmeldeKnopf::Abmelden => {
                            if bausteine::knopf(
                                ui,
                                &f,
                                Icon::Trennen,
                                &t(sprache, "anmeldung.abmelden"),
                            )
                            .clicked()
                            {
                                abmelden = true;
                            }
                        }
                    },
                );
        }
        // Die Einrichtung (Aufgabe 11): Tools verwalten Zugänge in Profilen.
        // Eingerichtet wird auf der Seite des Tools selbst — „Einrichten“ ist
        // ein normaler Knopf, kein Hauptknopf.
        if let Some(zustand) = &eintrag.einrichtung {
            ui.add_space(thema::SPACE_2);
            let zeile = logik::einrichtung_zeile(sprache, zustand);
            egui::Sides::new()
                .height(thema::SIZE_CONTROL_SM)
                .shrink_right()
                .truncate()
                .show(
                    ui,
                    |ui| {
                        bausteine::status(ui, &f, zeile.ton, &zeile.text);
                    },
                    |ui| match zeile.knopf {
                        logik::EinrichtKnopf::Keiner => {}
                        logik::EinrichtKnopf::Einrichten => {
                            if bausteine::knopf(
                                ui,
                                &f,
                                Icon::Einstellungen,
                                &t(sprache, "einrichtung.einrichten"),
                            )
                            .clicked()
                            {
                                einrichten = true;
                            }
                        }
                        logik::EinrichtKnopf::Erneut => {
                            if bausteine::knopf(
                                ui,
                                &f,
                                Icon::Neuladen,
                                &t(sprache, "einrichtung.erneut"),
                            )
                            .clicked()
                            {
                                einrichten = true;
                            }
                        }
                    },
                );
        }
        // Mehrere Zugänge (Aufgabe 14): Zeile „Verbindungen (2)“, Knopf
        // „Verbindung hinzufügen“ und die aufklappbare Liste. Der Eintrag
        // läuft auch ohne Zugang – das Tool meldet dann selbst „kein Zugang“.
        if let Some(zugaenge) = &eintrag.zugaenge {
            ui.add_space(thema::SPACE_2);
            let kopf = logik::zugang_beschriftung(sprache, zugaenge);
            ui.horizontal(|ui| {
                if bausteine::icon_text_knopf(ui, &f, Icon::Schluessel, &kopf, zugaenge_offen)
                    .clicked()
                {
                    zugaenge_aufklappen = true;
                }
                if bausteine::knopf(
                    ui,
                    &f,
                    Icon::Plus,
                    &logik::zugang_hinzufuegen(sprache, zugaenge),
                )
                .clicked()
                {
                    zugang_neu = true;
                }
            });
            if zugaenge_offen {
                ui.add_space(thema::SPACE_2);
                egui::Frame::new()
                    .fill(f.bg_raised)
                    .inner_margin(egui::Margin::symmetric(
                        thema::SPACE_3 as i8,
                        thema::SPACE_2 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if zugaenge.liste.is_empty() {
                            ui.add(
                                egui::Label::new(
                                    text(&f, t(sprache, "zugaenge.leer"), Ton::Blass).small(),
                                )
                                .wrap(),
                            );
                        }
                        for zugang in &zugaenge.liste {
                            egui::Sides::new()
                                .height(thema::SIZE_CONTROL_SM)
                                .shrink_right()
                                .truncate()
                                .show(
                                    ui,
                                    |ui| {
                                        ui.label(RichText::new(&zugang.name).strong());
                                        let werte =
                                            logik::zugang_werte_zeile(&zugaenge.felder, zugang);
                                        if !werte.is_empty() {
                                            ui.add(
                                                egui::Label::new(
                                                    text(&f, werte, Ton::Leise).small(),
                                                )
                                                .truncate(),
                                            );
                                        }
                                    },
                                    |ui| {
                                        if bausteine::knopf(
                                            ui,
                                            &f,
                                            Icon::Bearbeiten,
                                            &t(sprache, "zugaenge.bearbeiten"),
                                        )
                                        .clicked()
                                        {
                                            zugang_bearbeiten = Some(zugang.name.clone());
                                        }
                                        if bausteine::knopf(
                                            ui,
                                            &f,
                                            Icon::Papierkorb,
                                            &t(sprache, "zugaenge.loeschen"),
                                        )
                                        .clicked()
                                        {
                                            zugang_loeschen = Some(zugang.name.clone());
                                        }
                                    },
                                );
                        }
                    });
            }
        }
        // Was fehlt, mit der Erklärung des Felds — nicht nur der Name.
        let fehlende = logik::fehlende_texte(&eintrag.fehlende);
        if !fehlende.is_empty() {
            ui.add_space(thema::SPACE_1);
            ui.add(
                egui::Label::new(
                    text(&f, tf(sprache, "tools.fehlt", &[&fehlende]), Ton::Warnung).small(),
                )
                .wrap(),
            );
        }

        if !eintrag.tools.is_empty() {
            ui.add_space(thema::SPACE_2);
            let anzahl = eintrag.tools.len();
            let beschriftung = if offen {
                t(sprache, "tools.verbergen")
            } else {
                tf(sprache, "tools.anzeigen", &[&anzahl])
            };
            if bausteine::icon_text_knopf(ui, &f, Icon::Satz, &beschriftung, offen).clicked() {
                aufklappen = true;
            }
            if offen {
                ui.add_space(thema::SPACE_2);
                egui::Frame::new()
                    .fill(f.bg_raised)
                    .inner_margin(egui::Margin::symmetric(
                        thema::SPACE_3 as i8,
                        thema::SPACE_2 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for werkzeug in &eintrag.tools {
                            ui.add(egui::Label::new(RichText::new(werkzeug).monospace()).wrap());
                        }
                    });
            }
        }
    });

    if angaben || bearbeiten {
        sicht.werte_dialog(eintrag);
    }
    if aufklappen {
        sicht.umschalten(&eintrag.id);
    }
    if zugaenge_aufklappen {
        sicht.zugaenge_umschalten(&eintrag.id);
    }
    if zugang_neu {
        sicht.zugang_dialog(eintrag, None);
    }
    if let Some(name) = zugang_bearbeiten {
        sicht.zugang_dialog(eintrag, Some(&name));
    }
    if let Some(name) = zugang_loeschen {
        // Löschen nimmt die Geheimnisse mit – deshalb erst die Rückfrage.
        sicht.dialog = Dialog::ZugangEntfernen {
            eintrag: eintrag.id.clone(),
            name,
            einzahl: eintrag
                .zugaenge
                .as_ref()
                .map(|zugaenge| zugaenge.einzahl.clone())
                .unwrap_or_default(),
        };
    }
    if anmelden {
        aktionen.push(Aktion::EintragAnmelden {
            eintrag: eintrag.id.clone(),
        });
    }
    if einrichten {
        aktionen.push(Aktion::EintragEinrichten {
            eintrag: eintrag.id.clone(),
        });
    }
    if abmelden {
        // Abmelden beendet eine laufende Sitzung – deshalb erst die Rückfrage.
        sicht.dialog = Dialog::AnmeldungBeenden {
            eintrag: eintrag.id.clone(),
            titel: eintrag.titel.clone(),
        };
    }
}
