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

use std::collections::BTreeMap;

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
    /// Die Dateien dieses Releases (Aufgabe 39). Nur Namen und Adressen, wie
    /// GitHub sie nennt; was davon zu einem Gerät gehört, entscheidet
    /// [`pakete_fuer`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dateien: Vec<ReleaseDatei>,
}

impl Veroeffentlichung {
    /// Die Datei mit den Prüfsummen, falls das Release eine mitbringt.
    pub fn pruefsummen_datei(&self) -> Option<&ReleaseDatei> {
        self.dateien
            .iter()
            .find(|datei| datei.name == PRUEFSUMMEN_NAME)
    }
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
    #[serde(default)]
    assets: Vec<AntwortDatei>,
}

/// Eine Datei in der Antwort der Veröffentlichung.
#[derive(Debug, Deserialize)]
struct AntwortDatei {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
    #[serde(default)]
    size: u64,
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
    // Nur Dateien mit reinem Namen und einer Adresse, von der geladen werden
    // darf: ein Name wird Teil eines Pfades, eine Adresse wird aufgerufen.
    let dateien = antwort
        .assets
        .into_iter()
        .filter(|datei| {
            ist_dateiname(&datei.name)
                && (PaketArt::aus_name(&datei.name).is_some() || datei.name == PRUEFSUMMEN_NAME)
        })
        .map(|datei| ReleaseDatei {
            adresse: Some(datei.browser_download_url.trim().to_string())
                .filter(|adresse| adresse_erlaubt(adresse)),
            name: datei.name.trim().to_string(),
            groesse: datei.size.min(PAKET_BYTES_MAX),
        })
        .collect();
    Ok(Veroeffentlichung {
        version: version.to_string(),
        veroeffentlicht: antwort.published_at.filter(|zeit| zeit.len() <= 64),
        aenderungen,
        dateien,
    })
}

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------- Pakete
//
// Aufgabe 39: der Client bietet ein Update an und tauscht sich selbst aus. Die
// Pakete kommen aus demselben Release, das die Anzeige nennt. Welche Datei zu
// welchem Gerät gehört, ist **hier** entschieden – als reine Rechnung über
// Namen, damit Server und Client dieselbe Wahl treffen.

/// So groß darf ein Paket höchstens sein (dieselbe Grenze im Server).
pub const PAKET_BYTES_MAX: u64 = 512 * 1024 * 1024;

/// So heißt die Datei mit den Prüfsummen im Release.
pub const PRUEFSUMMEN_NAME: &str = "SHA256SUMS";

/// Die Art eines Pakets, erkannt an der Endung des Dateinamens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaketArt {
    /// Portables Windows-Paket.
    Zip,
    /// Windows-Installationspaket (je Benutzer).
    Msi,
    /// macOS-Datenträger mit `quiver.app`.
    Dmg,
    /// macOS-Installationspaket (für die Geräteverwaltung).
    Pkg,
    /// Linux-AppImage.
    AppImage,
    /// Linux-Baum, gepackt.
    TarGz,
    /// Debian-Paket (braucht root).
    Deb,
    /// RPM-Paket (braucht root).
    Rpm,
}

impl PaketArt {
    /// Die Art aus einem Dateinamen; `None` heißt: kein Paket, das quiver kennt.
    pub fn aus_name(name: &str) -> Option<Self> {
        let klein = name.to_ascii_lowercase();
        let endung = |text: &str| klein.ends_with(text);
        if endung(".tar.gz") {
            Some(Self::TarGz)
        } else if endung(".zip") {
            Some(Self::Zip)
        } else if endung(".msi") {
            Some(Self::Msi)
        } else if endung(".dmg") {
            Some(Self::Dmg)
        } else if endung(".pkg") {
            Some(Self::Pkg)
        } else if endung(".appimage") {
            Some(Self::AppImage)
        } else if endung(".deb") {
            Some(Self::Deb)
        } else if endung(".rpm") {
            Some(Self::Rpm)
        } else {
            None
        }
    }

    /// Die Endung, wie sie im Dateinamen steht.
    pub fn endung(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Msi => "msi",
            Self::Dmg => "dmg",
            Self::Pkg => "pkg",
            Self::AppImage => "AppImage",
            Self::TarGz => "tar.gz",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
        }
    }

    /// Der kurze Name für Vertrag, Modell und Anzeige.
    pub fn als_text(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Msi => "msi",
            Self::Dmg => "dmg",
            Self::Pkg => "pkg",
            Self::AppImage => "appimage",
            Self::TarGz => "tar.gz",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
        }
    }

    /// Die Art aus dem kurzen Namen des Vertrags.
    pub fn aus_text(text: &str) -> Option<Self> {
        match text.trim() {
            "zip" => Some(Self::Zip),
            "msi" => Some(Self::Msi),
            "dmg" => Some(Self::Dmg),
            "pkg" => Some(Self::Pkg),
            "appimage" => Some(Self::AppImage),
            "tar.gz" => Some(Self::TarGz),
            "deb" => Some(Self::Deb),
            "rpm" => Some(Self::Rpm),
            _ => None,
        }
    }
}

/// Eine Datei aus der Veröffentlichung (GitHub nennt sie „asset“).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseDatei {
    /// Der Dateiname, z. B. `quiver-0.5.0-linux-x86_64.tar.gz`.
    pub name: String,
    /// Wo sie liegt; `None`, wenn die Antwort keine brauchbare Adresse nannte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adresse: Option<String>,
    /// Größe in Bytes, so wie die Antwort sie nennt (0 = unbekannt).
    #[serde(default)]
    pub groesse: u64,
}

/// Ein Paket, das zu einem System gehört – samt Prüfsumme aus `SHA256SUMS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paketwahl {
    pub name: String,
    pub groesse: u64,
    pub art: PaketArt,
    /// SHA-256, klein und hexadezimal.
    pub sha256: String,
    /// Wo der Server es holt (`https`, in Tests Loopback).
    pub adresse: Option<String>,
}

/// Ist dieser Dateiname ein reiner Name (kein Pfad)?
fn ist_dateiname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.starts_with('.')
}

/// Ist die Adresse eine, von der quiver laden darf?
///
/// Im Betrieb ist das `https`. Loopback ist für Tests und die Entwicklung
/// erlaubt – wie bei der Quelle der Veröffentlichung.
pub fn adresse_erlaubt(adresse: &str) -> bool {
    let adresse = adresse.trim();
    if adresse.len() > 1024 {
        return false;
    }
    if adresse.starts_with("https://") {
        return true;
    }
    matches!(
        adresse.split('/').nth(2).map(|host| {
            let host = host.split(':').next().unwrap_or(host);
            host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "[::1]"
        }),
        Some(true)
    ) && adresse.starts_with("http://")
}

/// Die Datei, die zu diesem System gehört? (System und Architektur stehen im
/// Namen: `-windows-x86_64-`, `-macos-arm64.`, `-linux-x86_64.`, …)
pub fn datei_passt(name: &str, system: &str, arch: &str) -> bool {
    let klein = name.to_ascii_lowercase();
    if !klein.contains(&format!("-{system}-")) && !klein.contains(&format!("-{system}.")) {
        return false;
    }
    let arch = match arch {
        "x64" => vec!["x86_64"],
        "arm64" => vec!["arm64", "aarch64"],
        _ => return false,
    };
    arch.iter().any(|teil| klein.contains(teil))
}

/// Die Pakete einer Veröffentlichung für ein System, mit Prüfsummen.
///
/// `pruefsummen` ist der Inhalt von `SHA256SUMS`. Ein Paket ohne Prüfsumme wird
/// **nicht** angeboten: der Client könnte es nicht prüfen, und dann tauscht er
/// nicht.
pub fn pakete_fuer(
    veroeffentlichung: &Veroeffentlichung,
    system: &str,
    arch: &str,
    pruefsummen: &BTreeMap<String, String>,
) -> Vec<Paketwahl> {
    veroeffentlichung
        .dateien
        .iter()
        .filter(|datei| datei_passt(&datei.name, system, arch))
        // Ohne Adresse kann der Server nichts holen: dann gibt es kein Paket.
        .filter(|datei| datei.adresse.is_some())
        .filter_map(|datei| {
            let art = PaketArt::aus_name(&datei.name)?;
            let sha256 = pruefsummen.get(&datei.name)?.clone();
            // Ohne Größe lässt sich die Antwort des Servers nicht prüfen –
            // dann wird das Paket nicht angeboten.
            if !sha256_gueltig(&sha256) || datei.groesse == 0 || datei.groesse > PAKET_BYTES_MAX {
                return None;
            }
            Some(Paketwahl {
                name: datei.name.clone(),
                groesse: datei.groesse,
                art,
                sha256,
                adresse: datei.adresse.clone(),
            })
        })
        .collect()
}

/// Ist das eine brauchbare SHA-256-Prüfsumme (64 kleine Hex-Zeichen)?
pub fn sha256_gueltig(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `SHA256SUMS` lesen: je Zeile `<prüfsumme>  <name>` (auch ` *` statt `  `).
///
/// Unbrauchbare Zeilen fallen weg – eine unlesbare Datei ist kein Grund, die
/// ganze Veröffentlichung zu verwerfen.
pub fn pruefsummen_lesen(text: &str) -> BTreeMap<String, String> {
    let mut karte = BTreeMap::new();
    for zeile in text.lines() {
        let zeile = zeile.trim();
        if zeile.is_empty() || zeile.starts_with('#') {
            continue;
        }
        let mut teile = zeile.splitn(2, char::is_whitespace);
        let (Some(pruefsumme), Some(rest)) = (teile.next(), teile.next()) else {
            continue;
        };
        let name = rest.trim_start_matches([' ', '*']).trim();
        if !sha256_gueltig(&pruefsumme.to_ascii_lowercase()) || !ist_dateiname(name) {
            continue;
        }
        karte.insert(name.to_string(), pruefsumme.to_ascii_lowercase());
    }
    karte
}
