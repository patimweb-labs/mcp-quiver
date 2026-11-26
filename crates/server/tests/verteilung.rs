// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Paketskripte als Test: sie laufen in der CI, nicht hier.
//!
//! Seit Aufgabe 37 bringt der Client **keine** Werkzeuge mehr mit: der Server
//! verteilt die Programme. Was beim Ausbau stehen blieb, fällt aber erst auf,
//! wenn ein Paket bricht (ein `cp` auf einen Ordner, den es nicht gibt) oder
//! wenn `Set-StrictMode` über eine Variable stolpert, die niemand setzt.
//!
//! Die drei Prüfungen hier sind bewusst klein und lesen die Dateien, statt sie
//! auszuführen: `cargo test` soll auch ohne `wix`, `nfpm` und `hdiutil` laufen.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Die Wurzel des Repos (zwei Ebenen über dem Testordner).
fn wurzel() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn lies(pfad: &str) -> String {
    let pfad = wurzel().join(pfad);
    std::fs::read_to_string(&pfad).unwrap_or_else(|fehler| panic!("{}: {fehler}", pfad.display()))
}

/// Die Dateien der Verteilung: Skripte, Vorlagen und der Release-Lauf.
const VERTEILUNG: [&str; 6] = [
    "verteilung/linux/paket.sh",
    "verteilung/linux/nfpm.yaml.in",
    "verteilung/macos/paket.sh",
    "verteilung/windows/paket.ps1",
    "verteilung/windows/quiver.wxs",
    ".github/workflows/release.yml",
];

/// Vom Einbau der Werkzeuge in das Paket (Aufgabe 35) darf nichts übrig sein:
/// kein Ordner `tools`, keine Liste, keine Größentabelle mehr.
///
/// Gesucht wird nach **Pfaden** in den Paketbau, nicht nach dem Wort: „MCP
/// tools“ und `dist/werkzeuge/` (nfpm, appimagetool) sind in Ordnung.
#[test]
fn keine_reste_vom_einbau_der_tools() {
    let reste = [
        "TOOLS.txt",
        "$baum/tools",
        "$baum/lib",
        "__BAUM__/tools",
        "Resources/tools",
        "Contents/MacOS/tools",
        "release\\tools",
        "release/tools",
        "dist/tools",
    ];
    let mut treffer = Vec::new();
    for datei in VERTEILUNG {
        let text = lies(datei);
        for rest in reste {
            if text.contains(rest) {
                treffer.push(format!("{datei}: {rest}"));
            }
        }
    }
    assert!(
        treffer.is_empty(),
        "Reste vom Einbau der Werkzeuge: {treffer:#?}"
    );
}

/// Was das Linux-Skript für das AppImage aus dem Baum kopiert, muss der Baum
/// auch anlegen. Genau hier brach der Lauf: `cp` auf ein `lib`, das es seit
/// Aufgabe 37 nicht mehr gibt.
#[test]
fn das_appimage_kopiert_nur_was_der_baum_hat() {
    let text = zusammenfassen(&lies("verteilung/linux/paket.sh"));
    let mut kopiert = BTreeSet::new();
    for zeile in text.lines() {
        let Some(rest) = zeile.trim().strip_prefix("cp -R ") else {
            continue;
        };
        // `"$baum/bin" "$baum/share" "$appdir/usr/"` → bin, share
        for teil in rest.split_whitespace() {
            let name = teil
                .trim_matches('"')
                .strip_prefix("$baum/")
                .map(|name| name.trim_end_matches('/').to_string());
            if let Some(name) = name.filter(|name| !name.is_empty()) {
                kopiert.insert(name);
            }
        }
    }
    assert!(
        !kopiert.is_empty(),
        "kein `cp -R \"$baum/…\"` gefunden – hat sich das Skript geändert?"
    );
    for name in &kopiert {
        // Angelegt wird mit `install` (Datei samt Ordnern) oder `mkdir -p`;
        // die `cp`-Zeile selbst zählt nicht.
        let angelegt = text.lines().any(|zeile| {
            let zeile = zeile.trim();
            (zeile.starts_with("install ") || zeile.starts_with("mkdir "))
                && (zeile.contains(&format!("\"$baum/{name}/"))
                    || zeile.contains(&format!("\"$baum/{name}\"")))
        });
        assert!(
            angelegt,
            "das Skript kopiert \"$baum/{name}\", legt es aber nirgends an"
        );
    }
}

/// Fortsetzungszeilen (Ende `\`) zu einer Zeile machen: ein `install`, dessen
/// Ziel in der nächsten Zeile steht, ist eine Anweisung.
fn zusammenfassen(text: &str) -> String {
    let mut aus = String::with_capacity(text.len());
    let mut haengt = false;
    for zeile in text.lines() {
        let inhalt = zeile.trim_end();
        let weiter = inhalt.ends_with('\\');
        let inhalt = inhalt.trim_end_matches('\\').trim_end();
        if haengt {
            aus.push(' ');
        }
        aus.push_str(inhalt);
        if !weiter {
            aus.push('\n');
        }
        haengt = weiter;
    }
    aus
}

/// Jede Variable im Windows-Skript wird auch gesetzt. `Set-StrictMode
/// -Version Latest` macht aus einer vergessenen Zuweisung einen Abbruch –
/// unter Windows, und nur dort.
#[test]
fn jede_variable_im_windows_skript_wird_gesetzt() {
    let text = lies("verteilung/windows/paket.ps1");
    // Was PowerShell selbst füllt oder was keine Variable aus dem Skript ist.
    let automatisch: BTreeSet<&str> = [
        "_",
        "true",
        "false",
        "null",
        "LASTEXITCODE",
        "PSScriptRoot",
        "ErrorActionPreference",
    ]
    .into_iter()
    .collect();

    // `param(...)`-Angaben (auch mehrzeilig) sind Parameter, keine Zuweisungen
    // im Rumpf; sie werden vor dem Suchen herausgenommen.
    let mut gesetzt = BTreeSet::new();
    let mut rumpf = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(pos) = rest.find("param(") {
        rumpf.push_str(&rest[..pos]);
        let nach = &rest[pos + "param(".len()..];
        let ende = nach.find(')').map(|ende| pos + "param(".len() + ende + 1);
        let block = match ende {
            Some(ende) => &rest[pos..ende],
            None => &rest[pos..],
        };
        for (_, name) in variablen(block) {
            gesetzt.insert(name);
        }
        rest = ende.map(|ende| &rest[ende..]).unwrap_or("");
    }
    rumpf.push_str(rest);

    let mut benutzt = BTreeSet::new();
    for (ist_zuweisung, name) in variablen(&rumpf) {
        if automatisch.contains(name.as_str()) {
            continue;
        }
        if ist_zuweisung {
            gesetzt.insert(name);
        } else {
            benutzt.insert(name);
        }
    }

    let fehlend: Vec<&String> = benutzt.difference(&gesetzt).collect();
    assert!(
        fehlend.is_empty(),
        "im Windows-Skript benutzt, aber nie gesetzt: {fehlend:?}"
    );
}

/// Alle `$variablen` eines Textes mit der Angabe, ob eine Zuweisung folgt.
fn variablen(text: &str) -> Vec<(bool, String)> {
    let mut aus = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find('$') {
        rest = &rest[pos + 1..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        rest = &rest[name.len()..];
        // `$env:PATH`, `$global:x`: keine Variable des Skripts.
        if rest.starts_with(':') {
            continue;
        }
        let danach = rest.trim_start();
        // `$x = …` setzt, `foreach ($x in …)` ebenfalls – sonst ist es eine
        // Benutzung.
        let zuweisung =
            (danach.starts_with('=') && !danach.starts_with("==")) || danach.starts_with("in ");
        aus.push((zuweisung, name));
    }
    aus
}

// ------------------------------------------------------------------ Programme

/// Die Stufe „programme“ des Abbilds (`verteilung/programme.sh`) im
/// Temp-Ordner: mit selbst gebauten Archiven, so wie das Release von
/// quiver-tools sie veröffentlicht.
///
/// Geprüft wird, was der Lauf aus GitHub gezeigt hat: die Archive heißen
/// `…-<system>-<arch>.tar.gz` mit `x86_64`/`aarch64`, die Ordner im Abbild aber
/// `<system>/x64|arm64`; und geprüft wird **vor** dem Entpacken.
#[cfg(unix)]
mod programme {
    use super::wurzel;
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Fassung des nachgebauten Releases.
    const FASSUNG: &str = "9.9.9";

    /// Die Archive, die das Release baut (System und Archiv-Architektur) und
    /// der Ordner, den das Abbild dafür erwartet.
    const ARCHIVE: [(&str, &str, &str, &str); 5] = [
        ("windows", "x86_64", "windows", "x64"),
        ("macos", "aarch64", "macos", "arm64"),
        ("macos", "x86_64", "macos", "x64"),
        ("linux", "x86_64", "linux", "x64"),
        ("linux", "aarch64", "linux", "arm64"),
    ];

    /// Eine weggeräumte Welt je Test.
    struct Welt {
        ordner: PathBuf,
    }

    impl Drop for Welt {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.ordner);
        }
    }

    impl Welt {
        fn neu(name: &str) -> Self {
            static ZAEHLER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let ordner = std::env::temp_dir().join(format!(
                "quiver-programme-test-{name}-{}-{nummer}",
                std::process::id()
            ));
            std::fs::create_dir_all(&ordner).expect("Testordner");
            Self { ordner }
        }

        fn release(&self) -> PathBuf {
            self.ordner.join("release")
        }

        fn ziel(&self) -> PathBuf {
            self.ordner.join("programme")
        }

        /// Der Bauordner eines Archivs (Programm und `LISTE.json`).
        fn bau(&self, system: &str, arch: &str) -> PathBuf {
            self.ordner.join(format!("bau-{system}-{arch}"))
        }

        /// Ein Archiv bauen: ein Programm und eine `LISTE.json`, flach, wie das
        /// Release sie schreibt.
        fn archiv(&self, system: &str, arch: &str, inhalt: &[u8]) -> PathBuf {
            let name = if system == "windows" {
                "quiver-devops.exe"
            } else {
                "quiver-devops"
            };
            let bau = self.bau(system, arch);
            std::fs::create_dir_all(&bau).expect("Bauordner");
            std::fs::write(bau.join(name), inhalt).expect("Programm");
            let liste = format!(
                "{{\n  \"version\": \"{FASSUNG}\",\n  \"system\": \"{system}\",\n  \
                 \"arch\": \"{arch}\",\n  \"programme\": [\n    {{ \"name\": \"quiver-devops\", \
                 \"version\": \"{FASSUNG}\", \"groesse\": {}, \"sha256\": \"{}\" }}\n  ]\n}}\n",
                inhalt.len(),
                hex::encode(Sha256::digest(inhalt))
            );
            std::fs::write(bau.join("LISTE.json"), liste).expect("Liste");
            self.packen(system, arch)
        }

        /// Den Bauordner (neu) als Archiv schreiben.
        fn packen(&self, system: &str, arch: &str) -> PathBuf {
            let name = if system == "windows" {
                "quiver-devops.exe"
            } else {
                "quiver-devops"
            };
            let release = self.release();
            std::fs::create_dir_all(&release).expect("Release-Ordner");
            let archiv = release.join(format!("quiver-tools-{FASSUNG}-{system}-{arch}.tar.gz"));
            let status = Command::new("tar")
                .args(["-czf"])
                .arg(&archiv)
                .arg("-C")
                .arg(self.bau(system, arch))
                .args([name, "LISTE.json"])
                .status()
                .expect("tar läuft");
            assert!(status.success(), "tar ist gescheitert");
            archiv
        }

        /// Alle fünf Archive plus `SHA256SUMS`.
        fn alle_archive(&self, abweichend: Option<(&str, &[u8])>) {
            let mut summen = String::new();
            for (system, arch, _, _) in ARCHIVE {
                let inhalt: &[u8] = match abweichend {
                    Some((treffer, inhalt)) if treffer == format!("{system}-{arch}").as_str() => {
                        inhalt
                    }
                    _ => b"programm",
                };
                let archiv = self.archiv(system, arch, inhalt);
                let bytes = std::fs::read(&archiv).expect("Archiv");
                summen.push_str(&format!(
                    "{}  {}\n",
                    hex::encode(Sha256::digest(&bytes)),
                    archiv.file_name().expect("Name").to_string_lossy()
                ));
            }
            std::fs::write(self.release().join("SHA256SUMS"), summen).expect("Summen");
        }

        /// Das Skript laufen lassen; Rückgabe: Erfolg, Ausgabe.
        fn laufen(&self, version: &str) -> (bool, String) {
            let ausgabe = Command::new("bash")
                .arg(wurzel().join("verteilung/programme.sh"))
                .env("QUIVER_TOOLS_VERSION", version)
                .env("QUIVER_TOOLS_QUELLE", self.release())
                .env("ZIEL", self.ziel())
                .env_remove("QUIVER_TOOLS_ARCHIVE")
                .env_remove("QUIVER_TOOLS_BASIS")
                .output()
                .expect("Skript läuft");
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&ausgabe.stdout),
                String::from_utf8_lossy(&ausgabe.stderr)
            );
            (ausgabe.status.success(), text)
        }

        /// Liegt unter dem Zielordner irgendeine Datei?
        fn etwas_entpackt(&self) -> bool {
            fn zaehlen(pfad: &Path) -> usize {
                let Ok(eintraege) = std::fs::read_dir(pfad) else {
                    return 0;
                };
                eintraege
                    .flatten()
                    .map(|eintrag| {
                        if eintrag.path().is_dir() {
                            zaehlen(&eintrag.path())
                        } else {
                            1
                        }
                    })
                    .sum()
            }
            zaehlen(&self.ziel()) > 0
        }
    }

    #[test]
    fn die_programme_kommen_aus_dem_release_in_die_ordner_des_abbilds() {
        let welt = Welt::neu("gut");
        welt.alle_archive(None);
        let (ok, text) = welt.laufen(FASSUNG);
        assert!(ok, "{text}");
        for (_, _, system, ordner) in ARCHIVE {
            let datei = if system == "windows" {
                "quiver-devops.exe"
            } else {
                "quiver-devops"
            };
            let pfad = welt.ziel().join(system).join(ordner).join(datei);
            assert!(pfad.is_file(), "{} fehlt\n{text}", pfad.display());
            assert_eq!(std::fs::read(&pfad).expect("Programm"), b"programm");
            assert!(welt
                .ziel()
                .join(system)
                .join(ordner)
                .join("LISTE.json")
                .is_file());
        }
        // Kein Ordner mit der Archiv-Architektur: der Client fragt `x64`/`arm64`.
        assert!(!welt.ziel().join("linux/x86_64").exists());
    }

    #[test]
    fn ein_archiv_ohne_pruefsumme_bricht_ab_bevor_entpackt_wird() {
        let welt = Welt::neu("ohne-summe");
        welt.alle_archive(None);
        // Die Zeile für das letzte Archiv fällt weg.
        let liste = std::fs::read_to_string(welt.release().join("SHA256SUMS")).expect("Summen");
        let gekuerzt: String = liste
            .lines()
            .filter(|zeile| !zeile.ends_with("linux-aarch64.tar.gz"))
            .map(|zeile| format!("{zeile}\n"))
            .collect();
        std::fs::write(welt.release().join("SHA256SUMS"), gekuerzt).expect("Summen");

        let (ok, text) = welt.laufen(FASSUNG);
        assert!(!ok, "der Bau muss abbrechen\n{text}");
        assert!(text.contains("SHA256SUMS"), "{text}");
        assert!(
            !welt.etwas_entpackt(),
            "es wurde entpackt, obwohl eine Prüfsumme fehlte"
        );
    }

    #[test]
    fn eine_falsche_pruefsumme_bricht_ab_bevor_entpackt_wird() {
        let welt = Welt::neu("falsche-summe");
        welt.alle_archive(None);
        let liste = std::fs::read_to_string(welt.release().join("SHA256SUMS")).expect("Summen");
        let verdreht: String = liste
            .lines()
            .map(|zeile| {
                if zeile.ends_with("linux-x86_64.tar.gz") {
                    format!(
                        "{}  {}",
                        "0".repeat(64),
                        zeile.split_whitespace().nth(1).unwrap()
                    )
                } else {
                    zeile.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(welt.release().join("SHA256SUMS"), verdreht).expect("Summen");

        let (ok, text) = welt.laufen(FASSUNG);
        assert!(!ok, "der Bau muss abbrechen\n{text}");
        assert!(text.contains("Prüfsumme"), "{text}");
        assert!(
            !welt.etwas_entpackt(),
            "es wurde entpackt, obwohl eine Prüfsumme nicht stimmte"
        );
    }

    #[test]
    fn ein_programm_aus_der_liste_das_fehlt_bricht_ab() {
        let welt = Welt::neu("liste");
        welt.alle_archive(None);
        // Die Liste des Linux-Archivs um ein Programm ergänzen, das nicht
        // mitkommt.
        let bau = welt.bau("linux", "aarch64");
        let liste = std::fs::read_to_string(bau.join("LISTE.json")).expect("Liste");
        let erweitert = liste.replace(
            "  ]",
            "    { \"name\": \"quiver-fehlt\", \"version\": \"9.9.9\", \"groesse\": 1, \"sha256\": \"\
             0000000000000000000000000000000000000000000000000000000000000000\" }\n  ]",
        );
        std::fs::write(bau.join("LISTE.json"), erweitert).expect("Liste");
        let archiv = welt.packen("linux", "aarch64");
        // Prüfsumme nachziehen, damit der Lauf bis zur Liste kommt.
        let bytes = std::fs::read(&archiv).expect("Archiv");
        let liste_pfad = welt.release().join("SHA256SUMS");
        let summen = std::fs::read_to_string(&liste_pfad).expect("Summen");
        let neu: String = summen
            .lines()
            .map(|zeile| {
                if zeile.ends_with("linux-aarch64.tar.gz") {
                    format!(
                        "{}  {}",
                        hex::encode(Sha256::digest(&bytes)),
                        zeile.split_whitespace().nth(1).unwrap()
                    )
                } else {
                    zeile.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&liste_pfad, neu).expect("Summen");

        let (ok, text) = welt.laufen(FASSUNG);
        assert!(!ok, "der Bau muss abbrechen\n{text}");
        assert!(text.contains("quiver-fehlt"), "{text}");
    }

    #[test]
    fn eine_leere_fassung_baut_ohne_programme() {
        let welt = Welt::neu("leer");
        let (ok, text) = welt.laufen("");
        assert!(ok, "{text}");
        assert!(text.contains("KEINE Werkzeug-Programme"), "{text}");
        assert!(!welt.etwas_entpackt());
    }
}

/// Die Fassung des Werkzeug-Releases steht **im Repo** (Dockerfile), nicht in
/// einer GitHub-Variablen: was ein Abbild enthält, soll aus dem Commit
/// hervorgehen (Aufgabe 37).
#[test]
fn die_fassung_steht_im_dockerfile_und_nicht_in_einer_variablen() {
    let dockerfile = lies("Dockerfile");
    assert!(
        dockerfile.contains("ARG QUIVER_TOOLS_VERSION=0.2.1"),
        "die Vorgabe fehlt im Dockerfile"
    );
    assert!(
        dockerfile.contains("verteilung/programme.sh"),
        "die Stufe »programme« benutzt das geprüfte Skript"
    );
    let abbild = lies(".github/workflows/abbild.yml");
    assert!(
        !abbild.contains("vars.QUIVER_TOOLS"),
        "die Fassung kommt nicht mehr aus einer Repo-Variablen"
    );
    assert!(
        !abbild.contains("QUIVER_TOOLS_VERSION=${{"),
        "kein Bau-Argument, das die Vorgabe aus dem Dockerfile überschreibt"
    );
}
