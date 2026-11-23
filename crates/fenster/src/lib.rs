// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! mcp-quiver als Fensteranwendung (egui).
//!
//! Aufbau wie in sprechwerk: [`modell`] hält, was die Oberfläche zeigt,
//! [`logik`] rechnet (reine Funktionen mit Tests), [`ansichten`] zeichnen,
//! [`app`] verbindet das mit dem Client-Kern. Kein Zustand außerhalb des
//! Modells (AGENTS.md §2.1). [`kern`] ist der echte Zugang zum Kern,
//! [`profil`] merkt Theme und Sprache.

pub mod ansichten;
pub mod app;
pub mod kern;
pub mod logik;
pub mod modell;
pub mod profil;

pub use quiver_oberflaeche::{bausteine, egui, texte, thema};
