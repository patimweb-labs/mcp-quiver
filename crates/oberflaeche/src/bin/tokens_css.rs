// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Schreibt die Token-Dateien als CSS-Variablen nach `stdout` (AGENTS.md §2.11):
//! dieselben Tokens tragen das Client-Fenster in egui und die Admin-Oberfläche
//! in Vue. Erzeugt nach `admin/src/styles/tokens.css`:
//!
//! ```text
//! cargo run -p quiver-oberflaeche --bin tokens_css > admin/src/styles/tokens.css
//! ```

fn main() {
    print!("{}", quiver_oberflaeche::tokens_lesen::als_css());
}
