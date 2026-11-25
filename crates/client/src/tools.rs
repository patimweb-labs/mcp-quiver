// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wo die mitgelieferten Werkzeuge liegen und wie aus einem Programmnamen ein
//! Pfad wird (Aufgabe 35).
//!
//! Ein Eintrag der Bibliothek nennt nur den **Programmnamen**
//! (`quiver-devops`). Ein Client aus dem Paket bringt die Werkzeuge selbst
//! mit und sucht sie deshalb zuerst neben dem eigenen Programm, dann im
//! `PATH`; ein fester Pfad funktioniert weiterhin.
//!
//! Gesucht wird überall gleich — Start, Anmeldung, Status, Einrichtung und
//! Testen gehen durch [`fuer_eintrag`]. Damit kann keine Stelle die Regel
//! umgehen.
//!
//! Die Meldung an den Benutzer nennt nie einen fremden Dateipfad, sondern nur
//! den Programmnamen und den Grund; der volle Pfad steht allein im örtlichen
//! Protokoll (`tracing`) und, beim Altbestand, in der Protokollseite.
//!
//! Welcher Ordner „neben dem Programm“ liegt, hängt von der Verpackung ab:
//!
//! | Paket | Programm | Werkzeuge |
//! |---|---|---|
//! | macOS | `quiver.app/Contents/MacOS/quiver` | `quiver.app/Contents/Resources/tools` |
//! | Windows | `<Installation>\quiver.exe` | `<Installation>\tools` |
//! | Linux (.deb/.rpm/.tar.gz) | `/usr/bin/quiver` | `/usr/lib/mcp-quiver/tools` |
//! | AppImage | `<AppDir>/usr/bin/quiver` | `<AppDir>/usr/lib/mcp-quiver/tools` |
//!
//! Statt die Plattform abzufragen (`cfg(target_os)`), werden die Kandidaten
//! der Reihe nach probiert und der erste genommen, der existiert
//! (AGENTS.md §2.10): dieselbe Regel gilt damit auch für ein Bündel, das
//! jemand anders zusammengestellt hat.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quiver_domain::{EintragId, Laufzeit};

use crate::protokoll::Protokoll;

/// Der Schalter für die Entwicklung: ein Ordner mit Werkzeugen, der alles
/// andere überschreibt.
pub const ORDNER_SCHALTER: &str = "QUIVER_TOOLS_DIR";

/// Woher ein Programm kommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Herkunft {
    /// Aus dem Ordner neben dem Programm — mitgeliefert.
    Mitgeliefert,
    /// Im `PATH` gefunden.
    Pfad,
    /// Ein fester Pfad aus der Bibliothek.
    Fest,
}

/// Ein gefundenes Programm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fund {
    /// Der Pfad, der gestartet wird.
    pub pfad: PathBuf,
    /// Woran er erkannt wurde.
    pub herkunft: Herkunft,
    /// Ein fester Pfad zeigte ins Leere, dieser Name liegt aber im Paket: der
    /// alte Eintrag der Bibliothek heilt sich so von selbst (Altbestand).
    pub ersetzt: Option<String>,
}

impl Fund {
    /// Der Programmname, wie ihn der Benutzer sieht: `quiver-devops`.
    pub fn name(&self) -> String {
        dateiname(&self.pfad.to_string_lossy())
    }
}

/// Warum kein Programm gefunden wurde.
///
/// Die Meldung nennt nur den Namen und den Grund — nie einen Pfad: sie steht
/// auf der Tool-Karte im Fenster (Aufgabe 35).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NichtGefunden {
    #[error("»{0}« wurde nicht gefunden – weder mitgeliefert noch im PATH")]
    Nirgends(String),
    #[error("für diesen Eintrag ist kein Programm eingetragen")]
    OhneNamen,
}

/// Wo gesucht wird. Aus der Umgebung gebaut ([`Suche::aus_umgebung`]) oder im
/// Test von Hand gesetzt.
#[derive(Debug, Clone)]
pub struct Suche {
    /// Das laufende Programm; der Ordner daneben gehört dem Paket.
    pub programm: PathBuf,
    /// `QUIVER_TOOLS_DIR`, wenn gesetzt (Entwicklung).
    pub eigener: Option<PathBuf>,
    /// Die Einträge von `PATH`.
    pub pfad: Vec<PathBuf>,
    /// `APPDIR` eines laufenden AppImage, wenn gesetzt.
    pub appdir: Option<PathBuf>,
}

impl Suche {
    /// Die Suche, wie sie im Betrieb gilt – gemerkt, solange sich die Umgebung
    /// nicht ändert.
    ///
    /// Sie hängt nur an der Umgebung und am eigenen Programm, beides ändert
    /// sich im Betrieb nicht mehr; deshalb wird sie gemerkt, statt bei jedem
    /// Eintrag und jedem Bild des Fensters neu aufgebaut zu werden. Ändert
    /// sich `QUIVER_TOOLS_DIR` doch (Tests, Werkzeuge aus einem Checkout),
    /// wird sie neu gebaut — sonst liefe ein Test mit dem Ordner eines anderen.
    pub fn gemerkt() -> Suche {
        static SUCHE: Mutex<Option<(Option<PathBuf>, Suche)>> = Mutex::new(None);
        let eigener = umgebungsordner(ORDNER_SCHALTER);
        let mut fach = SUCHE
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        if let Some((schalter, suche)) = fach.as_ref() {
            if *schalter == eigener {
                return suche.clone();
            }
        }
        let suche = Suche::aus_umgebung();
        *fach = Some((eigener, suche.clone()));
        suche
    }

    /// Die Suche, wie sie im Betrieb gilt.
    pub fn aus_umgebung() -> Self {
        Self {
            programm: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("quiver")),
            eigener: umgebungsordner(ORDNER_SCHALTER),
            pfad: std::env::var_os("PATH")
                .map(|pfad| std::env::split_paths(&pfad).collect())
                .unwrap_or_default(),
            appdir: umgebungsordner("APPDIR"),
        }
    }

    /// Der Ordner mit den mitgelieferten Werkzeugen: der erste, den es gibt.
    ///
    /// `QUIVER_TOOLS_DIR` gilt, wenn gesetzt — auch wenn der Ordner leer ist.
    pub fn ordner(&self) -> Option<PathBuf> {
        if let Some(eigener) = &self.eigener {
            return Some(eigener.clone());
        }
        let daneben = self.programm.parent().unwrap_or(Path::new("."));
        // Ein `..` steht nicht im Weg: die Ebene darüber wird gerechnet, damit
        // der gefundene Pfad ohne Umweg lesbar bleibt.
        let darueber = daneben.parent().unwrap_or(daneben);
        let mut kandidaten = vec![
            daneben.join("tools"),
            daneben.join("Resources").join("tools"),
            darueber.join("Resources").join("tools"),
            darueber.join("lib").join("mcp-quiver").join("tools"),
        ];
        if let Some(appdir) = &self.appdir {
            kandidaten.push(
                appdir
                    .join("usr")
                    .join("lib")
                    .join("mcp-quiver")
                    .join("tools"),
            );
        }
        kandidaten.into_iter().find(|ordner| ordner.is_dir())
    }

    /// Ein Programm finden.
    ///
    /// Ein Name ohne Trennzeichen wird zuerst im Paket gesucht, dann im
    /// `PATH`; ein Pfad (mit `/` oder `\`) gilt, wie er dasteht. Gibt es ihn
    /// nicht, dessen Dateiname aber schon, kommt die mitgelieferte Fassung.
    pub fn finden(&self, programm: &str) -> Result<Fund, NichtGefunden> {
        let programm = programm.trim();
        if programm.is_empty() {
            return Err(NichtGefunden::OhneNamen);
        }
        if ist_pfad(programm) {
            let pfad = Path::new(programm);
            if pfad.is_file() {
                return Ok(Fund {
                    pfad: pfad.to_path_buf(),
                    herkunft: Herkunft::Fest,
                    ersetzt: None,
                });
            }
            let name = dateiname(programm);
            if let Some(mut fund) = self.mitgeliefert(&name) {
                fund.ersetzt = Some(programm.to_string());
                return Ok(fund);
            }
            return Err(NichtGefunden::Nirgends(name));
        }
        if let Some(fund) = self.mitgeliefert(programm) {
            return Ok(fund);
        }
        self.im_pfad(programm)
            .ok_or_else(|| NichtGefunden::Nirgends(programm.to_string()))
    }

    /// Im Ordner neben dem Programm suchen — nur unter dem reinen Namen.
    fn mitgeliefert(&self, name: &str) -> Option<Fund> {
        let ordner = self.ordner()?;
        let pfad = im_ordner(&ordner, name)?;
        Some(Fund {
            pfad,
            herkunft: Herkunft::Mitgeliefert,
            ersetzt: None,
        })
    }

    /// Im `PATH` suchen.
    fn im_pfad(&self, name: &str) -> Option<Fund> {
        for ordner in &self.pfad {
            for kandidat in kandidaten(name) {
                let pfad = ordner.join(kandidat);
                if pfad.is_file() {
                    return Some(Fund {
                        pfad,
                        herkunft: Herkunft::Pfad,
                        ersetzt: None,
                    });
                }
            }
        }
        None
    }
}

/// Ein Programm für einen Eintrag suchen und den Altbestand vermerken.
///
/// Die eine Stelle, durch die Start, Anmeldung, Status, Einrichtung und
/// Testen gehen.
pub(crate) fn fuer_eintrag(
    id: &EintragId,
    programm: &str,
    protokoll: &Protokoll,
) -> anyhow::Result<Fund> {
    let suche = Suche::gemerkt();
    let fund = match suche.finden(programm) {
        Ok(fund) => fund,
        Err(fehler) => {
            // Der Benutzer kann nichts tun, der Admin muss das Werkzeug
            // mitliefern: der Hinweis gehört ins Protokoll, nicht auf die
            // Karte (Aufgabe 35, §5).
            tracing::warn!(eintrag = %id, "Programm nicht gefunden: {fehler}");
            protokoll.zeile_schreiben(
                id,
                &format!(
                    "{fehler}. Bitte den Admin bitten, das Programm mit dem Paket \
                     mitzuliefern oder in den PATH zu legen."
                ),
            );
            return Err(fehler.into());
        }
    };
    tracing::debug!(eintrag = %id, programm = %fund.pfad.display(), "Programm gefunden");
    if let Some(alt) = &fund.ersetzt {
        // Der alte Eintrag der Bibliothek zeigt auf einen Pfad, den es hier
        // nicht gibt; der Name ist aber mitgeliefert. Der volle Pfad gehört
        // ins Protokoll, nicht in eine Meldung.
        tracing::info!(eintrag = %id, "»{alt}« gibt es nicht, es läuft {}", fund.pfad.display());
        protokoll.zeile_schreiben(
            id,
            &format!(
                "»{alt}« gibt es auf diesem Gerät nicht – es läuft die mitgelieferte Fassung von {}.",
                fund.name()
            ),
        );
    }
    Ok(fund)
}

/// Fehlt das Programm dieses Eintrags auf diesem Gerät?
///
/// `Some(name)`, wenn der Eintrag ein Programm startet, das weder mitgeliefert
/// noch im `PATH` liegt. Die Karte schreibt dann „Dieses Tool ist auf diesem
/// Gerät nicht installiert.“ statt eines technischen Fehlers und bietet keine
/// Anmeldung an (Aufgabe 35, §5).
///
/// Gemeldet wird nur „gibt es nicht“, nie ein Pfad: auch diese Frage bekommt
/// niemanden zu sehen, der nicht auf diesem Gerät sitzt.
pub(crate) fn fehlt(laufzeit: &Laufzeit) -> Option<String> {
    let Laufzeit::Stdio { befehl, .. } = laufzeit else {
        return None;
    };
    match Suche::gemerkt().finden(befehl) {
        Ok(_) => None,
        Err(NichtGefunden::OhneNamen) => None,
        Err(NichtGefunden::Nirgends(name)) => Some(name),
    }
}

/// Gibt es den Ordner mit den mitgelieferten Werkzeugen neben dem Programm
/// (Aufgabe 35, §6)?
///
/// `false` heißt: ein Programm, das der Eintrag über seinen bloßen Namen
/// nennt, ist hier nicht zu finden – bei einem entpackten ZIP fehlt dann der
/// Ordner `tools`.
pub(crate) fn ordner_da() -> bool {
    Suche::gemerkt().ordner().is_some()
}

/// Ist das ein Pfad und kein bloßer Name? (auch `C:\…`, das auf einem anderen
/// System kein absoluter Pfad ist)
fn ist_pfad(programm: &str) -> bool {
    programm.contains('/') || programm.contains('\\')
}

/// Die Namen, unter denen ein Programm in einem Ordner liegen kann: der Name
/// selbst und, unter Windows, derselbe mit `.exe`.
fn kandidaten(name: &str) -> Vec<String> {
    // Alles, was aus dem Ordner herausführen könnte, wird nicht gesucht.
    if name.is_empty() || name == "." || name == ".." || ist_pfad(name) {
        return Vec::new();
    }
    let mut aus = vec![name.to_string()];
    if !name.to_ascii_lowercase().ends_with(".exe") {
        aus.push(format!("{name}.exe"));
    }
    aus
}

/// Ein Programm im Ordner der mitgelieferten Werkzeuge suchen — ohne Ausbruch.
///
/// Ein Name mit Trennzeichen wird nie angehängt, und ein Verweis, der aus dem
/// Ordner herauszeigt, gilt nicht: das Paket ist die Grenze.
fn im_ordner(ordner: &Path, name: &str) -> Option<PathBuf> {
    let Ok(basis) = ordner.canonicalize() else {
        return None;
    };
    for kandidat in kandidaten(name) {
        let pfad = ordner.join(kandidat);
        if !pfad.is_file() {
            continue;
        }
        match pfad.canonicalize() {
            Ok(echt) if echt.starts_with(&basis) => return Some(pfad),
            _ => continue,
        }
    }
    None
}

/// Der Programmname ohne Ordner und ohne `.exe` — so, wie ihn ein Benutzer
/// nennt.
fn dateiname(programm: &str) -> String {
    let name = programm
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(programm)
        .trim();
    let name = if name.to_ascii_lowercase().ends_with(".exe") {
        &name[..name.len() - 4]
    } else {
        name
    };
    if name.is_empty() {
        programm.trim().to_string()
    } else {
        name.to_string()
    }
}

/// Eine Umgebungsvariable als Ordner; ein leerer Wert zählt nicht.
fn umgebungsordner(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|pfad| !pfad.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Ein eigener Ordner je Test unter `target` — kein Aufräumen nötig, und
    /// kein Test sieht die Bäume eines anderen.
    fn baum(name: &str) -> PathBuf {
        let ordner = std::env::temp_dir().join(format!(
            "quiver-tools-{}-{name}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        fs::remove_dir_all(&ordner).ok();
        fs::create_dir_all(&ordner).expect("Testordner");
        ordner
    }

    /// Ein Programm anlegen — der Inhalt ist gleichgültig, es muss nur eine
    /// Datei sein.
    fn werkzeug(pfad: &Path) {
        fs::create_dir_all(pfad.parent().expect("Ordner")).expect("Ordner");
        fs::write(pfad, b"#!/bin/sh\n").expect("Programm");
    }

    fn suche(programm: &Path) -> Suche {
        Suche {
            programm: programm.to_path_buf(),
            eigener: None,
            pfad: Vec::new(),
            appdir: None,
        }
    }

    #[test]
    fn macos_buendel_findet_die_werkzeuge_in_resources() {
        let baum = baum("macos");
        let programm = baum.join("quiver.app/Contents/MacOS/quiver");
        werkzeug(&programm);
        werkzeug(&baum.join("quiver.app/Contents/Resources/tools/quiver-devops"));

        let fund = suche(&programm).finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Mitgeliefert);
        assert_eq!(
            fund.pfad,
            baum.join("quiver.app/Contents/Resources/tools/quiver-devops")
        );
        assert_eq!(fund.name(), "quiver-devops");
        assert_eq!(fund.ersetzt, None);
    }

    #[test]
    fn windows_installation_findet_die_werkzeuge_im_ordner_tools() {
        let baum = baum("windows");
        let programm = baum.join("Programme/mcp-quiver/quiver.exe");
        werkzeug(&programm);
        werkzeug(&baum.join("Programme/mcp-quiver/tools/quiver-devops.exe"));

        let mut gesucht = suche(&programm);
        gesucht.pfad = vec![baum.join("Programme/mcp-quiver/tools")];
        let fund = gesucht.finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Mitgeliefert);
        assert_eq!(fund.name(), "quiver-devops", "ohne .exe");
    }

    #[test]
    fn linux_paket_findet_die_werkzeuge_unter_lib() {
        let baum = baum("linux");
        let programm = baum.join("usr/bin/quiver");
        werkzeug(&programm);
        werkzeug(&baum.join("usr/lib/mcp-quiver/tools/quiver-devops"));

        let fund = suche(&programm).finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Mitgeliefert);
        assert_eq!(
            fund.pfad,
            baum.join("usr/lib/mcp-quiver/tools/quiver-devops")
        );
    }

    #[test]
    fn appimage_findet_die_werkzeuge_ueber_appdir() {
        let baum = baum("appimage");
        let appdir = baum.join("AppDir");
        // Das Programm steht woanders (etwa entpackt): APPDIR entscheidet.
        let programm = baum.join("sonstwo/quiver");
        werkzeug(&programm);
        werkzeug(&appdir.join("usr/lib/mcp-quiver/tools/quiver-devops"));

        let mut gesucht = suche(&programm);
        gesucht.appdir = Some(appdir.clone());
        let fund = gesucht.finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Mitgeliefert);
        assert_eq!(
            fund.pfad,
            appdir.join("usr/lib/mcp-quiver/tools/quiver-devops")
        );
    }

    #[test]
    fn eigener_ordner_ueberschreibt_das_paket() {
        let baum = baum("eigener");
        let programm = baum.join("quiver.app/Contents/MacOS/quiver");
        werkzeug(&programm);
        werkzeug(&baum.join("quiver.app/Contents/Resources/tools/quiver-devops"));
        let eigen = baum.join("aus-checkout");
        werkzeug(&eigen.join("quiver-devops"));

        let mut gesucht = suche(&programm);
        gesucht.eigener = Some(eigen.clone());
        let fund = gesucht.finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.pfad, eigen.join("quiver-devops"));

        // Ein leerer eigener Ordner heißt: nichts mitgeliefert.
        let leer = baum.join("leer");
        fs::create_dir_all(&leer).expect("Ordner");
        gesucht.eigener = Some(leer);
        assert_eq!(
            gesucht.finden("quiver-devops"),
            Err(NichtGefunden::Nirgends("quiver-devops".into()))
        );
    }

    #[test]
    fn der_path_gilt_erst_nach_dem_paket() {
        let baum = baum("pfad");
        let programm = baum.join("usr/bin/quiver");
        werkzeug(&programm);
        werkzeug(&baum.join("usr/lib/mcp-quiver/tools/quiver-devops"));
        let im_pfad = baum.join("pfad");
        werkzeug(&im_pfad.join("quiver-devops"));
        werkzeug(&im_pfad.join("quiver-jira.exe"));

        let mut gesucht = suche(&programm);
        gesucht.pfad = vec![im_pfad.clone()];
        let fund = gesucht.finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Mitgeliefert, "das Paket geht vor");

        // Was nicht mitgeliefert ist, kommt aus dem PATH — auch als .exe.
        let fund = gesucht.finden("quiver-jira").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Pfad);
        assert_eq!(fund.pfad, im_pfad.join("quiver-jira.exe"));
        assert_eq!(fund.name(), "quiver-jira");

        assert_eq!(
            gesucht.finden("quiver-gibtsnicht"),
            Err(NichtGefunden::Nirgends("quiver-gibtsnicht".into()))
        );
    }

    #[test]
    fn ein_fester_pfad_gilt_wie_er_dasteht() {
        let baum = baum("fest");
        let programm = baum.join("usr/bin/quiver");
        werkzeug(&programm);
        let anderswo = baum.join("opt/werkzeuge/quiver-devops");
        werkzeug(&anderswo);

        let gesucht = suche(&programm);
        let fund = gesucht
            .finden(&anderswo.to_string_lossy())
            .expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Fest);
        assert_eq!(fund.pfad, anderswo);
        assert_eq!(fund.ersetzt, None);
    }

    #[test]
    fn altbestand_heilt_sich_aus_dem_paket() {
        // Der Server kennt den Eintrag noch mit dem Pfad einer
        // Entwicklermaschine; derselbe Name liegt im Paket.
        let baum = baum("altbestand");
        let programm = baum.join("usr/bin/quiver");
        werkzeug(&programm);
        werkzeug(&baum.join("usr/lib/mcp-quiver/tools/quiver-devops"));
        let alt = "/home/entwickler/quiver-tools/target/release/quiver-devops";

        let fund = suche(&programm).finden(alt).expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Mitgeliefert);
        assert_eq!(
            fund.pfad,
            baum.join("usr/lib/mcp-quiver/tools/quiver-devops")
        );
        assert_eq!(fund.ersetzt.as_deref(), Some(alt));
    }

    #[test]
    fn ein_alter_pfad_ohne_namen_im_paket_bleibt_ein_fehler() {
        let baum = baum("altbestand-fehler");
        let programm = baum.join("usr/bin/quiver");
        werkzeug(&programm);
        werkzeug(&baum.join("usr/lib/mcp-quiver/tools/quiver-devops"));

        let fehler = suche(&programm)
            .finden("/home/entwickler/quiver-tools/target/release/quiver-nix")
            .expect_err("nicht gefunden");
        assert_eq!(fehler, NichtGefunden::Nirgends("quiver-nix".into()));
        // Die Meldung nennt den Namen und den Grund, keinen Pfad.
        let text = fehler.to_string();
        assert!(text.contains("quiver-nix"), "{text}");
        assert!(!text.contains("entwickler"), "{text}");
    }

    #[test]
    fn kein_name_mit_trennzeichen_wird_angehaengt() {
        let baum = baum("ausbruch");
        let programm = baum.join("usr/bin/quiver");
        werkzeug(&programm);
        let werkzeuge = baum.join("usr/lib/mcp-quiver/tools");
        werkzeug(&werkzeuge.join("quiver-devops"));
        // Ein Verweis nach draußen: er gilt nicht.
        let draussen = baum.join("draussen/quiver-boese");
        werkzeug(&draussen);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&draussen, werkzeuge.join("quiver-boese")).expect("Verweis");

        let gesucht = suche(&programm);
        // Kein Ausbruch: weder mit `..` noch nach draußen.
        assert!(gesucht.finden("../../etc/passwd").is_err());
        #[cfg(unix)]
        assert!(
            gesucht.finden("quiver-boese").is_err(),
            "ein Verweis aus dem Ordner heraus gilt nicht"
        );
        assert!(gesucht.finden("").is_err());
    }

    #[test]
    fn der_name_bleibt_der_name() {
        assert_eq!(dateiname("quiver-devops"), "quiver-devops");
        assert_eq!(dateiname("quiver-devops.exe"), "quiver-devops");
        assert_eq!(
            dateiname(r"C:\Program Files\mcp-quiver\tools\quiver-m365.exe"),
            "quiver-m365"
        );
        assert_eq!(
            dateiname("/usr/lib/mcp-quiver/tools/quiver-m365"),
            "quiver-m365"
        );
        assert_eq!(dateiname("  quiver-ssh  "), "quiver-ssh");
    }
}
