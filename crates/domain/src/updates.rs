// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Updates: Versionen und die Veröffentlichung, wie GitHub sie meldet
//! (Aufgabe 34).
//!
//! Alles hier ist **reine Rechnung** – kein Netz, kein Dateisystem, keine
//! Signatur. quiver installiert sich nicht selbst: die Anzeige sagt nur, dass
//! eine neuere Fassung veröffentlicht ist; eingespielt wird sie durch die
//! Geräteverwaltung oder von Hand aus dem Release.
//!
//! Zwei Regeln tragen den Entwurf:
//!
//! 1. **Kein Rückschritt.** Neu ist nur, was echt größer ist als die laufende
//!    Version. Eine unlesbare Version ist nie neuer.
//! 2. **Die Anzeige erfindet nichts.** Was nicht in der Antwort der
//!    Veröffentlichung steht (Version, Link), wird nicht gezeigt; ein Fehler
//!    in der Antwort ist ein Fehler und kein Angebot.

use serde::{Deserialize, Serialize};

/// So groß darf die Antwort der Veröffentlichung höchstens sein.
pub const ANTWORT_BYTES_MAX: usize = 64 * 1024;

/// Der Text des Hinweises, solange keine Vorgabe der Firma etwas anderes sagt.
pub const TEXT_OHNE_VERWALTUNG: &str = "Version {version} ist verfügbar";
/// Der Text, wenn das Gerät über die Geräteverwaltung versorgt wird.
pub const TEXT_MIT_VERWALTUNG: &str = "Version {version} kommt über die Geräteverwaltung";

/// Die Vorgabe des Hinweistextes; `{version}` wird ersetzt, sonst bleibt der
/// Text, wie er ist.
pub fn hinweis_text(vorlage: &str, version: &str) -> String {
    vorlage.replace("{version}", version)
}

// --------------------------------------------------------------- Versionen

/// Eine Version `1.2.3` oder `1.2.3-rc.1`. Führende `v` werden geschluckt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    teile: Vec<u64>,
    /// `None` heißt: eine fertige Fassung. Eine Vorabfassung ist **kleiner**
    /// als dieselbe Nummer ohne Zusatz.
    vorab: Option<String>,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match self.teile.cmp(&other.teile) {
            Ordering::Equal => match (&self.vorab, &other.vorab) {
                (None, None) => Ordering::Equal,
                // Fertig ist größer als vorab.
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            },
            anderes => anderes,
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let teile: Vec<String> = self.teile.iter().map(u64::to_string).collect();
        f.write_str(&teile.join("."))?;
        if let Some(vorab) = &self.vorab {
            write!(f, "-{vorab}")?;
        }
        Ok(())
    }
}

/// Eine Version lesen; `None` heißt: kein brauchbarer Wert.
pub fn version_lesen(text: &str) -> Option<Version> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    if text.is_empty() || text.len() > 64 {
        return None;
    }
    let (zahlen, vorab) = match text.split_once('-') {
        Some((zahlen, vorab)) => (zahlen, Some(vorab)),
        None => (text, None),
    };
    if vorab.is_some_and(|vorab| {
        vorab.is_empty()
            || vorab.len() > 32
            || !vorab.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
    }) {
        return None;
    }
    let mut teile = Vec::new();
    for teil in zahlen.split('.') {
        if teil.is_empty() || teil.len() > 9 || !teil.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        teile.push(teil.parse::<u64>().ok()?);
    }
    if teile.is_empty() || teile.len() > 4 {
        return None;
    }
    Some(Version {
        teile,
        vorab: vorab.map(str::to_string),
    })
}

/// Ist `neu` echt größer als `alt`? Unlesbare Versionen sind nie größer –
/// damit ist ein Rückschritt ausgeschlossen, auch bei Unsinn in der Antwort.
pub fn version_neuer(neu: &str, alt: &str) -> bool {
    match (version_lesen(neu), version_lesen(alt)) {
        (Some(neu), Some(alt)) => neu > alt,
        _ => false,
    }
}

// --------------------------------------------------------- Veröffentlichung

/// Eine veröffentlichte Fassung, so wie der Server sie kennt und anzeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Veroeffentlichung {
    /// Die Version aus dem Tag, ohne führendes `v` (z. B. `0.3.0`).
    pub version: String,
    /// Wann sie veröffentlicht wurde (RFC 3339), nur zur Anzeige.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub veroeffentlicht: Option<String>,
    /// Die Seite mit den Änderungen; `None`, wenn die Antwort keine brauchbare
    /// Adresse enthielt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aenderungen: Option<String>,
}

/// Warum eine Antwort der Veröffentlichung nicht taugt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseFehler {
    #[error("die Antwort ist kein JSON: {0}")]
    Json(String),
    #[error("die Antwort nennt keine Version")]
    OhneVersion,
    #[error("die Version {0:?} ist unbrauchbar")]
    Version(String),
}

/// Was von der Antwort einer Veröffentlichung gelesen wird.
#[derive(Debug, Deserialize)]
struct Antwort {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Die Antwort der Veröffentlichung (GitHub-API, `releases/latest`) lesen.
///
/// Ein Entwurf oder eine Vorabfassung ist kein Angebot: sie werden abgelehnt,
/// damit die Anzeige nie eine unfertige Fassung als „neu“ meldet.
pub fn veroeffentlichung_lesen(text: &str) -> Result<Veroeffentlichung, ReleaseFehler> {
    if text.len() > ANTWORT_BYTES_MAX {
        return Err(ReleaseFehler::Json(String::from("zu groß")));
    }
    let antwort: Antwort =
        serde_json::from_str(text).map_err(|fehler| ReleaseFehler::Json(fehler.to_string()))?;
    if antwort.draft || antwort.prerelease {
        return Err(ReleaseFehler::Version(String::from(
            "Entwurf oder Vorabfassung",
        )));
    }
    if antwort.tag_name.trim().is_empty() {
        return Err(ReleaseFehler::OhneVersion);
    }
    let version = version_lesen(&antwort.tag_name)
        .ok_or_else(|| ReleaseFehler::Version(antwort.tag_name.clone()))?;
    // Nur eine Seite mit `https`: alles andere wäre ein Link, der irgendwohin
    // führt.
    let aenderungen = Some(antwort.html_url.trim().to_string())
        .filter(|url| url.starts_with("https://") && url.len() <= 512);
    Ok(Veroeffentlichung {
        version: version.to_string(),
        veroeffentlicht: antwort.published_at.filter(|zeit| zeit.len() <= 64),
        aenderungen,
    })
}

#[cfg(test)]
mod tests;
