// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! mcp-quiver als Fensteranwendung (egui).
//!
//! Aufbau wie in sprechwerk: [`modell`] hält, was die Oberfläche zeigt,
//! [`logik`] rechnet (reine Funktionen mit Tests), [`ansichten`] zeichnen,
//! [`app`] verbindet das mit dem Client-Kern. Kein Zustand außerhalb des
//! Modells (AGENTS.md §2.1). [`kern`] ist der echte Zugang zum Kern,
//! [`profil`] merkt Theme und Sprache.
//!
//! Im Hintergrund geht es weiter: [`tray`] ist das Symbol in Menüleiste,
//! Infobereich oder StatusNotifierItem, [`autostart`] der Eintrag beim
//! Anmelden, [`instanz`] sorgt dafür, dass es nur eine Instanz je Gerät gibt.

pub mod ansichten;
pub mod app;
pub mod autostart;
pub mod datei;
pub mod ende;
pub mod instanz;
pub mod kern;
pub mod logik;
pub mod modell;
pub mod profil;
pub mod tray;

pub use quiver_oberflaeche::{bausteine, egui, texte, thema};
