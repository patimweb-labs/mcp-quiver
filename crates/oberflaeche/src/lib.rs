// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Bausatz des Client-Fensters: Themes aus den Token-Dateien
//! (`build.rs`), Texte in zwei Sprachen, eingebettete Schriften, Bausteine.
//!
//! Die Token-Dateien in `tokens/` sind die einzige Quelle für egui **und** für
//! die Admin-Oberfläche in Vue (AGENTS.md §2.11); `tokens_css` schreibt sie als
//! CSS-Variablen.

pub mod bausteine;
pub mod texte;
pub mod thema;
pub mod tokens_lesen;

pub use egui;

/// Einmal beim Start: Schriften und die Bildlader (für die SVG-Icons).
pub fn einrichten(ctx: &egui::Context) {
    thema::schriften(ctx);
    egui_extras::install_image_loaders(ctx);
    // Cmd+Q ist kein Ende, sondern legt das Fenster in den Tray (Aufgabe 22).
    // egui würde sonst selbst `ViewportCommand::Close` schicken; das Fenster
    // fängt die Tastenkombination selbst ab.
    ctx.options_mut(|optionen| optionen.quit_shortcuts.clear());
}
