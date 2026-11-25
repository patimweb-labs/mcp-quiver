// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was sich das Fenster über einen Neustart hinweg merkt: Theme und Sprache.
//!
//! Der Kern legt seinen Stand in `<profil>/zustand.json` ab; das Fenster
//! schreibt daneben `<profil>/oberflaeche.json`. Damit bleibt es beim Kern
//! eine Wahrheit, und ein kaputter Stand des einen hindert den anderen nicht
//! am Starten.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::texte::Sprache;
use crate::thema::Thema;

/// Die Namen der Oberflächen-Einstellungen.
const DATEI: &str = "oberflaeche.json";

/// Theme und Sprache, wie sie in der Datei stehen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Einstellungen {
    /// `dark`, `light` oder `mono`.
    #[serde(default)]
    pub thema: String,
    /// `de` oder `en`.
    #[serde(default)]
    pub sprache: String,
}

impl Einstellungen {
    /// Aus den beiden Werten des Fensters.
    pub fn neu(thema: Thema, sprache: Sprache) -> Self {
        Self {
            thema: thema.name().to_string(),
            sprache: sprache.code().to_string(),
        }
    }

    pub fn thema(&self) -> Thema {
        Thema::aus_name(&self.thema)
    }

    pub fn sprache(&self) -> Sprache {
        Sprache::aus_code(&self.sprache)
    }

    /// Datei lesen. Fehlt sie oder ist sie unlesbar, gilt der Standard
    /// (`dark`, Deutsch) — die Oberfläche darf daran nicht scheitern.
    pub fn laden(ordner: &Path) -> Self {
        let pfad = pfad(ordner);
        match std::fs::read_to_string(&pfad) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|fehler| {
                tracing::warn!("{pfad:?} ist unlesbar, nehme den Standard: {fehler}");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Datei schreiben: atomar, 0600 im Ordner 0700 (wie `zustand.json`).
    pub fn speichern(&self, ordner: &Path) -> anyhow::Result<()> {
        let ziel = pfad(ordner);
        let text = serde_json::to_vec_pretty(self)?;
        quiver_client::rechte::schreiben_atomar(&ziel, &text)
    }
}

fn pfad(ordner: &Path) -> PathBuf {
    ordner.join(DATEI)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner() -> PathBuf {
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        static ZAEHLER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pfad = std::env::temp_dir().join(format!(
            "quiver-fenster-profil-{}-{stempel}-{nummer}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    #[test]
    fn rundlauf() {
        let ordner = ordner();
        assert_eq!(Einstellungen::laden(&ordner), Einstellungen::default());
        let e = Einstellungen::neu(Thema::Mono, Sprache::En);
        e.speichern(&ordner).expect("schreibbar");
        let zurueck = Einstellungen::laden(&ordner);
        assert_eq!(zurueck.thema(), Thema::Mono);
        assert_eq!(zurueck.sprache(), Sprache::En);
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn kaputte_datei_stoppt_den_start_nicht() {
        let ordner = ordner();
        std::fs::write(pfad(&ordner), "kein json").expect("schreibbar");
        assert_eq!(Einstellungen::laden(&ordner).thema(), Thema::Dunkel);
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn leerer_eintrag_bedeutet_standard() {
        let e = Einstellungen::default();
        assert_eq!(e.thema(), Thema::Dunkel);
        assert_eq!(e.sprache(), Sprache::De);
    }
}
