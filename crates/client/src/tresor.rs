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

/// Ein Ablageort für Geheimnisse.
///
/// Neben Lesen, Schreiben und Löschen gibt es drei Dinge, die der Kern
/// braucht, damit ein kaputter Tresor nicht den Start verhindert (Paket 3):
/// [`Tresor::fehler`] sagt, was nicht lesbar war, [`Tresor::neu_beginnen`]
/// räumt einen kaputten Tresor beiseite, und [`Tresor::werte_laden`] liest
/// viele Schlüssel in **einem** Zugriff (ein Abgleich liest den Tresor genau
/// einmal).
pub trait Tresor: Send + Sync {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>>;
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()>;
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()>;

    /// Warum der Tresor nicht lesbar ist – `None` heißt: alles in Ordnung.
    /// Der Text ist für das Fenster gedacht und enthält nie einen Wert.
    fn fehler(&self) -> Option<String> {
        None
    }

    /// Nach „Neu beginnen“: den kaputten Stand endgültig beiseitelegen. Die
    /// alten Dateien bleiben unter `.kaputt-<zeit>` erhalten.
    fn neu_beginnen(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Alle angegebenen Schlüssel auf einmal lesen. Der Standard geht einzeln
    /// vor; [`DateiTresor`] liest die Datei **einmal** und spart so je Wert
    /// eine Sperre und einen Durchlauf.
    fn werte_laden(&self, schluessel: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
        let mut werte = BTreeMap::new();
        for name in schluessel {
            if let Some(wert) = self.lesen(name)? {
                werte.insert(name.clone(), wert);
            }
        }
        Ok(werte)
    }
}

pub use crate::dateitresor::DateiTresor;

impl DateiTresor {
    /// Der Tresor des Clients in `<profil>/tresor`.
    pub fn fuer_profil(profil: &std::path::Path) -> Self {
        DateiTresor::im_ordner(profil.join("tresor"))
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
    fn fehler(&self) -> Option<String> {
        DateiTresor::fehler(self)
    }
    fn neu_beginnen(&self) -> anyhow::Result<()> {
        DateiTresor::neu_beginnen(self)
    }
    fn werte_laden(&self, schluessel: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
        DateiTresor::werte_laden(self, schluessel)
    }
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
