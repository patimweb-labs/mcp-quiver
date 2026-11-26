// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Updates aus Sicht des Clients (Aufgabe 34): **nur anzeigen**.
//!
//! Der Client fragt **nur den Server**, nie das Internet. Er erfährt die
//! neueste veröffentlichte Version und die Seite mit den Änderungen; ist sie
//! neuer als die laufende, zeigt er einen ruhigen Hinweis in Fenster und Tray.
//!
//! Es gibt hier **kein** Herunterladen, Prüfen oder Tauschen: installiert und
//! aktualisiert wird durch die Geräteverwaltung (Intune) oder von Hand aus den
//! Paketen des Releases (`docs/intune.md`). Die Konfiguration des Benutzers
//! bleibt dabei im Profil stehen.

use std::time::Duration;

use anyhow::Context;
use quiver_domain::updates::{
    self, version_lesen, version_neuer, TEXT_MIT_VERWALTUNG, TEXT_OHNE_VERWALTUNG,
};
use quiver_protocol::{UpdateAngebot, PRAEFIX};
use serde::{Deserialize, Serialize};

use crate::vorgaben::Vorgaben;
use crate::{netz, Innen};

/// So oft fragt der Client den Server (der Server selbst fragt höchstens alle
/// sechs Stunden die Veröffentlichung).
pub const ABSTAND: Duration = Duration::from_secs(6 * 60 * 60);
/// Streuung um [`ABSTAND`].
pub const STREUUNG: Duration = Duration::from_secs(30 * 60);
/// So groß darf die Antwort des Servers höchstens sein.
pub const ANGEBOT_BYTES: usize = 64 * 1024;

/// Der Stand des Updates, wie ihn das Fenster zeigt (Summentyp, kein Fächer
/// aus Booleschen Werten).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum UpdateLage {
    /// Noch nicht gefragt.
    #[default]
    Unbekannt,
    /// Nichts zu tun.
    Keins,
    /// Eine neuere Version ist veröffentlicht; `hinweis` ist der fertige Satz
    /// aus der Vorgabe der Firma.
    Verfuegbar {
        version: String,
        hinweis: String,
        #[serde(default)]
        aenderungen: Option<String>,
    },
    /// Fragen ist gescheitert.
    Fehler { text: String },
}

impl UpdateLage {
    /// Steht ein Update an (für Hinweis in Fenster und Tray)?
    pub fn anstehend(&self) -> Option<(&str, &str)> {
        match self {
            UpdateLage::Verfuegbar {
                version, hinweis, ..
            } => Some((version, hinweis)),
            _ => None,
        }
    }
}

/// Den Server fragen: welche Version ist veröffentlicht?
pub async fn angebot_holen(
    http: &reqwest::Client,
    server: &str,
    token: &str,
    version: &str,
) -> anyhow::Result<UpdateAngebot> {
    let adresse = format!(
        "{server}{PRAEFIX}/geraet/updates?version={}",
        url_teil(version)
    );
    let antwort = netz::frist(http.get(&adresse).bearer_auth(token))
        .send()
        .await
        .with_context(|| format!("{adresse} nicht erreichbar"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("{adresse} antwortet mit {}", antwort.status());
    }
    netz::json_begrenzt(antwort, ANGEBOT_BYTES).await
}

/// Nur Ziffern, Buchstaben, Punkt, Strich und Unterstrich in der Abfrage.
fn url_teil(wert: &str) -> String {
    wert.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .take(64)
        .collect()
}

/// Aus dem Angebot die Anzeige machen.
///
/// Kein Rückschritt: ist die neueste Version nicht größer als die laufende,
/// gibt es nichts zu zeigen – auch dann nicht, wenn der Server etwas anderes
/// behauptet. Der Text kommt aus der Vorgabe der Firma (oder aus dem
/// Standardtext, je nachdem ob das Gerät verwaltet ist).
pub fn lage_aus(angebot: &UpdateAngebot, vorgaben: &Vorgaben, laufend: &str) -> UpdateLage {
    let Some(neueste) = angebot.neueste_version.as_deref() else {
        return UpdateLage::Keins;
    };
    if !version_neuer(neueste, laufend) {
        return UpdateLage::Keins;
    }
    let vorlage = match (&vorgaben.update_hinweis, vorgaben.verwaltet()) {
        (Some(text), _) => text.as_str(),
        (None, true) => TEXT_MIT_VERWALTUNG,
        (None, false) => TEXT_OHNE_VERWALTUNG,
    };
    // Für die Anzeige zählt die gelesene Form (`v0.3.0` wird `0.3.0`).
    let version = version_lesen(neueste).map_or_else(|| neueste.to_string(), |v| v.to_string());
    UpdateLage::Verfuegbar {
        version: version.clone(),
        hinweis: updates::hinweis_text(vorlage, &version),
        aenderungen: angebot
            .aenderungen
            .clone()
            .filter(|link| link.starts_with("https://")),
    }
}

// ------------------------------------------------------------ Hintergrund

/// Fragt im Hintergrund, ob eine neuere Version veröffentlicht ist.
pub(crate) async fn lauf(innen: std::sync::Arc<Innen>) {
    let mut naechste = tokio::time::Instant::now();
    loop {
        if innen.ende.ist_abgebrochen() {
            return;
        }
        let jetzt = tokio::time::Instant::now();
        if jetzt >= naechste {
            if let Err(fehler) = pruefen(&innen).await {
                tracing::debug!("Update-Prüfung: {fehler:#}");
            }
            let streuung = if STREUUNG.is_zero() {
                Duration::ZERO
            } else {
                Duration::from_secs(rand::random::<u64>() % STREUUNG.as_secs().max(1))
            };
            naechste = jetzt + ABSTAND + streuung;
        }
        tokio::select! {
            _ = innen.ende.warten() => return,
            _ = innen.update_erneut.notified() => {
                // Neue Anmeldung: gleich fragen, aber nicht in einer Schleife.
                naechste = tokio::time::Instant::now();
            }
            _ = tokio::time::sleep(ABSTAND) => {}
        }
    }
}

/// Einmal fragen und den Stand setzen. Ohne Verbindung passiert nichts.
pub(crate) async fn pruefen(innen: &std::sync::Arc<Innen>) -> anyhow::Result<()> {
    let (server, token) = innen.zugang();
    let (Some(server), Some(token)) = (server, token) else {
        return Ok(());
    };
    let angebot = angebot_holen(&innen.http, &server, &token, crate::CLIENT_VERSION).await?;
    let lage = lage_aus(&angebot, &innen.konfig.vorgaben, crate::CLIENT_VERSION);
    innen.update_setzen(lage);
    Ok(())
}

#[cfg(test)]
mod tests;
