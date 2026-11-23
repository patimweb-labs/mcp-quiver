// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo Geheimnisse liegen: im Schlüsselbund oder (für Tests) im Speicher.
//!
//! Der Client legt dort ab:
//! - `geraet-token` – das Token aus der Anmeldung beim Server,
//! - `wert/<eintrag>/<feld>` – die Werte von Feldern mit `Quelle::Geheim`.
//!
//! Nichts davon verlässt je das Gerät in Richtung Agent.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Der Dienstname im Schlüsselbund.
pub const DIENST: &str = "mcp-quiver";

/// Ein Ablageort für Geheimnisse.
pub trait Tresor: Send + Sync {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>>;
    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()>;
    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()>;
}

/// Der Schlüsselbund des Systems (Keychain, Credential Manager, Secret Service).
pub struct Schluesselbund;

impl Schluesselbund {
    fn eintrag(schluessel: &str) -> anyhow::Result<keyring::Entry> {
        Ok(keyring::Entry::new(DIENST, schluessel)?)
    }
}

impl Tresor for Schluesselbund {
    fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        match Self::eintrag(schluessel)?.get_password() {
            Ok(wert) => Ok(Some(wert)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(fehler) => Err(fehler.into()),
        }
    }

    fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        Self::eintrag(schluessel)?.set_password(wert)?;
        Ok(())
    }

    fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        match Self::eintrag(schluessel)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(fehler) => Err(fehler.into()),
        }
    }
}

/// Ein Tresor im Arbeitsspeicher – für Tests und für Rechner ohne Schlüsselbund.
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
