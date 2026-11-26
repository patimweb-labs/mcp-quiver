// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wie quiver auf diesem Gerät installiert ist (Aufgabe 39).
//!
//! Der Client erkennt das **selbst** – aus seinem eigenen Pfad, ohne Abfrage
//! beim Server und ohne Annahme über einen Hersteller. Daraus folgt, welches
//! Paket aus der Veröffentlichung zu ihm gehört und ob er es überhaupt tauschen
//! darf:
//!
//! | So liegt quiver | Art | Getauscht wird |
//! | --- | --- | --- |
//! | `…\quiver.exe` (portabel, `.zip`) | [`Installationsart::Zip`] | Datei daneben legen, alte umbenennen |
//! | `%LOCALAPPDATA%\Programs\mcp-quiver\quiver.exe` | [`Installationsart::Msi`] | `msiexec /qn`, das MSI startet neu |
//! | `….app/Contents/MacOS/quiver` | [`Installationsart::AppBundle`] | Bündel aus dem `.dmg` daneben, dann tauschen |
//! | `…/quiver-….AppImage` | [`Installationsart::AppImage`] | Datei ersetzen |
//! | `…/bin/quiver` (aus `.tar.gz`) | [`Installationsart::TarGz`] | Datei ersetzen |
//! | `/usr/bin/quiver` aus `.deb`/`.rpm` | [`Installationsart::Deb`]/[`Installationsart::Rpm`] | nur Hinweis (braucht root) |
//!
//! Liegt das Programm in einem Ordner ohne Schreibrecht (Installation für alle,
//! Geräteverwaltung), gibt es ebenfalls nur einen Hinweis. Alles hier ist reine
//! Rechnung über Pfade und ist darum ohne echtes System prüfbar.

use std::path::{Path, PathBuf};

/// Wie quiver hier liegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installationsart {
    /// Portables Windows-Paket (`.zip`).
    Zip,
    /// Windows-MSI je Benutzer.
    Msi,
    /// macOS-Bündel (`quiver.app`), getauscht aus dem `.dmg`.
    AppBundle,
    /// Linux-AppImage.
    AppImage,
    /// Linux-Baum aus `.tar.gz`.
    TarGz,
    /// Debian-Paket – braucht root, nur Hinweis.
    Deb,
    /// RPM-Paket – braucht root, nur Hinweis.
    Rpm,
    /// Unbekannt: kein Tausch, nur Hinweis.
    Unbekannt,
}

impl Installationsart {
    /// Darf quiver sich hier selbst tauschen?
    pub fn tauschbar(self) -> bool {
        matches!(
            self,
            Self::Zip | Self::Msi | Self::AppBundle | Self::AppImage | Self::TarGz
        )
    }

    /// Der kurze Name für Protokoll und Tests.
    pub fn als_text(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Msi => "msi",
            Self::AppBundle => "app",
            Self::AppImage => "appimage",
            Self::TarGz => "tar.gz",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Unbekannt => "unbekannt",
        }
    }
}

/// Was das System über sich sagt, ohne dass gefragt werden muss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Umgebung {
    /// `windows`, `macos`, `linux` – wie [`quiver_domain::System::dieses`].
    pub system: String,
    /// `%LOCALAPPDATA%` (nur Windows von Bedeutung).
    pub lokal: Option<PathBuf>,
    /// Gibt es die Debian-Datenbank (`/var/lib/dpkg/info`)?
    pub dpkg: bool,
}

impl Umgebung {
    /// Die Umgebung dieses Prozesses.
    pub fn dieses() -> Self {
        Self {
            system: quiver_domain::System::dieses().system,
            lokal: dirs::data_local_dir(),
            dpkg: Path::new("/var/lib/dpkg/info").is_dir(),
        }
    }
}

/// Wo quiver liegt und was daraus folgt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aufstellung {
    pub art: Installationsart,
    /// Was ersetzt wird: die Datei oder das Bündel.
    pub programm: PathBuf,
    /// Darf in diesen Ordner geschrieben werden?
    pub schreibbar: bool,
}

impl Aufstellung {
    /// Die Aufstellung dieses Prozesses.
    pub fn dieses() -> Self {
        let programm = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("quiver"));
        Self::aus(&programm, &Umgebung::dieses())
    }

    /// Eine Aufstellung, die nichts tauscht: für Tests und für den Fall, dass
    /// die Art der Installation unbekannt ist.
    pub fn nicht_tauschen() -> Self {
        Self {
            art: Installationsart::Unbekannt,
            programm: PathBuf::from("quiver"),
            schreibbar: false,
        }
    }

    /// Die Aufstellung zu einem Programmpfad.
    pub fn aus(programm: &Path, umgebung: &Umgebung) -> Self {
        let art = art_erkennen(programm, umgebung);
        let ziel = match art {
            Installationsart::AppBundle => bundel_von(programm).unwrap_or_else(|| programm.into()),
            _ => programm.to_path_buf(),
        };
        let schreibbar = ordner_schreibbar(&ziel);
        Self {
            art,
            programm: ziel,
            schreibbar,
        }
    }
}

/// Die Art der Installation aus dem Programmpfad erkennen.
pub fn art_erkennen(programm: &Path, umgebung: &Umgebung) -> Installationsart {
    // Pfade ohne Rücksicht auf das Trennzeichen vergleichen: `\` und `/`
    // gelten gleich (Windows kennt beide).
    let klein = programm
        .to_string_lossy()
        .to_ascii_lowercase()
        .replace('\\', "/");
    match umgebung.system.as_str() {
        "macos" => {
            if bundel_von(programm).is_some() {
                Installationsart::AppBundle
            } else {
                Installationsart::Unbekannt
            }
        }
        "windows" => {
            // Ein MSI je Benutzer liegt unter `%LOCALAPPDATA%\Programs\…`.
            let im_programme_ordner = umgebung.lokal.as_ref().is_some_and(|lokal| {
                pfad_beginnt_mit(
                    &klein,
                    &lokal
                        .join("Programs")
                        .to_string_lossy()
                        .to_lowercase()
                        .replace('\\', "/"),
                )
            });
            if im_programme_ordner {
                Installationsart::Msi
            } else {
                Installationsart::Zip
            }
        }
        "linux" => {
            if klein.ends_with(".appimage") {
                Installationsart::AppImage
            } else if pfad_beginnt_mit(&klein, "/usr/") || pfad_beginnt_mit(&klein, "/opt/") {
                if umgebung.dpkg {
                    Installationsart::Deb
                } else {
                    Installationsart::Rpm
                }
            } else {
                Installationsart::TarGz
            }
        }
        _ => Installationsart::Unbekannt,
    }
}

/// Beginnt `pfad` mit `anfang` (auf ganzen Pfadteilen)?
fn pfad_beginnt_mit(pfad: &str, anfang: &str) -> bool {
    let anfang = anfang.trim_end_matches(['/', '\\']);
    if !pfad.starts_with(anfang) {
        return false;
    }
    matches!(
        pfad.as_bytes().get(anfang.len()),
        None | Some(b'/') | Some(b'\\')
    )
}

/// Das Bündel zu einem Programm in `…/quiver.app/Contents/MacOS/quiver`:
/// drei Ebenen hoch. `None` heißt: kein Bündel.
pub fn bundel_von(programm: &Path) -> Option<PathBuf> {
    let macos = programm.parent()?;
    if !macos.file_name()?.eq_ignore_ascii_case("MacOS") {
        return None;
    }
    let contents = macos.parent()?;
    if !contents.file_name()?.eq_ignore_ascii_case("Contents") {
        return None;
    }
    let bundel = contents.parent()?;
    let name = bundel.file_name()?.to_string_lossy();
    name.to_ascii_lowercase()
        .ends_with(".app")
        .then(|| bundel.to_path_buf())
}

/// Darf in den Ordner dieses Pfades geschrieben werden?
///
/// Gefragt wird mit einer Probedatei: sie ist die einzige Antwort, die auch
/// eine Rechtevergabe über eine Gruppe oder eine Geräteverwaltung mitzählt.
/// Die Probe wird sofort wieder entfernt.
pub fn ordner_schreibbar(ziel: &Path) -> bool {
    let ordner = if ziel.is_dir() {
        ziel
    } else {
        ziel.parent().unwrap_or(ziel)
    };
    let probe = ordner.join(format!(".quiver-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests;
