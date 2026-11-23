// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Musterseite: dieselbe Seite wie `/muster` in der Verwaltung
//! (`admin/src/ansichten/Muster.vue`) — Kopf, Seitenleiste mit drei
//! Einträgen, Seitenkopf, eine Karte mit Hauptknopf und Knopf, drei
//! Statuszeilen und ein Eingabefeld. Beide Welten bauen sie aus denselben
//! Bausteinen und lesen dieselben Token-Dateien; die Bilder müssen sich
//! deshalb nur in der Schriftglättung unterscheiden (AGENTS.md §2.11).
//!
//! Mit `QUIVER_BILDER=<verzeichnis>` schreibt der Lauf die beiden Bilder
//! `muster-egui-{dark,light}.png` (1200 × 800, wie die Verwaltung).

use std::path::PathBuf;

use egui_kittest::kittest::Queryable as _;
use egui_kittest::Harness;
use image::GenericImageView as _;
use quiver_oberflaeche::bausteine::{self, Icon, NavEintrag};
use quiver_oberflaeche::texte::{self, Sprache};
use quiver_oberflaeche::thema::{self, Thema, Ton};

/// Die Größe des Fensters — dieselbe wie im Bild der Verwaltung.
const GROESSE: (u32, u32) = (1200, 800);

/// Das Prüfgerüst legt um die Fläche einen Rand von 8 px; der wird vor dem
/// Speichern abgeschnitten, damit das Bild die Seite selbst zeigt.
const GERUEST_RAND: u32 = 8;

/// Die drei Einträge der Seitenleiste — wie im Fenster.
const NAVIGATION: [(&str, Icon); 3] = [
    ("Bibliothek", Icon::Paket),
    ("Sätze", Icon::Satz),
    ("Geräte", Icon::Geraet),
];

/// Zustand der Musterseite. In der Verwaltung ist das die Route `/muster`.
pub struct Muster {
    pub thema: Thema,
    pub englisch: bool,
    pub seite: usize,
    pub suche: String,
}

impl Muster {
    fn neu(thema: Thema) -> Self {
        Self {
            thema,
            englisch: false,
            seite: 0,
            suche: String::new(),
        }
    }

    /// Ein Bild: Kopf, Seitenleiste, Seite.
    fn zeigen(&mut self, ui: &mut egui::Ui) {
        let f = *self.thema.farben();
        let farbschemen: Vec<(Thema, String)> = Thema::ALLE
            .iter()
            .map(|wert| (*wert, texte::t(Sprache::De, wert.text_schluessel())))
            .collect();
        let sprachen = [
            (false, Sprache::De.anzeige().to_string()),
            (true, Sprache::En.anzeige().to_string()),
        ];
        let mut thema = self.thema;
        let mut englisch = self.englisch;

        bausteine::kopf(
            ui,
            &f,
            |ui| {
                bausteine::marke(ui, &f);
                ui.add_space(thema::SPACE_3);
                bausteine::status(ui, &f, Ton::Gut, "Verbunden mit http://quiver.example:8080");
            },
            |ui| {
                // Von rechts nach links, wie in der Verwaltung von links nach
                // rechts: erst die Sprache, daneben das Farbschema.
                ui.spacing_mut().item_spacing.x = thema::SPACE_3;
                bausteine::segmentauswahl(ui, &f, &mut englisch, &sprachen);
                bausteine::segmentauswahl(ui, &f, &mut thema, &farbschemen);
            },
        );

        let eintraege: Vec<NavEintrag<'_>> = NAVIGATION
            .iter()
            .enumerate()
            .map(|(index, (name, icon))| NavEintrag {
                icon: *icon,
                name,
                gewaehlt: index == self.seite,
            })
            .collect();
        if let Some(index) = bausteine::seitenleiste(ui, &f, "Verwaltung", &eintraege) {
            self.seite = index;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(f.bg_canvas))
            .show(ui, |ui| {
                bausteine::seite(ui, |ui| {
                    bausteine::seitenkopf(
                        ui,
                        &f,
                        "Bibliothek",
                        "Jedes MCP-Tool, das der Server verteilen kann.",
                    );
                    bausteine::karte(
                        ui,
                        &f,
                        "Einträge",
                        Some("Drei Zustände, wie sie die Seite zeigt."),
                        |ui| {
                            // Feste Abstände statt `item_spacing`: die
                            // Verwaltung setzt sie genauso.
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.horizontal(|ui| {
                                bausteine::hauptknopf_mit(ui, &f, Icon::Plus, "Neu");
                                bausteine::knopf(ui, &f, Icon::Neuladen, "Neu laden");
                            });
                            ui.add_space(thema::SPACE_3);
                            for (ton, text) in [
                                (Ton::Gut, "Läuft · 2 Tools"),
                                (Ton::Warnung, "Braucht Angaben: PFAD"),
                                (Ton::Fehler, "Fehler"),
                            ] {
                                bausteine::status(ui, &f, ton, text);
                                ui.add_space(thema::SPACE_1);
                            }
                            ui.add_space(thema::SPACE_2);
                            bausteine::feld(
                                ui,
                                &f,
                                "Suche",
                                Some("Kennung, Titel, Beschreibung"),
                                |ui| {
                                    ui.add(
                                        bausteine::eingabefeld(&mut self.suche)
                                            .hint_text("Suchen")
                                            .desired_width(bausteine::feldbreite(ui)),
                                    )
                                },
                            );
                        },
                    );
                });
            });
    }
}

fn harness(muster: Muster) -> Harness<'static, Muster> {
    Harness::builder()
        .with_size(egui::vec2(
            (GROESSE.0 + 2 * GERUEST_RAND) as f32,
            (GROESSE.1 + 2 * GERUEST_RAND) as f32,
        ))
        .with_context_setup(|ctx| {
            quiver_oberflaeche::einrichten(ctx);
        })
        .build_ui_state(
            |ui, muster: &mut Muster| {
                thema::anwenden(ui.ctx(), muster.thema);
                muster.zeigen(ui);
            },
            muster,
        )
}

#[test]
fn musterseite_steht_in_allen_teilen() {
    let mut h = harness(Muster::neu(Thema::Dunkel));
    h.run();
    // Kopf und Seitenleiste sind da — über Rollen und Namen gefunden, damit
    // die Prüfung zugleich die Zugänglichkeit abdeckt.
    assert!(h.query_by_label("mcp-quiver").is_some(), "Wortmarke fehlt");
    for (name, _) in NAVIGATION {
        assert!(
            h.query_by_role_and_label(quiver_oberflaeche::egui::accesskit::Role::Button, name)
                .is_some(),
            "„{name}\" fehlt in der Seitenleiste"
        );
    }
    // Die Segmentauswahl ist eine Gruppe von Optionsfeldern.
    for name in ["Dunkel", "Hell", "Mono", "Deutsch", "English"] {
        assert!(
            h.query_by_role_and_label(quiver_oberflaeche::egui::accesskit::Role::RadioButton, name)
                .is_some(),
            "„{name}\" fehlt in der Segmentauswahl"
        );
    }
    // Seite und Karte: „Bibliothek" steht in der Leiste **und** als Titel.
    assert!(
        h.query_all_by_label("Bibliothek").count() >= 2,
        "Seitenkopf oder Eintrag fehlt"
    );
    assert!(h.query_by_label("Einträge").is_some());
    for name in ["Neu", "Neu laden"] {
        assert!(
            h.query_by_role_and_label(quiver_oberflaeche::egui::accesskit::Role::Button, name)
                .is_some(),
            "„{name}\" fehlt"
        );
    }
    for name in ["Läuft · 2 Tools", "Braucht Angaben: PFAD", "Fehler"] {
        assert!(h.query_by_label(name).is_some(), "„{name}\" fehlt");
    }
    // Das Eingabefeld trägt den Namen seiner Beschriftung.
    assert!(
        h.query_by_role_and_label(
            quiver_oberflaeche::egui::accesskit::Role::TextInput,
            "Suche"
        )
        .is_some(),
        "Eingabefeld „Suche\" fehlt"
    );
}

/// Wohin die Bilder gehen (relativ ab dem Wurzelverzeichnis des Repos). Ein
/// leerer Wert heißt „keine Bilder“; sonst landeten sie im Wurzelverzeichnis.
fn bilder_ordner() -> Option<PathBuf> {
    let gesetzt = std::env::var_os("QUIVER_BILDER").map(PathBuf::from)?;
    if gesetzt.as_os_str().is_empty() {
        return None;
    }
    Some(match gesetzt {
        pfad if pfad.is_absolute() => pfad,
        pfad => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(pfad),
    })
}

/// Die beiden Bilder für die Doku — dasselbe Bild in beiden Themes.
#[test]
fn muster_bilder() {
    let Some(ziel) = bilder_ordner() else {
        return;
    };
    std::fs::create_dir_all(&ziel).expect("Bilderordner");
    for thema in [Thema::Dunkel, Thema::Hell] {
        let mut h = harness(Muster::neu(thema));
        h.run();
        let bild = h.render().expect("zeichnen (wgpu)");
        // Der Rand des Prüfgerüsts gehört nicht zur Seite.
        let zugeschnitten = bild
            .view(GERUEST_RAND, GERUEST_RAND, GROESSE.0, GROESSE.1)
            .to_image();
        zugeschnitten
            .save(ziel.join(format!("muster-egui-{}.png", thema.name())))
            .expect("Bild schreibbar");
    }
}
