// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Sprache der Oberfläche: Deutsch und Englisch.
//!
//! Eine Wahrheit, zwei Sprachen: `texte.json`. Fehlt ein Schlüssel in der
//! gewählten Sprache, gilt Deutsch; fehlt er ganz, steht der Schlüssel da
//! (sichtbar, nicht leer). Platzhalter `{}` werden der Reihe nach gefüllt.

use std::collections::HashMap;
use std::sync::OnceLock;

type Tabelle = HashMap<String, HashMap<String, String>>;

fn tabelle() -> &'static Tabelle {
    static T: OnceLock<Tabelle> = OnceLock::new();
    T.get_or_init(|| {
        serde_json::from_str(include_str!("../texte.json")).expect("texte.json ist gültig")
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sprache {
    #[default]
    De,
    En,
}

impl Sprache {
    pub const ALLE: [Sprache; 2] = [Sprache::De, Sprache::En];

    pub fn aus_code(code: &str) -> Self {
        match code {
            "en" => Sprache::En,
            _ => Sprache::De,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Sprache::De => "de",
            Sprache::En => "en",
        }
    }

    pub fn anzeige(self) -> &'static str {
        match self {
            Sprache::De => "Deutsch",
            Sprache::En => "English",
        }
    }
}

/// Text zu einem Schlüssel.
pub fn t(sprache: Sprache, schluessel: &str) -> String {
    match tabelle().get(schluessel) {
        None => schluessel.to_string(),
        Some(eintrag) => eintrag
            .get(sprache.code())
            .or_else(|| eintrag.get("de"))
            .cloned()
            .unwrap_or_else(|| schluessel.to_string()),
    }
}

/// Text mit Platzhaltern.
pub fn tf(sprache: Sprache, schluessel: &str, werte: &[&dyn std::fmt::Display]) -> String {
    let mut text = t(sprache, schluessel);
    for wert in werte {
        text = text.replacen("{}", &wert.to_string(), 1);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deutsch_und_rueckfall() {
        assert_eq!(t(Sprache::De, "app.einstellungen"), "Einstellungen");
        assert_eq!(t(Sprache::En, "app.einstellungen"), "Settings");
        assert_eq!(t(Sprache::En, "gibt.es.nicht"), "gibt.es.nicht");
    }

    #[test]
    fn platzhalter_der_reihe_nach() {
        assert_eq!(
            tf(Sprache::De, "verbindung.verbunden", &[&"quiver.example"]),
            "Verbunden mit quiver.example"
        );
        assert_eq!(tf(Sprache::En, "tools.laeuft", &[&3]), "Running · 3 tools");
    }

    #[test]
    fn jeder_schluessel_hat_beide_sprachen() {
        for (k, v) in tabelle() {
            for s in Sprache::ALLE {
                assert!(
                    v.get(s.code()).is_some_and(|text| !text.is_empty()),
                    "{k} ohne {}",
                    s.code()
                );
            }
        }
    }

    #[test]
    fn codes_hin_und_zurueck() {
        for s in Sprache::ALLE {
            assert_eq!(Sprache::aus_code(s.code()), s);
        }
        assert_eq!(Sprache::aus_code("es"), Sprache::De);
    }
}
