// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Schreibt die Tokens als CSS-Variablen für die Seiten des Servers nach
//! `stdout` (Aufgabe 26, Punkt 5): dunkel als Vorgabe, hell über
//! `prefers-color-scheme`.
//!
//! ```text
//! cargo run -p quiver-oberflaeche --bin tokens_seite_css > crates/server/seiten/system.css
//! ```

fn main() {
    print!("{}", quiver_oberflaeche::tokens_lesen::als_seiten_css());
}
