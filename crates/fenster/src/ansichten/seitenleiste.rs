// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Navigation links: eine Zeile je Seite, mit Zeichen und Name. Die Ansichten
//! wissen nichts von der Leiste; gewählt wird über das Modell (`Sicht.seite`).

use eframe::egui::{self, Align2, CursorIcon, FontId, Rect, Sense};

use crate::bausteine::{self, Icon};
use crate::modell::{Seite, Sicht};
use crate::texte;
use crate::thema::{self, Farben};

/// Breite der Leiste — dasselbe Maß wie in der Admin-Oberfläche
/// (`--wp-space-8 * 3`).
pub const BREITE: f32 = thema::SPACE_8 * 3.0;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let gewaehlt = sicht.seite;
    let mut wahl = None;
    egui::Panel::left("nav")
        .exact_size(BREITE)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(f.bg_panel)
                .inner_margin(egui::Margin::same(thema::SPACE_2 as i8)),
        )
        .show(ui, |ui| {
            ui.add_space(thema::SPACE_2);
            // Wie die Überschrift der Verwaltung: klein, blass, in Großbuchstaben.
            ui.label(
                bausteine::text(
                    &f,
                    texte::t(sprache, "nav.titel").to_uppercase(),
                    thema::Ton::Blass,
                )
                .font(FontId::proportional(thema::FONT_SIZE_XS)),
            );
            ui.add_space(thema::SPACE_2);
            for seite in Seite::ALLE {
                let name = texte::t(sprache, seite.text_schluessel());
                if eintrag(ui, &f, seite.icon(), &name, seite == gewaehlt).clicked() {
                    wahl = Some(seite);
                }
                ui.add_space(thema::SPACE_1);
            }
        });
    if let Some(seite) = wahl {
        sicht.seite = seite;
    }
}

/// Eine Zeile der Navigation: Zeichen, Name, und bei Auswahl eine ruhige
/// Fläche. Hand-Cursor und Name für den Screenreader kommen von hier
/// (AGENTS.md §2.12).
fn eintrag(
    ui: &mut egui::Ui,
    f: &Farben,
    icon: Icon,
    name: &str,
    gewaehlt: bool,
) -> egui::Response {
    let breite = ui.available_width();
    let (rect, antwort) =
        ui.allocate_exact_size(egui::vec2(breite, thema::SIZE_CONTROL_LG), Sense::click());
    if ui.is_rect_visible(rect) {
        let flaeche = if antwort.hovered() {
            f.bg_hover
        } else if gewaehlt {
            f.bg_selected
        } else {
            egui::Color32::TRANSPARENT
        };
        let (farbe, schrift) = if gewaehlt {
            (f.accent_default, f.text_default)
        } else {
            (f.text_muted, f.text_muted)
        };
        let maler = ui.painter();
        maler.rect_filled(rect, 0.0, flaeche);
        let zeichen = icon.bild(thema::FONT_SIZE_MD, farbe);
        let zeichen_rect = Rect::from_center_size(
            egui::pos2(
                rect.left() + thema::SPACE_2 + thema::FONT_SIZE_MD / 2.0,
                rect.center().y,
            ),
            egui::vec2(thema::FONT_SIZE_MD, thema::FONT_SIZE_MD),
        );
        zeichen.paint_at(ui, zeichen_rect);
        maler.text(
            egui::pos2(zeichen_rect.right() + thema::SPACE_2, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(thema::FONT_SIZE_MD),
            schrift,
        );
    }
    antwort.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    antwort.on_hover_cursor(CursorIcon::PointingHand)
}
