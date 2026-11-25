// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Welches Programm ein Eintrag startet (Aufgabe 37).
//!
//! Der Client bringt **keine** Werkzeuge mehr mit. Es gibt zwei Fälle:
//!
//! - **Vom Server verteilt:** ein reiner Name, der mit `quiver-` beginnt. Das
//!   Programm liegt im Profil unter `tools/<name>/<sha256>/<programm>`
//!   ([`crate::werkzeuge`]) und wird vor jedem Start geprüft.
//! - **Alles andere:** `npx`, `uvx`, `docker`, ein Pfad zu einem eigenen
//!   Programm. Das gilt wie bisher: ein reiner Name wird im `PATH` gesucht,
//!   ein Pfad genommen, wie er dasteht.
//!
//! Ein Eintrag mit einem **alten festen Pfad** auf ein verteiltes Programm
//! heilt sich selbst: es zählt nur noch der Programmname (Altbestand).
//!
//! [`ORDNER_SCHALTER`] (`QUIVER_TOOLS_DIR`) bleibt für die Entwicklung: ein
//! Ordner, in dem die Programme flach liegen, überschreibt die Ablage im
//! Profil. Damit lässt sich ein ausgecheckter Stand ohne Server ausprobieren.
//!
//! Die Meldung an den Benutzer nennt nie einen fremden Dateipfad, sondern nur
//! den Programmnamen und den Grund; der volle Pfad steht allein im örtlichen
//! Protokoll (`tracing`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use quiver_domain::{EintragId, Laufzeit};

use crate::protokoll::Protokoll;

/// Der Schalter für die Entwicklung: ein Ordner mit den Programmen darin,
/// flach, ohne Prüfsummen-Ordner.
pub const ORDNER_SCHALTER: &str = "QUIVER_TOOLS_DIR";

/// Woher ein Programm kommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Herkunft {
    /// Aus `QUIVER_TOOLS_DIR` (Entwicklung).
    Entwicklung,
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
/// auf der Tool-Karte im Fenster.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NichtGefunden {
    #[error("»{0}« wurde auf diesem Gerät nicht gefunden")]
    Nirgends(String),
    #[error("für diesen Eintrag ist kein Programm eingetragen")]
    OhneNamen,
}

/// Wo gesucht wird. Aus der Umgebung gebaut ([`Suche::aus_umgebung`]) oder im
/// Test von Hand gesetzt.
#[derive(Debug, Clone)]
pub struct Suche {
    /// `QUIVER_TOOLS_DIR`, wenn gesetzt (Entwicklung).
    pub eigener: Option<PathBuf>,
    /// Die Einträge von `PATH`.
    pub pfad: Vec<PathBuf>,
}

impl Suche {
    /// Die Suche, wie sie im Betrieb gilt – gemerkt, solange sich die Umgebung
    /// nicht ändert.
    ///
    /// Sie hängt nur an der Umgebung, die sich im Betrieb nicht mehr ändert;
    /// deshalb wird sie gemerkt, statt bei jedem Eintrag und jedem Bild des
    /// Fensters neu aufgebaut zu werden. Ändert sich `QUIVER_TOOLS_DIR` doch
    /// (Tests, Werkzeuge aus einem Checkout), wird sie neu gebaut — sonst
    /// liefe ein Test mit dem Ordner eines anderen.
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
            eigener: umgebungsordner(ORDNER_SCHALTER),
            pfad: std::env::var_os("PATH")
                .map(|pfad| std::env::split_paths(&pfad).collect())
                .unwrap_or_default(),
        }
    }

    /// Ein Programm finden.
    ///
    /// Ein Name ohne Trennzeichen wird zuerst im Entwicklungsordner gesucht,
    /// dann im `PATH`; ein Pfad gilt, wie er dasteht. Gibt es den Pfad nicht
    /// mehr, zählt sein Dateiname (Altbestand): der Eintrag heilt sich, sobald
    /// das Programm als Name gefunden wird.
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
                });
            }
            let name = dateiname(programm);
            return self.nach_namen(&name).ok_or(NichtGefunden::Nirgends(name));
        }
        self.nach_namen(programm)
            .ok_or_else(|| NichtGefunden::Nirgends(programm.to_string()))
    }

    /// Einen bloßen Namen suchen: erst der Entwicklungsordner, dann `PATH`.
    fn nach_namen(&self, name: &str) -> Option<Fund> {
        if let Some(ordner) = &self.eigener {
            if let Some(pfad) = im_ordner(ordner, name) {
                return Some(Fund {
                    pfad,
                    herkunft: Herkunft::Entwicklung,
                });
            }
        }
        self.im_pfad(name)
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
                    });
                }
            }
        }
        None
    }
}

/// Ist das ein Programm, das der Server verteilt (Aufgabe 37)?
///
/// `Some(name)` heißt: der Eintrag erwartet ein Programm vom Server, und zwar
/// unter diesem Namen – auch dann, wenn er noch als alter Pfad dasteht
/// (Altbestand). `None` heißt: der Eintrag startet ein fremdes Programm
/// (`npx`, `uvx`, `docker`, ein Pfad zu einem eigenen Programm).
pub fn verteilt_name(laufzeit: &Laufzeit) -> Option<String> {
    let Laufzeit::Stdio { befehl, .. } = laufzeit else {
        return None;
    };
    let befehl = befehl.trim();
    if quiver_domain::ist_verteilt(befehl) {
        return Some(befehl.to_string());
    }
    if ist_pfad(befehl) && !Path::new(befehl).is_file() {
        let name = dateiname(befehl);
        if quiver_domain::ist_verteilt(&name) {
            return Some(name);
        }
    }
    None
}

/// Ein Programm für einen Eintrag suchen.
///
/// Die eine Stelle, durch die Start, Anmeldung, Status, Einrichtung und
/// Testen gehen. Verteilt der Server das Programm, löst der Aufrufer es vorher
/// über [`crate::werkzeuge`] auf; hier landen nur fremde Programme und feste
/// Pfade.
pub(crate) fn fuer_eintrag(
    id: &EintragId,
    programm: &str,
    protokoll: &Protokoll,
) -> anyhow::Result<Fund> {
    let suche = Suche::gemerkt();
    let fund = match suche.finden(programm) {
        Ok(fund) => fund,
        Err(fehler) => {
            // Der Benutzer kann nichts tun, der Admin muss das Programm auf
            // dem Server bereitstellen: der Hinweis gehört ins Protokoll,
            // nicht auf die Karte.
            tracing::warn!(eintrag = %id, "Programm nicht gefunden: {fehler}");
            protokoll.zeile_schreiben(
                id,
                &format!(
                    "{fehler}. Bitte den Admin bitten, das Programm auf dem Server \
                     bereitzustellen oder in den PATH zu legen."
                ),
            );
            return Err(fehler.into());
        }
    };
    tracing::debug!(eintrag = %id, programm = %fund.pfad.display(), "Programm gefunden");
    Ok(fund)
}

/// Fehlt das Programm dieses Eintrags auf diesem Gerät?
///
/// `Some(name)`, wenn der Eintrag ein Programm startet, das **nicht** vom
/// Server kommt und weder im Entwicklungsordner noch im `PATH` liegt. Die
/// Karte schreibt dann „Dieses Tool ist auf diesem Gerät nicht installiert.“
/// und bietet keine Anmeldung an. Verteilt der Server das Programm, ist das
/// hier nie der Fall – darum kümmert sich der Abgleich.
pub(crate) fn fehlt(laufzeit: &Laufzeit) -> Option<String> {
    if verteilt_name(laufzeit).is_some() {
        return None;
    }
    let Laufzeit::Stdio { befehl, .. } = laufzeit else {
        return None;
    };
    match Suche::gemerkt().finden(befehl) {
        Ok(_) => None,
        Err(NichtGefunden::OhneNamen) => None,
        Err(NichtGefunden::Nirgends(name)) => Some(name),
    }
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

/// Ein Programm in einem Ordner suchen — ohne Ausbruch.
///
/// Ein Name mit Trennzeichen wird nie angehängt, und ein Verweis, der aus dem
/// Ordner herauszeigt, gilt nicht: der Ordner ist die Grenze.
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
pub(crate) fn dateiname(programm: &str) -> String {
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

/// Liegt ein Programm dieses Namens im Entwicklungsordner (`QUIVER_TOOLS_DIR`)?
///
/// Nur für die Entwicklung: ein ausgecheckter Stand lässt sich so ohne Server
/// ausprobieren. Im Betrieb ist der Schalter leer.
pub fn im_entwicklungsordner(name: &str) -> Option<PathBuf> {
    let ordner = Suche::gemerkt().eigener?;
    im_ordner(&ordner, name)
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

    /// Ein eigener Ordner je Test unter `temp` — kein Aufräumen nötig, und
    /// kein Test sieht die Bäume eines anderen.
    fn baum(name: &str) -> PathBuf {
        let ordner = std::env::temp_dir().join(format!(
            "quiver-tools-{}-{name}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|dauer| dauer.as_nanos())
                .unwrap_or(0)
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

    #[test]
    fn ein_blosser_name_kommt_aus_dem_path() {
        let baum = baum("pfad");
        let im_pfad = baum.join("pfad");
        werkzeug(&im_pfad.join("quiver-devops"));
        werkzeug(&im_pfad.join("quiver-jira.exe"));

        let gesucht = Suche {
            eigener: None,
            pfad: vec![im_pfad.clone()],
        };
        let fund = gesucht.finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Pfad);
        assert_eq!(fund.pfad, im_pfad.join("quiver-devops"));
        assert_eq!(fund.name(), "quiver-devops");

        // Auch als .exe gefunden und ohne Endung benannt.
        let fund = gesucht.finden("quiver-jira").expect("gefunden");
        assert_eq!(fund.pfad, im_pfad.join("quiver-jira.exe"));
        assert_eq!(fund.name(), "quiver-jira");

        // Fremde Programme ebenso.
        werkzeug(&im_pfad.join("npx"));
        assert_eq!(
            gesucht.finden("npx").expect("gefunden").herkunft,
            Herkunft::Pfad
        );

        assert_eq!(
            gesucht.finden("quiver-gibtsnicht"),
            Err(NichtGefunden::Nirgends("quiver-gibtsnicht".into()))
        );
    }

    #[test]
    fn der_entwicklungsordner_geht_vor_dem_path() {
        let baum = baum("entwicklung");
        let eigen = baum.join("aus-checkout");
        werkzeug(&eigen.join("quiver-devops"));
        let im_pfad = baum.join("pfad");
        werkzeug(&im_pfad.join("quiver-devops"));

        let gesucht = Suche {
            eigener: Some(eigen.clone()),
            pfad: vec![im_pfad],
        };
        let fund = gesucht.finden("quiver-devops").expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Entwicklung);
        assert_eq!(fund.pfad, eigen.join("quiver-devops"));
    }

    #[test]
    fn ein_fester_pfad_gilt_wie_er_dasteht() {
        let baum = baum("fest");
        let anderswo = baum.join("opt/werkzeuge/mein-programm");
        werkzeug(&anderswo);

        let gesucht = Suche {
            eigener: None,
            pfad: Vec::new(),
        };
        let fund = gesucht
            .finden(&anderswo.to_string_lossy())
            .expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Fest);
        assert_eq!(fund.pfad, anderswo);
    }

    #[test]
    fn ein_alter_pfad_heilt_sich_ueber_den_namen() {
        // Der Server kennt den Eintrag noch mit dem Pfad einer
        // Entwicklermaschine; derselbe Name liegt im PATH.
        let baum = baum("altbestand");
        let im_pfad = baum.join("pfad");
        werkzeug(&im_pfad.join("quiver-devops"));
        let alt = "/home/entwickler/quiver-tools/target/release/quiver-devops";

        let gesucht = Suche {
            eigener: None,
            pfad: vec![im_pfad.clone()],
        };
        let fund = gesucht.finden(alt).expect("gefunden");
        assert_eq!(fund.herkunft, Herkunft::Pfad);
        assert_eq!(fund.pfad, im_pfad.join("quiver-devops"));

        // Und der Name ist als verteilt erkennbar, obwohl ein Pfad dasteht.
        let laufzeit = Laufzeit::Stdio {
            befehl: alt.into(),
            argumente: Vec::new(),
            umgebung: quiver_domain::Werte::new(),
        };
        assert_eq!(verteilt_name(&laufzeit).as_deref(), Some("quiver-devops"));
    }

    #[test]
    fn ein_alter_pfad_ohne_namen_bleibt_ein_fehler() {
        let baum = baum("altbestand-fehler");
        let im_pfad = baum.join("pfad");
        werkzeug(&im_pfad.join("quiver-devops"));

        let fehler = Suche {
            eigener: None,
            pfad: vec![im_pfad],
        }
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
        let eigen = baum.join("eigen");
        werkzeug(&eigen.join("quiver-devops"));
        // Ein Verweis nach draußen: er gilt nicht.
        let draussen = baum.join("draussen/quiver-boese");
        werkzeug(&draussen);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&draussen, eigen.join("quiver-boese")).expect("Verweis");

        let gesucht = Suche {
            eigener: Some(eigen),
            pfad: Vec::new(),
        };
        // Kein Ausbruch: weder mit `..` noch nach draußen.
        assert!(gesucht.finden("../../etc/passwd").is_err());
        #[cfg(unix)]
        assert!(
            gesucht.finden("quiver-boese").is_err(),
            "ein Verweis aus dem Ordner heraus gilt nicht"
        );
        assert!(gesucht.finden("").is_err());
    }

    /// Nur eigene Programme kommen vom Server. Fremde Läufer und Pfade bleiben,
    /// wie sie sind (Aufgabe 37).
    #[test]
    fn nur_eigene_programme_kommen_vom_server() {
        let stdio = |befehl: &str| Laufzeit::Stdio {
            befehl: befehl.into(),
            argumente: Vec::new(),
            umgebung: quiver_domain::Werte::new(),
        };
        assert_eq!(
            verteilt_name(&stdio("quiver-devops")).as_deref(),
            Some("quiver-devops")
        );
        for fremd in ["npx", "uvx", "docker", "/opt/eigenes-programm"] {
            assert_eq!(verteilt_name(&stdio(fremd)), None, "{fremd}");
        }
        assert_eq!(
            verteilt_name(&Laufzeit::Remote {
                url: "https://example.test/mcp".into(),
                kopfzeilen: quiver_domain::Werte::new(),
            }),
            None
        );
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
