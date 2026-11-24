// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Protokoll: die Werkzeug-Aufrufe der Agenten und die Prozesszeilen der
//! Einträge (Aufgabe 17). Die Seite hat einen eigenen Scrollbereich, damit
//! lange Listen nur im sichtbaren Teil gezeichnet werden (`show_rows`).

use eframe::egui::{self, RichText};

use crate::ansichten::{abstand, hinweis, leer, seitenkopf, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::{Dialog, ProtokollReiter, Sicht};
use crate::texte::{t, Sprache};
use crate::thema::{self, Farben, Ton};
use quiver_client::protokoll::Zeitraum;
use quiver_client::Aufruf;

/// Breite einer Auswahl in der Filterleiste.
const FILTER_BREITE: f32 = thema::SPACE_8 * 3.5;
/// Höhe einer Zeile in der Liste.
const ZEILENHOEHE: f32 = thema::SIZE_CONTROL_SM + thema::SPACE_2;
/// Breite der festen Spalten; den Rest bekommt das Werkzeug.
const SPALTE_ZEIT: f32 = thema::SPACE_8 * 2.0;
const SPALTE_AGENT: f32 = thema::SPACE_8 * 2.0;
const SPALTE_DAUER: f32 = thema::SPACE_8 * 2.0;
const SPALTE_STATUS: f32 = thema::SPACE_8 * 3.0;
/// Zwischenraum der Spalten (fünf Lücken zwischen sechs Spalten).
const LUECKE: f32 = thema::SPACE_2;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, _aktionen: &mut Vec<Aktion>) {
    bausteine::rand(ui, |ui| {
        hinweis(ui, sicht);
        seitenkopf(
            ui,
            sicht,
            &t(sicht.sprache, "nav.protokoll"),
            &t(sicht.sprache, "protokoll.sub"),
        );
        let leeren = reiter(ui, sicht);
        abstand(ui);
        match sicht.protokoll_reiter {
            ProtokollReiter::Aufrufe => {
                filterleiste(ui, sicht);
            }
            ProtokollReiter::ToolLogs => {
                logleiste(ui, sicht);
            }
        }
        // Der Knopf steht über der Liste, damit er nicht unter ihr verschwindet.
        if leeren {
            sicht.dialog = Dialog::ProtokollLeeren;
        }
        abstand(ui);
        match sicht.protokoll_reiter {
            ProtokollReiter::Aufrufe => liste(ui, sicht),
            ProtokollReiter::ToolLogs => logliste(ui, sicht),
        }
    });
}

/// Die zwei Reiter der Seite; rechts daneben „Protokoll leeren“. Gibt zurück,
/// ob geleert werden soll (mit Rückfrage).
fn reiter(ui: &mut egui::Ui, sicht: &mut Sicht) -> bool {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let mut gewaehlt = sicht.protokoll_reiter;
    let mut leeren = false;
    ui.horizontal(|ui| {
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            leeren = bausteine::knopf(ui, &f, Icon::Papierkorb, &t(sprache, "protokoll.leeren"))
                .clicked();
        });
    });
    sicht.protokoll_reiter = gewaehlt;
    leeren
}

/// Eintrag, Werkzeug, Agent, Zeitraum und „nur Fehler“.
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

    let eingaben = &mut sicht.protokoll_eingaben;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = LUECKE;
        auswahl_feld(
            ui,
            &f,
            &t(sprache, "protokoll.filter.eintrag"),
            &t(sprache, "protokoll.filter.alle"),
            &mut eingaben.eintrag,
            &[(String::new(), t(sprache, "protokoll.filter.alle"))]
                .into_iter()
                .chain(
                    eintraege
                        .iter()
                        .map(|eintrag| (eintrag.id.als_str().to_string(), eintrag.titel.clone())),
                )
                .collect::<Vec<_>>(),
        );
        auswahl_feld(
            ui,
            &f,
            &t(sprache, "protokoll.filter.tool"),
            &t(sprache, "protokoll.filter.alle"),
            &mut eingaben.tool,
            &[(String::new(), t(sprache, "protokoll.filter.alle"))]
                .into_iter()
                .chain(werkzeuge.into_iter().map(|tool| (tool.clone(), tool)))
                .collect::<Vec<_>>(),
        );
        auswahl_feld(
            ui,
            &f,
            &t(sprache, "protokoll.filter.agent"),
            &t(sprache, "protokoll.filter.alle"),
            &mut eingaben.agent,
            &[(String::new(), t(sprache, "protokoll.filter.alle"))]
                .into_iter()
                .chain(agenten.into_iter().map(|name| (name.clone(), name.clone())))
                .collect::<Vec<_>>(),
        );
        ui.vertical(|ui| {
            ui.label(text(&f, t(sprache, "protokoll.filter.zeitraum"), Ton::Leise).small());
            let mut zeitraum = eingaben.zeitraum;
            let moeglichkeiten: Vec<(Zeitraum, String)> = Zeitraum::ALLE
                .iter()
                .map(|wert| (*wert, logik::zeitraum_text(sprache, *wert)))
                .collect();
            if bausteine::segmentauswahl(ui, &f, &mut zeitraum, &moeglichkeiten) {
                eingaben.zeitraum = zeitraum;
            }
        });
        bausteine::haken(
            ui,
            &mut eingaben.nur_fehler,
            &t(sprache, "protokoll.filter.nur_fehler"),
        );
    });
}

/// Eine Auswahl mit Beschriftung darüber (Filterleiste). Leer heißt „alle“.
fn auswahl_feld(
    ui: &mut egui::Ui,
    f: &Farben,
    beschriftung: &str,
    platzhalter: &str,
    wert: &mut String,
    optionen: &[(String, String)],
) {
    ui.vertical(|ui| {
        ui.label(text(f, beschriftung, Ton::Leise).small());
        bausteine::auswahl_mit(ui, f, wert, optionen, platzhalter, FILTER_BREITE);
    });
}

/// Die Liste der Aufrufe — nur der sichtbare Teil wird gezeichnet.
fn liste(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    if sicht.aufrufe.is_empty() {
        leer(
            ui,
            sicht,
            &t(sprache, "protokoll.leer"),
            &t(sprache, "protokoll.leer_text"),
        );
        return;
    }
    kopfzeile(ui, sicht);
    let mut gewaehlt: Option<Aufruf> = None;
    let ausgabe = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, ZEILENHOEHE, sicht.aufrufe.len(), |ui, bereich| {
            for index in bereich {
                let Some(aufruf) = sicht.aufrufe.get(index) else {
                    continue;
                };
                if zeile(ui, &f, sprache, aufruf).clicked() {
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
fn kopfzeile(ui: &mut egui::Ui, sicht: &Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let werkzeug = werkzeug_breite(ui.available_width());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = LUECKE;
        for (breite, schluessel) in [
            (SPALTE_ZEIT, "protokoll.spalte.zeit"),
            (SPALTE_AGENT, "protokoll.spalte.agent"),
            (werkzeug, "protokoll.spalte.tool"),
            (SPALTE_DAUER, "protokoll.spalte.dauer"),
            (SPALTE_STATUS, "protokoll.spalte.status"),
        ] {
            spalte(ui, breite, |ui| {
                ui.add(
                    egui::Label::new(text(&f, t(sprache, schluessel), Ton::Blass).small())
                        .truncate(),
                );
            });
        }
    });
}

/// Wie breit die Werkzeugspalte wird: was nach den festen Spalten bleibt.
fn werkzeug_breite(verfuegbar: f32) -> f32 {
    let fest = SPALTE_ZEIT + SPALTE_AGENT + SPALTE_DAUER + SPALTE_STATUS + 4.0 * LUECKE;
    (verfuegbar - fest).max(thema::SPACE_8 * 2.0)
}

/// Eine Zeile der Liste: Zeit, Agent, Werkzeug, Dauer, Ergebnis. Ein Klick
/// öffnet die Einzelheiten.
fn zeile(ui: &mut egui::Ui, f: &Farben, sprache: Sprache, aufruf: &Aufruf) -> egui::Response {
    let breite = ui.available_width();
    let (rect, antwort) =
        ui.allocate_exact_size(egui::vec2(breite, ZEILENHOEHE), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if antwort.hovered() {
            ui.painter().rect_filled(rect, 0.0, f.bg_hover);
        }
        let mut inhalt = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        inhalt.spacing_mut().item_spacing.x = LUECKE;
        spalte(&mut inhalt, SPALTE_ZEIT, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(logik::zeit_text(&aufruf.zeit))
                        .font(egui::FontId::monospace(thema::FONT_SIZE_SM))
                        .color(f.text_muted),
                )
                .truncate(),
            );
        });
        spalte(&mut inhalt, SPALTE_AGENT, |ui| {
            ui.add(egui::Label::new(RichText::new(&aufruf.agent)).truncate());
        });
        let werkzeug = werkzeug_breite(breite);
        spalte(&mut inhalt, werkzeug, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(&aufruf.tool).font(egui::FontId::monospace(thema::FONT_SIZE_SM)),
                )
                .truncate(),
            );
        });
        spalte(&mut inhalt, SPALTE_DAUER, |ui| {
            ui.add(
                egui::Label::new(text(
                    f,
                    logik::dauer_text(sprache, aufruf.dauer_ms),
                    Ton::Leise,
                ))
                .truncate(),
            );
        });
        spalte(&mut inhalt, SPALTE_STATUS, |ui| {
            bausteine::status(
                ui,
                f,
                logik::status_ton(aufruf.status),
                &logik::status_text(sprache, aufruf.status),
            );
        });
    }
    let name = format!(
        "{} {} {} {}",
        logik::zeit_text(&aufruf.zeit),
        aufruf.agent,
        aufruf.tool,
        logik::status_text(sprache, aufruf.status)
    );
    antwort.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    antwort.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Eine Spalte fester Breite.
fn spalte(ui: &mut egui::Ui, breite: f32, inhalt: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(breite, ZEILENHOEHE),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(breite);
            ui.set_max_width(breite);
            inhalt(ui);
        },
    );
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
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = LUECKE;
        auswahl_feld(
            ui,
            &f,
            &t(sprache, "protokoll.log.eintrag"),
            &t(sprache, "protokoll.filter.alle"),
            &mut gewaehlt,
            &optionen,
        );
        ui.vertical(|ui| {
            ui.label(text(&f, t(sprache, "protokoll.log.suche"), Ton::Leise).small());
            ui.add(
                bausteine::eingabefeld(&mut sicht.protokoll_eingaben.suche)
                    .hint_text(t(sprache, "protokoll.log.suche_hinweis"))
                    .desired_width(FILTER_BREITE),
            );
        });
    });
    if gewaehlt != vorher {
        sicht.log_eintrag = (!gewaehlt.is_empty()).then(|| quiver_domain::EintragId::neu(gewaehlt));
    }
}

/// Die Prozesszeilen in Monoschrift.
fn logliste(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    if sicht.log_eintrag.is_none() || sicht.log_zeilen.is_empty() {
        leer(
            ui,
            sicht,
            &t(sprache, "protokoll.log.leer"),
            &t(sprache, "protokoll.log.leer_text"),
        );
        return;
    }
    let hoehe = thema::FONT_SIZE_SM * thema::FONT_LINE_NORMAL + thema::SPACE_1;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, hoehe, sicht.log_zeilen.len(), |ui, bereich| {
            for index in bereich {
                let Some(zeile) = sicht.log_zeilen.get(index) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = LUECKE;
                    ui.add(
                        egui::Label::new(
                            RichText::new(logik::zeit_text(&zeile.zeit))
                                .font(egui::FontId::monospace(thema::FONT_SIZE_SM))
                                .color(f.text_faint),
                        )
                        .truncate(),
                    );
                    let antwort = ui.add(
                        egui::Label::new(
                            RichText::new(&zeile.text)
                                .font(egui::FontId::monospace(thema::FONT_SIZE_SM)),
                        )
                        .truncate(),
                    );
                    // Gekürzt wird nur die Zeile; der ganze Text steht im Hinweis.
                    antwort.on_hover_text(&zeile.text);
                });
            }
        });
}
