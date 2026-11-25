// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Protokoll: die Werkzeug-Aufrufe der Agenten und die Prozesszeilen der
//! Einträge (Aufgabe 17). Die Seite hat einen eigenen Scrollbereich, damit
//! lange Listen nur im sichtbaren Teil gezeichnet werden.
//!
//! Aufgabe 18 räumt die Seite auf: Filter in **einer** umbrechenden Leiste,
//! eine Tabelle mit festen Spaltenbreiten (unter `breakpoint.formular` ohne
//! Agent und Dauer), Zebra-Streifen, fest stehender Kopf und ein
//! Detail-Dialog mit Blöcken in Monoschrift.

use eframe::egui::{self, RichText};

use crate::ansichten::{abstand, hinweis, leer_mitte, seitenkopf, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::{Dialog, ProtokollReiter, Sicht};
use crate::texte::{t, Sprache};
use crate::thema::{self, Farben, Ton};
use quiver_client::protokoll::Zeitraum;
use quiver_client::Aufruf;

/// Breite einer Auswahl in der Filterleiste.
const FILTER_BREITE: f32 = thema::SPACE_8 * 3.0;
/// Höhe einer Zeile in der Liste — kompakt, mit `SPACE_2`/`SPACE_3`.
const ZEILENHOEHE: f32 = thema::SIZE_CONTROL_SM + thema::SPACE_1;
/// Waagerechtes Polster einer Zelle.
const ZELLE: f32 = thema::SPACE_2;
/// Wunschbreiten der festen Spalten; den Rest bekommt das Werkzeug. Reicht
/// der Platz nicht, schrumpfen die festen Spalten gleichmäßig (Aufgabe 18).
const SPALTE_ZEIT: f32 = thema::SPACE_8 * 1.5;
const SPALTE_STATUS: f32 = thema::SPACE_8 * 1.5;
const SPALTE_AGENT: f32 = thema::SPACE_8 * 1.5;
const SPALTE_DAUER: f32 = thema::SPACE_8 * 1.25;
/// Schmalste Werkzeugspalte.
const SPALTE_TOOL_MIN: f32 = thema::SPACE_8 * 2.0;
/// Zwischenraum der Spalten.
const LUECKE: f32 = thema::SPACE_2;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    bausteine::rand(ui, |ui| {
        hinweis(ui, sicht, aktionen);
        seitenkopf(
            ui,
            sicht,
            &t(sicht.sprache, "nav.protokoll"),
            &t(sicht.sprache, "protokoll.sub"),
        );
        reiter(ui, sicht);
        abstand(ui);
        match sicht.protokoll_reiter {
            ProtokollReiter::Aufrufe => filterleiste(ui, sicht),
            ProtokollReiter::ToolLogs => logleiste(ui, sicht),
        }
        abstand(ui);
        match sicht.protokoll_reiter {
            ProtokollReiter::Aufrufe => liste(ui, sicht),
            ProtokollReiter::ToolLogs => logliste(ui, sicht),
        }
    });
}

/// Die zwei Reiter der Seite.
fn reiter(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut gewaehlt = sicht.protokoll_reiter;
    bausteine::leiste(ui, |ui| {
        for (reiter, schluessel) in [
            (ProtokollReiter::Aufrufe, "protokoll.reiter.aufrufe"),
            (ProtokollReiter::ToolLogs, "protokoll.reiter.logs"),
        ] {
            let jetzt = gewaehlt == reiter;
            if bausteine::icon_text_knopf(ui, &f, Icon::Protokoll, &t(sprache, schluessel), jetzt)
                .clicked()
            {
                gewaehlt = reiter;
            }
        }
    });
    sicht.protokoll_reiter = gewaehlt;
}

/// Eintrag, Werkzeug, Agent, Zeitraum, „nur Fehler“ und rechts „Protokoll
/// leeren“ — eine Leiste, die umbricht statt seitlich hinauszuragen.
fn filterleiste(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let eintraege = sicht.modell.eintraege.clone();
    let agenten: Vec<String> = sicht
        .modell
        .agenten
        .iter()
        .map(|agent| agent.name.clone())
        .collect();

    // Die Werkzeuge gehören zum gewählten Eintrag; ohne Wahl zu allen.
    let mut werkzeuge: Vec<String> = eintraege
        .iter()
        .filter(|eintrag| {
            sicht.protokoll_eingaben.eintrag.is_empty()
                || eintrag.id.als_str() == sicht.protokoll_eingaben.eintrag
        })
        .flat_map(|eintrag| {
            eintrag
                .tools
                .iter()
                .map(|tool| quiver_domain::tool_name(&eintrag.id, tool))
        })
        .collect();
    werkzeuge.sort();
    werkzeuge.dedup();

    let leeren_text = t(sprache, "protokoll.leeren");
    let leeren_breite = bausteine::knopfbreite(ui, &leeren_text);
    let mut leeren = false;
    let eingaben = &mut sicht.protokoll_eingaben;

    // Drei Auswahlen passen in eine Zeile, solange der Platz reicht — die
    // Breite folgt dem Fenster, statt hinauszuragen.
    let filter_breite = filterbreite(ui);
    let zeitraum_namen: Vec<String> = Zeitraum::ALLE
        .iter()
        .map(|wert| logik::zeitraum_text(sprache, *wert))
        .collect();
    let zeitraum_breite = bausteine::segmentbreite(ui, &zeitraum_namen);
    let haken_breite = bausteine::hakenbreite(ui, &t(sprache, "protokoll.filter.nur_fehler"));

    bausteine::leiste(ui, |ui| {
        bausteine::leistenfeld(
            ui,
            &f,
            Some(&t(sprache, "protokoll.filter.eintrag")),
            filter_breite,
            |ui| {
                bausteine::auswahl_mit(
                    ui,
                    &f,
                    &mut eingaben.eintrag,
                    &[(String::new(), t(sprache, "protokoll.filter.alle"))]
                        .into_iter()
                        .chain(eintraege.iter().map(|eintrag| {
                            (eintrag.id.als_str().to_string(), eintrag.titel.clone())
                        }))
                        .collect::<Vec<_>>(),
                    &t(sprache, "protokoll.filter.alle"),
                    filter_breite,
                );
            },
        );
        bausteine::leistenfeld(
            ui,
            &f,
            Some(&t(sprache, "protokoll.filter.tool")),
            filter_breite,
            |ui| {
                bausteine::auswahl_mit(
                    ui,
                    &f,
                    &mut eingaben.tool,
                    &[(String::new(), t(sprache, "protokoll.filter.alle"))]
                        .into_iter()
                        .chain(werkzeuge.into_iter().map(|tool| (tool.clone(), tool)))
                        .collect::<Vec<_>>(),
                    &t(sprache, "protokoll.filter.alle"),
                    filter_breite,
                );
            },
        );
        bausteine::leistenfeld(
            ui,
            &f,
            Some(&t(sprache, "protokoll.filter.agent")),
            filter_breite,
            |ui| {
                bausteine::auswahl_mit(
                    ui,
                    &f,
                    &mut eingaben.agent,
                    &[(String::new(), t(sprache, "protokoll.filter.alle"))]
                        .into_iter()
                        .chain(agenten.into_iter().map(|name| (name.clone(), name)))
                        .collect::<Vec<_>>(),
                    &t(sprache, "protokoll.filter.alle"),
                    filter_breite,
                );
            },
        );
        // „nur Fehler“ steht in derselben Leiste wie die Filter — nicht in
        // einer eigenen Zeile (Aufgabe 18).
        bausteine::leistenfeld(ui, &f, None, haken_breite, |ui| {
            bausteine::haken(
                ui,
                &mut eingaben.nur_fehler,
                &t(sprache, "protokoll.filter.nur_fehler"),
            );
        });
        bausteine::leistenfeld(
            ui,
            &f,
            Some(&t(sprache, "protokoll.filter.zeitraum")),
            zeitraum_breite,
            |ui| {
                let mut zeitraum = eingaben.zeitraum;
                let moeglichkeiten: Vec<(Zeitraum, String)> = Zeitraum::ALLE
                    .iter()
                    .zip(zeitraum_namen.iter())
                    .map(|(wert, name)| (*wert, name.clone()))
                    .collect();
                if bausteine::segmentauswahl(ui, &f, &mut zeitraum, &moeglichkeiten) {
                    eingaben.zeitraum = zeitraum;
                }
            },
        );
        // „Protokoll leeren“ steht rechts in der Leiste.
        bausteine::nach_rechts(ui, leeren_breite);
        leeren = bausteine::knopf(ui, &f, Icon::Papierkorb, &leeren_text).clicked();
    });
    if leeren {
        sicht.dialog = Dialog::ProtokollLeeren;
    }
}

/// Die Breiten der Spalten einer Zeile — abhängig von der Fensterbreite.
/// Unter `breakpoint.formular` fallen Agent und Dauer weg; sie stehen dann
/// nur im Detail (Aufgabe 18).
struct Spalten {
    zeit: f32,
    status: f32,
    tool: f32,
    agent: Option<f32>,
    dauer: Option<f32>,
}

impl Spalten {
    fn neu(ui: &egui::Ui, verfuegbar: f32) -> Self {
        let sichtbar = logik::protokoll_spalten(ui.ctx().content_rect().width());
        let (agent, dauer) = if sichtbar.agent {
            (Some(SPALTE_AGENT), Some(SPALTE_DAUER))
        } else {
            (None, None)
        };
        let anzahl = 3 + usize::from(agent.is_some()) + usize::from(dauer.is_some());
        let luecken = (anzahl as f32 - 1.0) * LUECKE;
        let platz = verfuegbar - luecken;
        let fest = SPALTE_ZEIT + SPALTE_STATUS + agent.unwrap_or(0.0) + dauer.unwrap_or(0.0);
        // Reicht der Platz nicht, schrumpfen die festen Spalten gleichmäßig;
        // das Werkzeug behält seine Mindestbreite.
        let skala = if fest + SPALTE_TOOL_MIN > platz {
            ((platz - SPALTE_TOOL_MIN) / fest).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let tool = (platz - fest * skala).max(SPALTE_TOOL_MIN);
        Self {
            zeit: SPALTE_ZEIT * skala,
            status: SPALTE_STATUS * skala,
            tool,
            agent: agent.map(|breite| breite * skala),
            dauer: dauer.map(|breite| breite * skala),
        }
    }
}

/// Die Liste der Aufrufe — nur der sichtbare Teil wird gezeichnet.
fn liste(ui: &mut egui::Ui, sicht: &mut Sicht) {
    if sicht.aufrufe.is_empty() {
        leer_mitte(
            ui,
            sicht,
            &t(sicht.sprache, "protokoll.leer"),
            &t(sicht.sprache, "protokoll.leer_text"),
        );
        return;
    }
    let spalten = Spalten::neu(ui, ui.available_width());
    // Der Kopf steht fest über der Liste und schließt mit einer dünnen Linie.
    kopfzeile(ui, sicht, &spalten);
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut gewaehlt: Option<Aufruf> = None;
    let ausgabe = egui::ScrollArea::vertical()
        .id_salt("protokoll-aufrufe")
        .auto_shrink([false, false])
        .show_rows(ui, ZEILENHOEHE, sicht.aufrufe.len(), |ui, bereich| {
            for index in bereich {
                let Some(aufruf) = sicht.aufrufe.get(index) else {
                    continue;
                };
                if zeile(ui, &f, sprache, aufruf, &spalten, index % 2 == 1).clicked() {
                    gewaehlt = Some(aufruf.clone());
                }
            }
        });
    // Hat der User gescrollt? Dann bleiben die Zeilen stehen, statt unter ihm
    // wegzurutschen, wenn oben neue dazukommen.
    sicht.protokoll_gescrollt = ausgabe.state.clamped_offset().y > thema::SPACE_1;
    if let Some(aufruf) = gewaehlt {
        sicht.dialog = Dialog::Aufruf { aufruf };
    }
}

/// Die Spaltenüberschriften über der Liste.
fn kopfzeile(ui: &mut egui::Ui, sicht: &Sicht, spalten: &Spalten) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = LUECKE;
        for (breite, schluessel) in [
            (spalten.zeit, "protokoll.spalte.zeit"),
            (spalten.status, "protokoll.spalte.status"),
            (spalten.tool, "protokoll.spalte.tool"),
            (spalten.agent.unwrap_or_default(), "protokoll.spalte.agent"),
            (spalten.dauer.unwrap_or_default(), "protokoll.spalte.dauer"),
        ] {
            if breite <= 0.0 {
                continue;
            }
            zelle(ui, breite, false, |ui| {
                ui.add(
                    egui::Label::new(text(&f, t(sprache, schluessel), Ton::Blass).small())
                        .truncate(),
                );
            });
        }
    });
    ui.add_space(thema::SPACE_1);
    // Die Trennlinie von Hand: ein `separator` brächte Höhe und Abstand mit,
    // die der Tabelle fehlen (Aufgabe 18).
    let (linie, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), thema::SIZE_BORDER),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(linie, 0.0, f.border_subtle);
}

/// Eine Zeile der Liste: Zeit, Status, Werkzeug, Agent, Dauer. Ein Klick
/// öffnet die Einzelheiten.
fn zeile(
    ui: &mut egui::Ui,
    f: &Farben,
    sprache: Sprache,
    aufruf: &Aufruf,
    spalten: &Spalten,
    zebra: bool,
) -> egui::Response {
    let breite = ui.available_width();
    let (rect, antwort) =
        ui.allocate_exact_size(egui::vec2(breite, ZEILENHOEHE), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if antwort.hovered() {
            ui.painter().rect_filled(rect, 0.0, f.bg_hover);
        } else if zebra {
            ui.painter().rect_filled(rect, 0.0, f.bg_panel);
        }
        let mut inhalt = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        inhalt.spacing_mut().item_spacing.x = LUECKE;
        // Die Zeile ist ein Knopf: ihr Text lässt sich nicht markieren, sonst
        // stünde über der Zeile ein Textcursor statt der Hand (Aufgabe 18).
        inhalt.style_mut().interaction.selectable_labels = false;
        let zeit = logik::zeit_text(&aufruf.zeit);
        zelle(&mut inhalt, spalten.zeit, false, |ui| {
            let antwort = ui.add(
                egui::Label::new(
                    RichText::new(&zeit)
                        .font(egui::FontId::monospace(thema::FONT_SIZE_SM))
                        .color(f.text_muted),
                )
                .truncate(),
            );
            // Das Datum steht im Hinweis — in der Zeile ist kein Platz dafür.
            antwort.on_hover_text(logik::datum_text(&aufruf.zeit));
        });
        // Der Status steht als Abzeichen direkt hinter der Zeit.
        zelle(&mut inhalt, spalten.status, false, |ui| {
            bausteine::status(
                ui,
                f,
                logik::status_ton(aufruf.status),
                &logik::status_text(sprache, aufruf.status),
            );
        });
        zelle(&mut inhalt, spalten.tool, false, |ui| {
            let werkzeug = logik::tool_text(aufruf);
            let antwort = ui.add(
                egui::Label::new(
                    RichText::new(&werkzeug).font(egui::FontId::monospace(thema::FONT_SIZE_SM)),
                )
                .truncate(),
            );
            antwort.on_hover_text(&aufruf.tool);
        });
        if let Some(breite) = spalten.agent {
            zelle(&mut inhalt, breite, false, |ui| {
                ui.add(egui::Label::new(RichText::new(&aufruf.agent)).truncate());
            });
        }
        if let Some(breite) = spalten.dauer {
            // Die Dauer steht rechtsbündig in ihrer Spalte.
            zelle(&mut inhalt, breite, true, |ui| {
                ui.add(
                    egui::Label::new(text(
                        f,
                        logik::dauer_text(sprache, aufruf.dauer_ms),
                        Ton::Leise,
                    ))
                    .truncate(),
                );
            });
        }
    }
    let name = logik::aufruf_zeile(sprache, aufruf);
    antwort.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    antwort.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Eine Zelle fester Breite. `rechts` richtet den Inhalt am rechten Rand aus
/// (die Dauer) — alles andere steht links.
fn zelle(ui: &mut egui::Ui, breite: f32, rechts: bool, inhalt: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(breite, ZEILENHOEHE),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(breite);
            ui.set_max_width(breite);
            // Die Zelle beginnt mit ihrem Polster (wie die Zellen der
            // Verwaltung), damit Text und Abzeichen nicht am Rand kleben.
            let lage = if rechts {
                egui::Layout::right_to_left(egui::Align::Center)
            } else {
                egui::Layout::left_to_right(egui::Align::Center)
            };
            let rand = ui.available_width();
            ui.allocate_ui_with_layout(egui::vec2(rand, ZEILENHOEHE), lage, |ui| {
                ui.add_space(ZELLE);
                inhalt(ui);
            });
        },
    );
}

/// Die Breite einer Auswahl in der Filterleiste: so breit wie gewünscht,
/// aber drei passen immer in eine Zeile (Aufgabe 18).
fn filterbreite(ui: &egui::Ui) -> f32 {
    let platz = ui.available_width() - 2.0 * LUECKE;
    // Etwas schmaler als ein Drittel: Knopf und Rahmen brauchen ein paar
    // Pixel Luft, sonst rutscht die dritte Auswahl doch in die nächste Zeile.
    FILTER_BREITE
        .min(platz / 3.0 - LUECKE)
        .max(thema::SPACE_8 * 1.5)
}

/// Eintrag und Filter für den Reiter „Tool-Logs“.
fn logleiste(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let eintraege = sicht.modell.eintraege.clone();
    // Ohne Wahl steht der erste Eintrag da — sonst gäbe es nichts zu lesen.
    if sicht.log_eintrag.is_none() {
        sicht.log_eintrag = eintraege.first().map(|eintrag| eintrag.id.clone());
    }
    let mut gewaehlt = sicht
        .log_eintrag
        .as_ref()
        .map(|id| id.als_str().to_string())
        .unwrap_or_default();
    let optionen: Vec<(String, String)> = eintraege
        .iter()
        .map(|eintrag| (eintrag.id.als_str().to_string(), eintrag.titel.clone()))
        .collect();
    let vorher = gewaehlt.clone();
    let filter_breite = filterbreite(ui);
    bausteine::leiste(ui, |ui| {
        bausteine::leistenfeld(
            ui,
            &f,
            Some(&t(sprache, "protokoll.log.eintrag")),
            filter_breite,
            |ui| {
                bausteine::auswahl_mit(
                    ui,
                    &f,
                    &mut gewaehlt,
                    &optionen,
                    &t(sprache, "protokoll.filter.alle"),
                    filter_breite,
                );
            },
        );
        bausteine::leistenfeld(
            ui,
            &f,
            Some(&t(sprache, "protokoll.log.suche")),
            filter_breite,
            |ui| {
                ui.add(
                    bausteine::eingabefeld(&mut sicht.protokoll_eingaben.suche)
                        .hint_text(t(sprache, "protokoll.log.suche_hinweis"))
                        .desired_width(filter_breite),
                );
            },
        );
    });
    if gewaehlt != vorher {
        sicht.log_eintrag = (!gewaehlt.is_empty()).then(|| quiver_domain::EintragId::neu(gewaehlt));
    }
}

/// Die Prozesszeilen in Monoschrift: Zeit, Stufe und Text, umbrechend und an
/// das Ende gescrollt, solange der User unten ist.
fn logliste(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    if sicht.log_eintrag.is_none() || sicht.log_zeilen.is_empty() {
        leer_mitte(
            ui,
            sicht,
            &t(sprache, "protokoll.log.leer"),
            &t(sprache, "protokoll.log.leer_text"),
        );
        return;
    }
    // Die Stufe steht in einer eigenen Spalte; wer keine hat, lässt sie leer.
    let stufenbreite = thema::SPACE_8 * 1.5;
    let kopf = t(sprache, "protokoll.log.stufe");
    egui::ScrollArea::vertical()
        .id_salt("protokoll-logs")
        .auto_shrink([false, false])
        // Ans Ende scrollen, solange der User unten ist (Aufgabe 18).
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for zeile in &sicht.log_zeilen {
                let ton = logik::log_ton(&zeile.text);
                let stufe = logik::log_stufe(sprache, &zeile.text);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = LUECKE;
                    ui.add(
                        egui::Label::new(
                            RichText::new(logik::zeit_text(&zeile.zeit))
                                .font(egui::FontId::monospace(thema::FONT_SIZE_SM))
                                .color(f.text_faint),
                        )
                        .truncate(),
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(stufenbreite, thema::FONT_SIZE_SM * thema::FONT_LINE_NORMAL),
                        egui::Layout::left_to_right(egui::Align::Min),
                        |ui| {
                            if let Some(stufe) = stufe {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(stufe)
                                            .font(egui::FontId::monospace(thema::FONT_SIZE_XS))
                                            .color(thema::ton(&f, ton)),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&kopf);
                            }
                        },
                    );
                    // Zeilen brechen um; Fehlerzeilen stehen in der
                    // Fehlerfarbe (Aufgabe 18).
                    ui.add(
                        egui::Label::new(
                            RichText::new(&zeile.text)
                                .font(egui::FontId::monospace(thema::FONT_SIZE_SM))
                                .color(thema::ton(&f, ton)),
                        )
                        .wrap(),
                    );
                });
            }
        });
}
