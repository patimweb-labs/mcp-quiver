// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Programme, die der Server an die Geräte verteilt (Aufgabe 37).
//!
//! Der Server führt keine Werkzeuge aus; er hält ihre Programme bereit. Zwei
//! Quellen gibt es:
//!
//! 1. das **Abbild**: die Archive des festgelegten Releases von quiver-tools
//!    (Aufgabe 38), beim Bauen entpackt und gegen `SHA256SUMS` geprüft, unter
//!    `/opt/quiver/tools/<system>/<arch>/`;
//! 2. der **Betreiber**: `<daten>/tools/<system>/` (nur root am Server, nicht
//!    über die Weboberfläche). Was dort liegt, geht vor.
//!
//! Gesucht wird in genau dieser Reihenfolge, und innerhalb einer Quelle geht
//! die genauere Angabe vor: `<system>/<arch>/` vor `<system>/`.
//!
//! Eine `LISTE.json` neben den Programmen nennt Fassung, Größe und Prüfsumme
//! (die Form, die das Release mitbringt):
//!
//! ```json
//! {"version":"1.2.3","programme":[
//!   {"name":"quiver-devops","version":"1.2.3","groesse":1234,"sha256":"…"}
//! ]}
//! ```
//!
//! Fehlt sie (ein Programm, das der Betreiber selbst abgelegt hat), wird die
//! Fassung `unbekannt`, und Größe und Prüfsumme werden aus der Datei gerechnet.
//! Gerechnet wird immer: was in der Liste steht, ist eine Behauptung, die Datei
//! ist die Wahrheit.
//!
//! Kein Name und kein Pfad aus dem Netz wird je zu einem Pfad: gesucht wird
//! nur unter den Namen, die [`quiver_domain::ist_verteilt`] durchlässt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use quiver_domain::{ist_verteilt, Programm, System};
use sha2::{Digest, Sha256};

/// So groß darf ein Programm höchstens sein (Grenze beim Ausliefern).
pub const PROGRAMM_BYTES_MAX: u64 = 128 * 1024 * 1024;

/// Die Fassung, die ein Programm ohne `LISTE.json` bekommt.
pub const OHNE_FASSUNG: &str = "unbekannt";

/// Ein gefundenes Programm samt seiner Datei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fund {
    pub programm: Programm,
    pub datei: PathBuf,
}

/// Was eine `LISTE.json` je Programm sagt.
#[derive(Debug, Clone, serde::Deserialize)]
struct ListenEintrag {
    name: String,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct Liste {
    #[serde(default)]
    version: Option<String>,
    #[serde(default, alias = "tools", alias = "programme")]
    programme: Vec<ListenEintrag>,
}

/// Der gemerkte Stand einer Datei: ändert sie sich, wird neu gerechnet.
#[derive(Debug, Clone)]
struct Gerechnet {
    groesse: u64,
    geaendert: Option<SystemTime>,
    sha256: String,
}

impl Gerechnet {
    fn passt(&self, groesse: u64, geaendert: Option<SystemTime>) -> bool {
        self.groesse == groesse && self.geaendert == geaendert
    }
}

/// Die Ablage der Programme.
#[derive(Debug)]
pub struct Ablage {
    /// Die Wurzelordner in der Reihenfolge, in der sie gelten.
    wurzeln: Vec<PathBuf>,
    /// Prüfsummen je Datei, damit nicht bei jeder Anfrage alles neu gerechnet
    /// wird. Der Schlüssel ist der Pfad; was verschwindet, fällt heraus.
    gerechnet: Mutex<BTreeMap<PathBuf, Gerechnet>>,
}

impl Ablage {
    /// Eine Ablage über den genannten Ordnern (die erste gilt zuerst).
    pub fn neu(wurzeln: Vec<PathBuf>) -> Self {
        Self {
            wurzeln,
            gerechnet: Mutex::new(BTreeMap::new()),
        }
    }

    /// Alle Programme, die es für dieses System gibt – je Name das, das gelten
    /// würde.
    pub fn fuer(&self, system: &System) -> Vec<Programm> {
        self.finden_alle(system, None)
            .into_iter()
            .map(|fund| fund.programm)
            .collect()
    }

    /// Alle Programme aller Systeme (für die Verwaltung).
    pub fn alle(&self) -> Vec<Programm> {
        let mut alle = Vec::new();
        for system in System::SYSTEME {
            for arch in System::ARCHITEKTUREN {
                let system = System {
                    system: system.to_string(),
                    arch: arch.to_string(),
                };
                alle.extend(self.fuer(&system));
            }
        }
        alle.sort_by(|a, b| (&a.system, &a.arch, &a.name).cmp(&(&b.system, &b.arch, &b.name)));
        alle
    }

    /// Ein Programm für dieses System und diesen Namen ([`ist_verteilt`]).
    pub fn finden(&self, system: &System, name: &str) -> Option<Fund> {
        if !ist_verteilt(name) {
            return None;
        }
        self.finden_alle(system, Some(name)).into_iter().next()
    }

    /// Wie viele Programme je System liegen hier? (für den Start ins Log)
    pub fn stand(&self) -> String {
        let mut teile = Vec::new();
        for system in System::SYSTEME {
            for arch in System::ARCHITEKTUREN {
                let system = System {
                    system: system.to_string(),
                    arch: arch.to_string(),
                };
                let anzahl = self.fuer(&system).len();
                if anzahl > 0 {
                    teile.push(format!("{}/{}: {anzahl}", system.system, system.arch));
                }
            }
        }
        if teile.is_empty() {
            format!(
                "keine Programme (gesucht in {})",
                self.wurzeln
                    .iter()
                    .map(|pfad| pfad.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            teile.join(", ")
        }
    }

    /// Alle Funde für dieses System; `name` grenzt auf einen Namen ein.
    fn finden_alle(&self, system: &System, name: Option<&str>) -> Vec<Fund> {
        let mut treffer: BTreeMap<String, (usize, usize, Fund)> = BTreeMap::new();
        for (rang, wurzel) in self.wurzeln.iter().enumerate() {
            // Erst die genauere Angabe (mit Architektur), dann die allgemeine.
            for (genau, arch) in [(1usize, Some(system.arch.as_str())), (0, None)] {
                let ordner = match arch {
                    Some(arch) => wurzel.join(&system.system).join(arch),
                    None => wurzel.join(&system.system),
                };
                for programm in self.ordner_lesen(&ordner, &system.system, arch.unwrap_or("")) {
                    if name.is_some_and(|name| programm.programm.name != name) {
                        continue;
                    }
                    // Eine frühere Quelle (der Betreiber) und, innerhalb einer
                    // Quelle, die genauere Angabe gewinnen.
                    let besser = treffer.get(&programm.programm.name).is_none_or(
                        |(vor_rang, vor_genau, _)| {
                            rang < *vor_rang || (rang == *vor_rang && genau > *vor_genau)
                        },
                    );
                    if besser {
                        treffer.insert(programm.programm.name.clone(), (rang, genau, programm));
                    }
                }
            }
        }
        treffer.into_values().map(|(_, _, fund)| fund).collect()
    }

    /// Die Programme eines Ordners lesen.
    ///
    /// `arch` ist leer, wenn der Ordner nur nach System geht; dann gilt das
    /// Programm für jede Architektur dieses Systems.
    fn ordner_lesen(&self, ordner: &Path, system: &str, arch: &str) -> Vec<Fund> {
        let Ok(eintraege) = std::fs::read_dir(ordner) else {
            return Vec::new();
        };
        let liste = liste_lesen(ordner);
        let mut funde = Vec::new();
        for eintrag in eintraege.flatten() {
            let pfad = eintrag.path();
            let Ok(art) = eintrag.file_type() else {
                continue;
            };
            // Keine Verweise: was aus dem Ordner herauszeigt, gilt nicht.
            if !art.is_file() {
                continue;
            }
            let Some(dateiname) = pfad.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let name = ohne_endung(dateiname);
            if !ist_verteilt(&name) {
                continue;
            }
            let Ok(metadaten) = eintrag.metadata() else {
                continue;
            };
            if metadaten.len() > PROGRAMM_BYTES_MAX {
                tracing::warn!(
                    "{} ist zu groß für ein Programm ({} Bytes), wird nicht verteilt",
                    pfad.display(),
                    metadaten.len()
                );
                continue;
            }
            let geaendert = metadaten.modified().ok();
            let Some(sha256) = self.pruefsumme(&pfad, metadaten.len(), geaendert) else {
                continue;
            };
            // Die Fassung des Programms, sonst die des ganzen Ordners, sonst
            // „unbekannt“.
            let version = liste
                .programme
                .iter()
                .find(|eintrag| eintrag.name == name)
                .and_then(|eintrag| eintrag.version.clone())
                .or_else(|| liste.version.clone())
                .unwrap_or_else(|| OHNE_FASSUNG.to_string());
            funde.push(Fund {
                programm: Programm {
                    name,
                    version,
                    groesse: metadaten.len(),
                    sha256,
                    system: system.to_string(),
                    arch: arch.to_string(),
                },
                datei: pfad,
            });
        }
        funde
    }

    /// Die Prüfsumme einer Datei – gemerkt, solange Größe und Zeit gleich sind.
    fn pruefsumme(
        &self,
        pfad: &Path,
        groesse: u64,
        geaendert: Option<SystemTime>,
    ) -> Option<String> {
        {
            let karte = crate::sperre(&self.gerechnet);
            if let Some(gemerkten) = karte.get(pfad) {
                if gemerkten.passt(groesse, geaendert) {
                    return Some(gemerkten.sha256.clone());
                }
            }
        }
        let mut datei = std::fs::File::open(pfad).ok()?;
        let mut h = Sha256::new();
        let mut puffer = vec![0u8; 64 * 1024];
        loop {
            use std::io::Read;
            let gelesen = datei.read(&mut puffer).ok()?;
            if gelesen == 0 {
                break;
            }
            h.update(&puffer[..gelesen]);
        }
        let sha256 = hex::encode(h.finalize());
        crate::sperre(&self.gerechnet).insert(
            pfad.to_path_buf(),
            Gerechnet {
                groesse,
                geaendert,
                sha256: sha256.clone(),
            },
        );
        Some(sha256)
    }
}

/// Die `LISTE.json` eines Ordners, falls es sie gibt.
fn liste_lesen(ordner: &Path) -> Liste {
    let Ok(bytes) = std::fs::read(ordner.join("LISTE.json")) else {
        return Liste::default();
    };
    match serde_json::from_slice::<Liste>(&bytes) {
        Ok(liste) => liste,
        Err(fehler) => {
            tracing::warn!(
                "LISTE.json in {} ist nicht lesbar: {fehler}",
                ordner.display()
            );
            Liste::default()
        }
    }
}

/// Der Programmname ohne `.exe` (Windows) – so, wie ihn ein Eintrag nennt.
fn ohne_endung(dateiname: &str) -> String {
    let klein = dateiname.to_ascii_lowercase();
    if klein.ends_with(".exe") {
        dateiname[..dateiname.len() - 4].to_string()
    } else {
        dateiname.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baum(name: &str) -> PathBuf {
        let ordner = std::env::temp_dir().join(format!(
            "quiver-programme-{}-{name}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|dauer| dauer.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::remove_dir_all(&ordner).ok();
        std::fs::create_dir_all(&ordner).expect("Testordner");
        ordner
    }

    fn programm(pfad: &Path, inhalt: &[u8]) {
        std::fs::create_dir_all(pfad.parent().expect("Ordner")).expect("Ordner");
        std::fs::write(pfad, inhalt).expect("Programm");
    }

    fn system(system: &str, arch: &str) -> System {
        System {
            system: system.to_string(),
            arch: arch.to_string(),
        }
    }

    #[test]
    fn ein_programm_wird_mit_groesse_und_pruefsumme_gefunden() {
        let baum = baum("einfach");
        programm(&baum.join("linux/x64/quiver-devops"), b"hallo");
        let ablage = Ablage::neu(vec![baum]);

        let fund = ablage
            .finden(&system("linux", "x64"), "quiver-devops")
            .expect("gefunden");
        assert_eq!(fund.programm.groesse, 5);
        assert_eq!(fund.programm.sha256, hex::encode(Sha256::digest(b"hallo")));
        assert_eq!(fund.programm.version, OHNE_FASSUNG);
        assert!(fund.datei.ends_with("quiver-devops"));

        // Ein anderes System hat es nicht.
        assert!(ablage
            .finden(&system("macos", "arm64"), "quiver-devops")
            .is_none());
        // Fremde Namen werden nicht verteilt.
        assert!(ablage.finden(&system("linux", "x64"), "npx").is_none());
        assert!(ablage
            .finden(&system("linux", "x64"), "../../etc/passwd")
            .is_none());
    }

    #[test]
    fn die_genauere_angabe_geht_vor_und_die_liste_nennt_die_fassung() {
        let baum = baum("genau");
        programm(&baum.join("linux/quiver-devops"), b"allgemein");
        programm(&baum.join("linux/x64/quiver-devops"), b"genau");
        std::fs::write(
            baum.join("linux/x64/LISTE.json"),
            br#"{"version":"1.2.3","programme":[
                {"name":"quiver-devops","version":"1.2.3","groesse":5,"sha256":"egal"}
            ]}"#,
        )
        .expect("Liste");

        let ablage = Ablage::neu(vec![baum]);
        let fund = ablage
            .finden(&system("linux", "x64"), "quiver-devops")
            .expect("gefunden");
        assert_eq!(fund.programm.sha256, hex::encode(Sha256::digest(b"genau")));
        assert_eq!(fund.programm.version, "1.2.3");

        // Ohne genauere Angabe gilt die allgemeine.
        let fund = ablage
            .finden(&system("linux", "arm64"), "quiver-devops")
            .expect("gefunden");
        assert_eq!(
            fund.programm.sha256,
            hex::encode(Sha256::digest(b"allgemein"))
        );
        assert_eq!(fund.programm.version, OHNE_FASSUNG);
    }

    #[test]
    fn der_betreiber_geht_vor_dem_abbild() {
        let betreiber = baum("betreiber");
        let abbild = baum("abbild");
        programm(&betreiber.join("linux/quiver-devops"), b"betreiber");
        programm(&abbild.join("linux/x64/quiver-devops"), b"abbild");
        let ablage = Ablage::neu(vec![betreiber, abbild]);
        let fund = ablage
            .finden(&system("linux", "x64"), "quiver-devops")
            .expect("gefunden");
        assert_eq!(
            fund.programm.sha256,
            hex::encode(Sha256::digest(b"betreiber"))
        );
    }

    #[test]
    fn eine_geaenderte_datei_bekommt_eine_neue_pruefsumme() {
        let baum = baum("aenderung");
        let pfad = baum.join("linux/x64/quiver-devops");
        programm(&pfad, b"alt");
        let ablage = Ablage::neu(vec![baum]);
        let alt = ablage
            .finden(&system("linux", "x64"), "quiver-devops")
            .expect("gefunden")
            .programm
            .sha256;

        // Gleiche Länge, andere Zeit: die Prüfsumme wird neu gerechnet.
        std::thread::sleep(std::time::Duration::from_millis(10));
        programm(&pfad, b"neu");
        let neu = ablage
            .finden(&system("linux", "x64"), "quiver-devops")
            .expect("gefunden")
            .programm
            .sha256;
        assert_ne!(alt, neu);
    }

    #[test]
    fn verweise_und_fremde_namen_gelten_nicht() {
        let baum = baum("verweis");
        let draussen = baum.join("draussen/quiver-boese");
        programm(&draussen, b"boese");
        std::fs::create_dir_all(baum.join("linux/x64")).expect("Ordner");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&draussen, baum.join("linux/x64/quiver-boese"))
            .expect("Verweis");
        programm(&baum.join("linux/x64/npx"), b"fremd");

        let ablage = Ablage::neu(vec![baum]);
        #[cfg(unix)]
        assert!(ablage
            .finden(&system("linux", "x64"), "quiver-boese")
            .is_none());
        assert!(ablage.finden(&system("linux", "x64"), "npx").is_none());
    }

    #[test]
    fn windows_programme_tragen_exe_und_heissen_ohne() {
        let baum = baum("windows");
        programm(&baum.join("windows/x64/quiver-devops.exe"), b"exe");
        let ablage = Ablage::neu(vec![baum]);
        let fund = ablage
            .finden(&system("windows", "x64"), "quiver-devops")
            .expect("gefunden");
        assert_eq!(fund.programm.name, "quiver-devops");
        assert!(fund.datei.ends_with("quiver-devops.exe"));
    }

    #[test]
    fn ohne_programme_sagt_der_stand_wo_gesucht_wurde() {
        let baum = baum("leer");
        let ablage = Ablage::neu(vec![baum.clone()]);
        let stand = ablage.stand();
        assert!(stand.contains("keine Programme"), "{stand}");
        assert!(stand.contains(&baum.display().to_string()), "{stand}");
        assert!(ablage.alle().is_empty());
    }
}
