// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Hand-Cursor liegt auf allem Klickbaren (AGENTS.md §2.12). In egui ist
//! das nicht selbstverständlich: ohne `interact_cursor` zeigt der Zeiger auf
//! Knöpfen den Pfeil.

use egui::CursorIcon;
use egui_kittest::kittest::Queryable as _;
use egui_kittest::Harness;
use quiver_oberflaeche::bausteine::{self, Icon};
use quiver_oberflaeche::thema::{self, Thema};

/// Zeiger über das Element mit diesem Namen führen und den Zeiger zurückgeben.
/// `egui` leert `PlatformOutput` am Ende eines Durchlaufs, deshalb steht der
/// Zeiger im Ergebnis des Durchlaufs (`ctx.output` wäre dort schon leer).
fn zeiger_ueber(harness: &mut Harness<'_>, name: &str) -> CursorIcon {
    harness.get_by_label(name).hover();
    harness.run();
    harness.output().platform_output.cursor_icon
}

#[test]
fn hand_ueber_allem_klickbaren() {
    let mut haken = false;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 480.0))
        .with_context_setup(|ctx| {
            quiver_oberflaeche::einrichten(ctx);
            thema::anwenden(ctx, Thema::Dunkel);
        })
        .build_ui(|ui| {
            let f = Thema::Dunkel.farben();
            bausteine::hauptknopf(ui, f, "Gerät einbinden");
            bausteine::knopf(ui, f, Icon::Kopieren, "Kopieren");
            bausteine::icon_knopf(ui, f, Icon::Schluessel, "Geheimnis", false);
            bausteine::haken(ui, &mut haken, "Merken");
        });

    for name in ["Gerät einbinden", "Kopieren", "Geheimnis", "Merken"] {
        assert_eq!(
            zeiger_ueber(&mut harness, name),
            CursorIcon::PointingHand,
            "{name}: kein Hand-Cursor"
        );
    }
}
