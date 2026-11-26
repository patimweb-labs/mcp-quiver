// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Vorgaben der Firma (Aufgabe 34, §3).
//!
//! Die Verwaltung kann drei Dinge vorgeben; sie haben **Vorrang vor dem
//! Profil**, sind nur lesbar und werden von quiver nie geschrieben:
//!
//! | Feld | Windows | macOS | Linux |
//! | --- | --- | --- | --- |
//! | `ServerUrl` | `HKLM\Software\Policies\mcp-quiver` | verwaltete Einstellungen `de.weppelmann.mcp-quiver` | `/etc/mcp-quiver/vorgaben.json` |
//! | `Autostart` | dito (DWORD 1/0) | dito | dito |
//! | `UpdateHinweis` | dito | dito | dito |
//!
//! Gelesen wird **nur** der verwaltete Bereich: unter macOS ausschließlich
//! `/Library/Managed Preferences/…` (nicht die Einstellungen des Benutzers),
//! unter Linux nur eine Datei, die root gehört und nicht welt-schreibbar ist.
//! Ungültige Werte werden verworfen und im Protokoll genannt; ohne Vorgaben
//! gilt alles wie bisher.
//!
//! Der Ort ist verlegbar ([`Ort::Datei`]) – Tests schreiben nie nach
//! `/etc`, `/Library` oder in die Registry. Im Betrieb gilt [`Ort::System`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Die Kennung der Vorgaben (macOS-Domain, Linux-Ordner).
pub const KENNUNG: &str = "de.weppelmann.mcp-quiver";
/// Der Registry-Schlüssel der Vorgaben unter Windows (unter `HKLM`).
pub const WINDOWS_SCHLUESSEL: &str = r"Software\Policies\mcp-quiver";
/// Die Datei der Vorgaben unter Linux.
pub const LINUX_DATEI: &str = "/etc/mcp-quiver/vorgaben.json";
/// Die verwalteten Einstellungen unter macOS (nur der verwaltete Bereich).
pub const MACOS_PLIST: &str = "/Library/Managed Preferences/de.weppelmann.mcp-quiver.plist";
/// Umgebungsvariable, die den Ort im Entwicklungsbau verlegt (Tests).
pub const UMGEBUNG: &str = "QUIVER_VORGABEN";

/// So lang darf ein Hinweistext höchstens sein.
pub const HINWEIS_MAX: usize = 200;

/// Was die Verwaltung vorgeben kann.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vorgaben {
    /// Feste Server-Adresse; im Fenster nicht änderbar.
    #[serde(rename = "ServerUrl", default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    /// Autostart an oder aus.
    #[serde(rename = "Autostart", default, skip_serializing_if = "Option::is_none")]
    pub autostart: Option<bool>,
    /// Text des Update-Hinweises; `{version}` wird ersetzt.
    #[serde(
        rename = "UpdateHinweis",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub update_hinweis: Option<String>,
}

impl Vorgaben {
    /// Ist nichts vorgegeben?
    pub fn ist_leer(&self) -> bool {
        self.server_url.is_none() && self.autostart.is_none() && self.update_hinweis.is_none()
    }

    /// Wird dieses Gerät über die Geräteverwaltung versorgt?
    pub fn verwaltet(&self) -> bool {
        !self.ist_leer()
    }

    /// Ungültige Werte verwerfen und jeden Fund im Protokoll nennen.
    ///
    /// Verworfen wird nur das betroffene Feld: eine falsche Server-Adresse
    /// nimmt den Autostart nicht mit.
    pub fn bereinigen(mut self) -> Self {
        if let Some(url) = self.server_url.take() {
            match server_url_bereinigen(&url) {
                Some(url) => self.server_url = Some(url),
                None => tracing::warn!(
                    "Vorgabe ServerUrl {url:?} ist keine brauchbare Adresse; sie wird ignoriert"
                ),
            }
        }
        if let Some(text) = self.update_hinweis.take() {
            match hinweis_bereinigen(&text) {
                Some(text) => self.update_hinweis = Some(text),
                None => tracing::warn!("Vorgabe UpdateHinweis ist leer; sie wird ignoriert"),
            }
        }
        self
    }
}

/// Wo die Vorgaben liegen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ort {
    /// Der Ort des Systems (Registry, verwaltete Einstellungen, `/etc`).
    System,
    /// Eine Datei (JSON oder XML-plist). Für Tests und den Entwicklungsbau.
    Datei(PathBuf),
}

impl Ort {
    /// Der Ort des Systems – im Entwicklungsbau durch [`UMGEBUNG`] verlegt.
    pub fn aus_umgebung() -> Self {
        if cfg!(debug_assertions) {
            if let Some(pfad) = std::env::var_os(UMGEBUNG).filter(|wert| !wert.is_empty()) {
                return Self::Datei(PathBuf::from(pfad));
            }
        }
        Self::System
    }
}

/// Die Vorgaben lesen. Fehlt die Ablage oder ist sie unlesbar, gilt „keine
/// Vorgaben“ – quiver startet weiter.
pub fn lesen(ort: &Ort) -> Vorgaben {
    match ort {
        Ort::Datei(pfad) => aus_datei(pfad),
        Ort::System => aus_system(),
    }
    .bereinigen()
}

/// Eine Datei lesen: `.plist` als XML-plist, alles andere als JSON.
pub fn aus_datei(pfad: &Path) -> Vorgaben {
    let text = match std::fs::read_to_string(pfad) {
        Ok(text) => text,
        Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => return Vorgaben::default(),
        Err(fehler) => {
            tracing::warn!("Vorgaben {} nicht lesbar: {fehler}", pfad.display());
            return Vorgaben::default();
        }
    };
    if pfad
        .extension()
        .is_some_and(|endung| endung.eq_ignore_ascii_case("plist"))
    {
        aus_plist(&text)
    } else {
        aus_json(&text)
    }
}

/// Die Vorgaben aus JSON lesen (Linux, Tests).
pub fn aus_json(text: &str) -> Vorgaben {
    match serde_json::from_str::<Vorgaben>(text) {
        Ok(vorgaben) => vorgaben,
        Err(fehler) => {
            tracing::warn!("Vorgaben sind kein brauchbares JSON: {fehler}");
            Vorgaben::default()
        }
    }
}

/// Die Vorgaben aus den Werten einer plist bauen (macOS, Tests).
pub fn aus_plist(text: &str) -> Vorgaben {
    let werte = plist_werte(text);
    let wert = |name: &str| werte.get(name).cloned();
    Vorgaben {
        server_url: wert("ServerUrl"),
        autostart: wert("Autostart").and_then(|wert| bool_lesen(&wert)),
        update_hinweis: wert("UpdateHinweis"),
    }
}

/// Die Vorgaben aus einer Liste von Namen und Werten bauen (Windows, Tests).
///
/// Der Autostart darf dort ein DWORD sein; die Registry liefert ihn deshalb
/// als Text (`1`/`0`) und [`bool_lesen`] versteht beides.
pub fn aus_werten(werte: &BTreeMap<String, String>) -> Vorgaben {
    Vorgaben {
        server_url: werte.get("ServerUrl").cloned(),
        autostart: werte.get("Autostart").and_then(|wert| bool_lesen(wert)),
        update_hinweis: werte.get("UpdateHinweis").cloned(),
    }
}

/// `1`/`0`, `true`/`false`, `an`/`aus` – alles andere ist kein Schalter.
pub fn bool_lesen(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "an" | "ja" | "yes" => Some(true),
        "0" | "false" | "aus" | "nein" | "no" => Some(false),
        _ => None,
    }
}

/// Eine Server-Adresse prüfen und ohne Schrägstrich am Ende zurückgeben.
///
/// `https` ist immer erlaubt; `http` nur auf der eigenen Maschine (so wie der
/// Server selbst es hält).
pub fn server_url_bereinigen(text: &str) -> Option<String> {
    let text = text.trim().trim_end_matches('/');
    if text.len() < 8 || text.len() > 512 {
        return None;
    }
    let rest = if let Some(rest) = text.strip_prefix("https://") {
        rest
    } else {
        let rest = text.strip_prefix("http://")?;
        // `http` nur auf der eigenen Maschine.
        ist_loopback(rest).then_some(rest)?
    };
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return None;
    }
    Some(text.to_string())
}

/// Ist der Anfang einer Adresse die eigene Maschine?
fn ist_loopback(rest: &str) -> bool {
    let host = rest.split('/').next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

/// Einen Hinweistext prüfen: eine Zeile, nicht leer, nicht zu lang.
pub fn hinweis_bereinigen(text: &str) -> Option<String> {
    let text: String = text
        .chars()
        .map(|zeichen| if zeichen.is_control() { ' ' } else { zeichen })
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.chars().count() > HINWEIS_MAX {
        return None;
    }
    Some(text)
}

// --------------------------------------------------------------- plist

/// Die `<key>`/Wert-Paare einer XML-plist lesen.
///
/// Bewusst klein: nur die Formen, die eine `.mobileconfig` für diese drei
/// Werte erzeugt (`<string>`, `<true/>`, `<false/>`, `<integer>`). Alles
/// andere wird übergangen – es wird nichts geraten.
pub fn plist_werte(text: &str) -> BTreeMap<String, String> {
    let mut werte = BTreeMap::new();
    let mut rest = text;
    while let Some((vor, nach)) = rest.split_once("<key>") {
        let _ = vor;
        let Some((name, nach_name)) = nach.split_once("</key>") else {
            break;
        };
        rest = nach_name;
        let nach_name = rest.trim_start();
        let wert = if let Some(inner) = nach_name.strip_prefix("<string>") {
            inner
                .split_once("</string>")
                .map(|(wert, _)| wert.trim().to_string())
        } else if let Some(inner) = nach_name.strip_prefix("<integer>") {
            inner
                .split_once("</integer>")
                .map(|(wert, _)| wert.trim().to_string())
        } else if nach_name.starts_with("<true/>") {
            Some(String::from("true"))
        } else if nach_name.starts_with("<false/>") {
            Some(String::from("false"))
        } else {
            None
        };
        if let Some(wert) = wert {
            werte.insert(decode(name.trim()), decode(&wert));
        }
    }
    werte
}

/// Die wenigen XML-Ersetzungen, die in Werten vorkommen können.
fn decode(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// -------------------------------------------------------------- System

fn aus_system() -> Vorgaben {
    if cfg!(target_os = "windows") {
        return windows::lesen();
    }
    if cfg!(target_os = "macos") {
        return macos::lesen();
    }
    linux::lesen()
}

/// Linux: `/etc/mcp-quiver/vorgaben.json` – nur root-eigen und nicht
/// welt-schreibbar.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod linux {
    use std::os::unix::fs::MetadataExt;

    use super::{aus_json, Vorgaben, LINUX_DATEI};

    pub fn lesen() -> Vorgaben {
        let pfad = std::path::Path::new(LINUX_DATEI);
        let Ok(meta) = std::fs::metadata(pfad) else {
            return Vorgaben::default();
        };
        // Welt-schreibbar oder nicht root: nicht vertrauenswürdig.
        if meta.mode() & 0o002 != 0 || meta.uid() != 0 {
            tracing::warn!(
                "{LINUX_DATEI} gehört nicht root oder ist welt-schreibbar; \
                 die Vorgaben werden ignoriert"
            );
            return Vorgaben::default();
        }
        match std::fs::read_to_string(pfad) {
            Ok(text) => aus_json(&text),
            Err(fehler) => {
                tracing::warn!("{LINUX_DATEI} nicht lesbar: {fehler}");
                Vorgaben::default()
            }
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod linux {
    use super::Vorgaben;

    pub fn lesen() -> Vorgaben {
        Vorgaben::default()
    }
}

/// Windows: `HKLM\Software\Policies\mcp-quiver`.
mod windows {
    use super::Vorgaben;

    #[cfg(target_os = "windows")]
    pub fn lesen() -> Vorgaben {
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
        use winreg::RegKey;

        let schluessel = super::WINDOWS_SCHLUESSEL;
        let key = match RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(schluessel, KEY_READ | KEY_WOW64_64KEY)
        {
            Ok(key) => key,
            Err(_) => return Vorgaben::default(),
        };
        let mut werte = std::collections::BTreeMap::new();
        for name in ["ServerUrl", "UpdateHinweis"] {
            if let Ok(wert) = key.get_value::<String, _>(name) {
                werte.insert(name.to_string(), wert);
            }
        }
        if let Ok(wert) = key.get_value::<u32, _>("Autostart") {
            werte.insert(String::from("Autostart"), wert.to_string());
        }
        super::aus_werten(&werte)
    }

    #[cfg(not(target_os = "windows"))]
    pub fn lesen() -> Vorgaben {
        Vorgaben::default()
    }
}

/// macOS: nur die verwalteten Einstellungen unter `/Library/Managed
/// Preferences`. Sie können binär vorliegen; `plutil` (Bordmittel) macht daraus
/// XML, das hier gelesen wird.
mod macos {
    use super::{aus_plist, Vorgaben, MACOS_PLIST};

    #[cfg(target_os = "macos")]
    pub fn lesen() -> Vorgaben {
        if !std::path::Path::new(MACOS_PLIST).is_file() {
            return Vorgaben::default();
        }
        // `plutil` ohne Shell: der Pfad ist fest und enthält nichts Fremdes.
        let ausgabe = match std::process::Command::new("plutil")
            .args(["-convert", "xml1", "-o", "-", MACOS_PLIST])
            .output()
        {
            Ok(ausgabe) if ausgabe.status.success() => ausgabe.stdout,
            Ok(ausgabe) => {
                tracing::warn!(
                    "{MACOS_PLIST} nicht lesbar (plutil: {})",
                    String::from_utf8_lossy(&ausgabe.stderr).trim()
                );
                return Vorgaben::default();
            }
            Err(fehler) => {
                tracing::warn!("plutil nicht ausführbar: {fehler}");
                return Vorgaben::default();
            }
        };
        match String::from_utf8(ausgabe) {
            Ok(text) => aus_plist(&text),
            Err(_) => {
                tracing::warn!("{MACOS_PLIST} ist kein UTF-8");
                Vorgaben::default()
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn lesen() -> Vorgaben {
        Vorgaben::default()
    }
}

#[cfg(test)]
mod tests;
