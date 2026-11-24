// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Datei-Dialog: Export und Import der Einstellungen (Aufgabe 15) und die
//! Ordnerwahl der Angaben (`FeldArt::Ordner`).
//!
//! Nativer Dialog über [`rfd`] – reines Rust, auf allen drei Plattformen
//! derselbe Aufruf. Die Ansicht fragt ihn über
//! [`crate::modell::Sicht::datei_waehlen`] an; im Test steht der Pfad in der
//! Sicht und der Dialog öffnet sich gar nicht.

use std::path::PathBuf;

/// Eine Datei wählen: `speichern` öffnet den Speichern-Dialog mit `vorschlag`
/// als Namen, sonst den Öffnen-Dialog. `None` heißt: abgebrochen.
pub fn waehlen(speichern: bool, titel: &str, vorschlag: &str) -> Option<PathBuf> {
    let dialog = rfd::FileDialog::new()
        .set_title(titel)
        .add_filter("JSON", &["json"]);
    if speichern {
        dialog.set_file_name(vorschlag).save_file()
    } else {
        dialog.pick_file()
    }
}

/// Einen Ordner wählen; der Dialog beginnt im bisherigen, wenn es ihn gibt.
/// Neue Ordner legt der Dialog selbst an. `None` heißt: abgebrochen.
pub fn ordner_waehlen(titel: &str, bisher: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title(titel)
        .set_can_create_directories(true);
    let bisher = PathBuf::from(bisher.trim());
    if bisher.is_dir() {
        dialog = dialog.set_directory(bisher);
    }
    dialog.pick_folder()
}
