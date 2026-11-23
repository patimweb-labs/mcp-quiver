// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo Geheimnisse liegen: in einer eigenen verschlüsselten Datei
//! ([`DateiTresor`], `<profil>/tresor`) oder (für Tests) im Speicher.
//!
//! Der Client legt dort ab:
//! - `geraet-token` – das Token aus der Anmeldung beim Server,
//! - `wert/<eintrag>/<feld>` – die Werte von Feldern mit `Quelle::Geheim`.
//!
//! Nichts davon verlässt je das Gerät in Richtung Agent.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Der Dienstname im alten Schlüsselbund (nur noch für den Umzug).
pub const DIENST: &str = "mcp-quiver";

/// Ein Ablageort für Geheimnisse.
pub trait Tresor: Send + Sync {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>>;
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()>;
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()>;
}

pub use crate::dateitresor::DateiTresor;

impl DateiTresor {
    /// Der Tresor des Clients in `<profil>/tresor`.
    ///
    /// Unter macOS wird ein Wert, der noch nicht in der Datei steht, einmal
    /// im alten Schlüsselbund gesucht und übernommen (Umzug ohne neue
    /// Eingabe). Ein anderes Profil (`QUIVER_PROFIL`) zieht nicht um.
    pub fn fuer_profil(profil: &std::path::Path, umziehen: bool) -> Self {
        let tresor = DateiTresor::im_ordner(profil.join("tresor"));
        if umziehen {
            if let Some(altbestand) = altbestand() {
                return tresor.mit_altbestand(altbestand);
            }
        }
        tresor
    }
}

impl Tresor for DateiTresor {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        DateiTresor::lesen(self, schluessel)
    }
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        DateiTresor::schreiben(self, schluessel, wert)
    }
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        DateiTresor::loeschen(self, schluessel)
    }
}

/// Der alte Schlüsselbund als Altbestand – nur unter macOS.
#[cfg(target_os = "macos")]
fn altbestand() -> Option<crate::dateitresor::Altbestand> {
    Some(Box::new(|schluessel: &str| {
        keyring::Entry::new(DIENST, schluessel)
            .ok()?
            .get_password()
            .ok()
    }))
}

#[cfg(not(target_os = "macos"))]
fn altbestand() -> Option<crate::dateitresor::Altbestand> {
    None
}

/// Ein Tresor im Arbeitsspeicher – für Tests und für Läufe ohne Spuren.
#[derive(Debug, Default)]
pub struct SpeicherTresor {
    werte: Mutex<BTreeMap<String, String>>,
}

impl SpeicherTresor {
    pub fn neu() -> Self {
        Self::default()
    }

    /// Was gerade drin liegt (für Prüfungen).
    pub fn alles(&self) -> BTreeMap<String, String> {
        self.werte
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clone()
    }
}

impl Tresor for SpeicherTresor {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        let werte = self
            .werte
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        Ok(werte.get(schluessel).cloned())
    }

    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        let mut werte = self
            .werte
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        werte.insert(schluessel.to_string(), wert.to_string());
        Ok(())
    }

    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        let mut werte = self
            .werte
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        werte.remove(schluessel);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speicher_tresor_rundlauf() {
        let tresor = SpeicherTresor::neu();
        assert_eq!(tresor.lesen("a").expect("lesbar"), None);
        tresor.schreiben("a", "geheim").expect("schreibbar");
        assert_eq!(
            tresor.lesen("a").expect("lesbar"),
            Some("geheim".to_string())
        );
        tresor.loeschen("a").expect("löschbar");
        assert_eq!(tresor.lesen("a").expect("lesbar"), None);
        tresor
            .loeschen("gibt-es-nicht")
            .expect("Löschen ist geduldig");
    }
}
