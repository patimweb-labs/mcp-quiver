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
        hinweis(ui, sicht, aktionen);
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
        // Filter „nur aktive“ (Aufgabe 19): abgeschaltete Karten verschwinden.
        // Die Zeile gibt es nur, wenn es etwas zu filtern gibt – sonst nähme sie
        // den Karten Platz weg (Aufgabe 18).
        if sicht
            .modell
            .eintraege
            .iter()
            .any(|eintrag| eintrag.am_geraet_aus)
        {
            bausteine::leiste(ui, |ui| {
                bausteine::haken(
                    ui,
                    &mut sicht.nur_aktive,
                    &t(sicht.sprache, "tools.nur_aktive"),
                );
            });
            ui.add_space(thema::SPACE_4);
        }
        let sichtbar = logik::sichtbare_eintraege(&sicht.modell.eintraege, sicht.nur_aktive);
        if sichtbar.is_empty() {
            leer(
                ui,
                sicht,
                &t(sicht.sprache, "tools.leer_aus"),
                &t(sicht.sprache, "tools.abgeschaltet_hinweis"),
            );
            return;
        }
        for eintrag in sichtbar {
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
/// „Importieren“ rechts (Aufgabe 15). Ist das Fenster zu schmal für beides
/// nebeneinander, stehen die Knöpfe darunter (Aufgabe 18): die Unterzeile
/// wird dann nicht abgeschnitten.
fn kopfzeile(ui: &mut egui::Ui, sicht: &Sicht, exportieren: &mut bool, importieren: &mut bool) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let export = t(sprache, "einstellungen.exportieren");
    let import = t(sprache, "einstellungen.importieren");
    let knoepfe = bausteine::knopfbreite(ui, &export) + bausteine::knopfbreite(ui, &import);
    // So viel Platz braucht der Titel, damit er lesbar bleibt.
    let titel = thema::SPACE_8 * 5.0;
    let nebeneinander = ui.available_width() >= titel + knoepfe + thema::SPACE_3;
    if nebeneinander {
        ui.add_space(thema::SPACE_4);
        egui::Sides::new().shrink_left().truncate().show(
            ui,
            |ui| titelkopf(ui, &f, sprache),
            |ui| {
                // Die rechte Seite füllt egui von rechts: erst „Importieren“,
                // damit links „Exportieren“ steht — wie in der Leiste darunter.
                if bausteine::knopf(ui, &f, Icon::Hochladen, &import).clicked() {
                    *importieren = true;
                }
                if bausteine::knopf(ui, &f, Icon::Herunterladen, &export).clicked() {
                    *exportieren = true;
                }
            },
        );
    } else {
        bausteine::seitenkopf(ui, &f, &t(sprache, "nav.tools"), &t(sprache, "tools.sub"));
        bausteine::leiste(ui, |ui| {
            if bausteine::knopf(ui, &f, Icon::Herunterladen, &export).clicked() {
                *exportieren = true;
            }
            if bausteine::knopf(ui, &f, Icon::Hochladen, &import).clicked() {
                *importieren = true;
            }
        });
    }
    ui.add_space(thema::SPACE_5);
}

/// Titel und Unterzeile der Seite.
fn titelkopf(ui: &mut egui::Ui, f: &crate::thema::Farben, sprache: crate::texte::Sprache) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = thema::SPACE_1;
        ui.label(
            RichText::new(t(sprache, "nav.tools"))
                .font(egui::FontId::new(thema::FONT_SIZE_2XL, thema::fett())),
        );
        ui.label(text(f, t(sprache, "tools.sub"), Ton::Leise));
    });
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
    let mut abbrechen = false;
    let mut schalten: Option<bool> = None;
    // Abgeschaltet: die Karte ist gedämpft, die Knöpfe sind gesperrt
    // (Aufgabe 19).
    let aus = eintrag.am_geraet_aus;
    let gesperrt = t(sprache, "tools.gesperrt_hinweis");
    // Zugänge (Aufgabe 14): aufklappen, anlegen, bearbeiten, löschen.
    let mut zugaenge_aufklappen = false;
    let mut zugang_neu = false;
    let mut zugang_bearbeiten: Option<String> = None;
    let mut zugang_loeschen: Option<String> = None;
    let offen = sicht.offen.contains(&eintrag.id);
    let zugaenge_offen = sicht.zugaenge_offen.contains(&eintrag.id);

    bausteine::karte_mit(ui, &f, &eintrag.titel, unter.as_deref(), aus, |ui| {
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
        // Eine Kennung, die nicht als Präfix taugt: der Eintrag startet nicht,
        // und das Fenster sagt warum (Paket 3).
        if let Some(grund) = &eintrag.unzulaessig {
            ui.add_space(thema::SPACE_1);
            ui.add(
                egui::Label::new(text(
                    &f,
                    tf(sprache, "tools.kennung_unbrauchbar", &[grund]),
                    Ton::Fehler,
                ))
                .wrap(),
            );
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
                            if kartenknopf(ui, aus, &gesperrt, |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Schluessel,
                                    &t(sprache, "anmeldung.anmelden"),
                                )
                            }) {
                                anmelden = true;
                            }
                        }
                        logik::AnmeldeKnopf::Erneut => {
                            if kartenknopf(ui, aus, &gesperrt, |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Neuladen,
                                    &t(sprache, "anmeldung.erneut"),
                                )
                            }) {
                                anmelden = true;
                            }
                        }
                        logik::AnmeldeKnopf::Abmelden => {
                            if kartenknopf(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Trennen,
                                    &t(sprache, "anmeldung.abmelden"),
                                )
                            }) {
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
                            if kartenknopf(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Einstellungen,
                                    &t(sprache, "einrichtung.einrichten"),
                                )
                            }) {
                                einrichten = true;
                            }
                        }
                        logik::EinrichtKnopf::Erneut => {
                            if kartenknopf(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Neuladen,
                                    &t(sprache, "einrichtung.erneut"),
                                )
                            }) {
                                einrichten = true;
                            }
                        }
                        logik::EinrichtKnopf::Abbrechen => {
                            if kartenknopf(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Schliessen,
                                    &t(sprache, "einrichtung.abbrechen"),
                                )
                            }) {
                                abbrechen = true;
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
                                        // Gekürzt, mit dem ganzen Text im Hinweis.
                                        ui.add(
                                            egui::Label::new(RichText::new(&zugang.name).strong())
                                                .truncate(),
                                        )
                                        .on_hover_text(&zugang.name);
                                        let werte =
                                            logik::zugang_werte_zeile(&zugaenge.felder, zugang);
                                        if !werte.is_empty() {
                                            ui.add(
                                                egui::Label::new(
                                                    text(&f, &werte, Ton::Leise).small(),
                                                )
                                                .truncate(),
                                            )
                                            .on_hover_text(&werte);
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

        // Der Schalter „Auf diesem Gerät aktiv“ steht rechts in der Karte
        // (Aufgabe 19). Standard an – auch für neu zugewiesene Einträge. Er
        // schließt die Karte ab, damit die Zeilen darüber an ihrem Platz
        // bleiben (Aufgabe 18: nichts verrutscht, wenn es enger wird).
        if aus {
            ui.add(
                egui::Label::new(
                    text(&f, t(sprache, "tools.abgeschaltet_hinweis"), Ton::Leise).small(),
                )
                .wrap(),
            );
        }
        ui.add_space(thema::SPACE_2);
        egui::Sides::new()
            .height(thema::SIZE_CONTROL_SM)
            .shrink_left()
            .truncate()
            .show(
                ui,
                |_ui| {},
                |ui| {
                    let mut an = !aus;
                    if bausteine::haken(ui, &mut an, &t(sprache, "tools.aktiv")).changed() {
                        schalten = Some(an);
                    }
                },
            );
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
    if abbrechen {
        aktionen.push(Aktion::EinrichtungAbbrechen {
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
    if let Some(an) = schalten {
        aktionen.push(Aktion::EintragSchalten {
            eintrag: eintrag.id.clone(),
            an,
        });
    }
}

/// Einen Knopf der Karte zeichnen. Ist der Eintrag auf diesem Gerät
/// abgeschaltet, bleibt der Knopf gesperrt und nennt am Zeiger den Grund
/// (Aufgabe 19). `true` heißt: geklickt.
fn kartenknopf(
    ui: &mut egui::Ui,
    aus: bool,
    hinweis: &str,
    knopf: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> bool {
    let antwort = ui.add_enabled_ui(!aus, knopf).inner;
    antwort.on_disabled_hover_text(hinweis).clicked()
}
