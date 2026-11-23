// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Hand-Cursor liegt auf allem Klickbaren (AGENTS.md §2.12). In egui ist
//! das nicht selbstverständlich: ohne `interact_cursor` zeigt der Zeiger auf
//! Knöpfen den Pfeil.

use egui::CursorIcon;
use egui_kittest::kittest::Queryable as _;
use egui_kittest::Harness;
use quiver_oberflaeche::bausteine::{self, Icon, NavEintrag};
use quiver_oberflaeche::texte::{self, Sprache};
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
    let mut thema = Thema::Dunkel;
    let farbschemen: Vec<(Thema, String)> = Thema::ALLE
        .iter()
        .map(|wert| (*wert, texte::t(Sprache::De, wert.text_schluessel())))
        .collect();
    let eintraege = [
        NavEintrag {
            icon: Icon::Paket,
            name: "Bibliothek",
            gewaehlt: true,
        },
        NavEintrag {
            icon: Icon::Satz,
            name: "Sätze",
            gewaehlt: false,
        },
        NavEintrag {
            icon: Icon::Geraet,
            name: "Geräte",
            gewaehlt: false,
        },
    ];
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
            bausteine::segmentauswahl(ui, f, &mut thema, &farbschemen);
            bausteine::seitenleiste(ui, f, "Verwaltung", &eintraege);
        });

    for name in [
        // Knöpfe, Häkchen
        "Gerät einbinden",
        "Kopieren",
        "Geheimnis",
        "Merken",
        // Segmentauswahl
        "Dunkel",
        "Hell",
        "Mono",
        // Seitenleiste
        "Bibliothek",
        "Sätze",
        "Geräte",
    ] {
        assert_eq!(
            zeiger_ueber(&mut harness, name),
            CursorIcon::PointingHand,
            "{name}: kein Hand-Cursor"
        );
    }
}
