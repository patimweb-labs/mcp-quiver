// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Dateirechte am Client: der Profilordner gehört 0700, was darin liegt 0600.
//!
//! Im Profilordner stehen Dinge, die nur diesen Benutzer etwas angehen: der
//! Gerätezustand, die Oberflächen-Einstellungen, die Protokolle und die
//! Sperrdatei des Tresors. Dass der Tresor selbst verschlüsselt ist, hilft
//! nicht, wenn die Protokolle und der Zustand für andere lesbar sind.
//!
//! Drei Wege, damit es kein Zeitfenster gibt:
//!
//! 1. **Alles neu Angelegte** entsteht mit 0700 bzw. 0600 ([`ordner_anlegen`],
//!    [`schreiben_atomar`]).
//! 2. **Beim Start** zieht [`profil_pruefen`] vorhandene Dateien nach – wer die
//!    Rechte von Hand verstellt hat (oder aus einer alten Fassung kommt),
//!    bekommt sie zurück.
//! 3. **Symlinks** am Ziel werden nicht verfolgt: geschrieben wird über eine
//!    Zwischendatei und ein Umbenennen, das den Symlink selbst ersetzt; beim
//!    Nachziehen der Rechte wird ein Symlink übersprungen.
//!
//! Windows kennt diese Bits nicht; dort gilt die Vererbung der ACL des
//! Benutzerprofils (siehe README, „Daten und Datenschutz“).

use std::io::Write;
use std::path::{Path, PathBuf};

/// Rechte des Profilordners: nur der Eigentümer.
pub const ORDNER: u32 = 0o700;
/// Rechte der Dateien darin.
pub const DATEI: u32 = 0o600;
/// So alte Zwischendateien (`*.neu`) dürfen beim Start noch liegen bleiben.
pub const REST_ALTER: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Die umask des Prozesses auf `0077` setzen (Unix; sonst nichts).
///
/// Damit ist auch das, was außerhalb unserer Funktionen entsteht, von der
/// ersten Sekunde an nur für den Benutzer da.
pub fn umask_setzen() {
    umask_setzen_impl()
}

#[cfg(unix)]
fn umask_setzen_impl() {
    // `umask` steht in der C-Bibliothek, die jedes Unix-Programm ohnehin lädt.
    extern "C" {
        fn umask(maske: u32) -> u32;
    }
    // SAFETY: `umask` nimmt und gibt nur eine Zahl, kein Zeiger.
    unsafe {
        umask(0o077);
    }
}

#[cfg(not(unix))]
fn umask_setzen_impl() {}

/// Einen Ordner anlegen (0700) – auch einen, den es schon gibt.
pub fn ordner_anlegen(ordner: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(ordner)?;
    setze_rechte(ordner, ORDNER);
    Ok(())
}

/// Eine Datei schreiben: erst daneben (0600, neue Datei), dann umbenennen.
///
/// Das Umbenennen ersetzt das Ziel; ein Symlink am Ziel wird also nicht
/// befolgt, sondern durch die neue Datei ersetzt. Die Datei ist entweder ganz
/// alt oder ganz neu – nie halb.
pub fn schreiben_atomar(pfad: &Path, inhalt: &[u8]) -> anyhow::Result<()> {
    if let Some(ordner) = pfad.parent() {
        // Was wir anlegen, gehört von Anfang an nur dem Benutzer. Was schon da
        // ist, bleibt wie es ist: hat jemand den Ordner gesperrt, bleibt er
        // gesperrt (und das Schreiben scheitert, wie es soll). Nachziehen tut
        // der Start (`profil_pruefen`).
        if !ordner.as_os_str().is_empty() && !ordner.exists() {
            ordner_anlegen(ordner)?;
        }
    }
    let zwischen = zwischenpfad(pfad);
    {
        let mut optionen = std::fs::OpenOptions::new();
        optionen.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            optionen.mode(DATEI);
        }
        let mut datei = optionen.open(&zwischen)?;
        datei.write_all(inhalt)?;
        datei.sync_all()?;
    }
    if let Err(fehler) = std::fs::rename(&zwischen, pfad) {
        let _ = std::fs::remove_file(&zwischen);
        return Err(fehler.into());
    }
    setze_rechte(pfad, DATEI);
    // Ohne `fsync` auf den Ordner kann der neue Name nach einem Stromausfall
    // fehlen.
    if let Some(ordner) = pfad.parent() {
        if let Ok(ordner) = std::fs::File::open(ordner) {
            let _ = ordner.sync_all();
        }
    }
    Ok(())
}

/// Der Zwischenname: `<ziel>.<pid>.<zufall>.neu`. PID **und** Zufall: zwei
/// Aufgaben im selben Prozess nutzen nie dieselbe Datei, und was nach einem
/// Absturz liegen blieb, blockiert kein späteres Schreiben. Mit `create_new`
/// belegt, damit nie ein fremder Inhalt überschrieben wird.
pub(crate) fn zwischenpfad(ziel: &Path) -> PathBuf {
    use rand::RngCore;
    let mut zufall = [0u8; 8];
    rand::rng().fill_bytes(&mut zufall);
    let mut name = ziel.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        ".{}.{}.neu",
        std::process::id(),
        hex::encode(zufall)
    ));
    ziel.with_file_name(name)
}

/// Eine vorhandene Datei auf 0600 bringen; Symlinks bleiben unangetastet.
pub fn datei_schuetzen(pfad: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(pfad) else {
        return;
    };
    if meta.file_type().is_symlink() {
        tracing::warn!(
            "{} ist ein Symlink; die Rechte werden nicht verfolgt",
            pfad.display()
        );
        return;
    }
    if !meta.file_type().is_file() {
        return;
    }
    if zu_offen(&meta) {
        tracing::warn!(
            "{} war für Gruppe oder alle zugänglich; Rechte auf 0600 gesetzt",
            pfad.display()
        );
    }
    setze_rechte(pfad, DATEI);
}

/// Beim Start: Profilordner 0700, alle Dateien darin 0600.
///
/// Unterordner (etwa `protokoll/` oder `tresor/`) bekommen 0700; hineingesehen
/// wird eine Ebene tief, weil dort genau die Dateien liegen, um die es geht.
pub fn profil_pruefen(ordner: &Path) -> std::io::Result<()> {
    ordner_anlegen(ordner)?;
    let jetzt = std::time::SystemTime::now();
    // Was nach einem Absturz liegen blieb, blockiert kein Schreiben mehr (der
    // Zwischenname ist zufällig), wäre aber für immer da. Ältere Reste weg.
    reste_entfernen(ordner, jetzt);
    let Ok(eintraege) = std::fs::read_dir(ordner) else {
        return Ok(());
    };
    for eintrag in eintraege.flatten() {
        let pfad = eintrag.path();
        let Ok(meta) = std::fs::symlink_metadata(&pfad) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            setze_rechte(&pfad, ORDNER);
            reste_entfernen(&pfad, jetzt);
            let Ok(innen) = std::fs::read_dir(&pfad) else {
                continue;
            };
            for kind in innen.flatten() {
                datei_schuetzen(&kind.path());
            }
        } else {
            datei_schuetzen(&pfad);
        }
    }
    Ok(())
}

/// Liegen gebliebene Zwischendateien (`*.neu`) entfernen, wenn sie älter als
/// [`REST_ALTER`] sind. Gibt zurück, wie viele es waren.
pub fn reste_entfernen(ordner: &Path, jetzt: std::time::SystemTime) -> usize {
    let Ok(eintraege) = std::fs::read_dir(ordner) else {
        return 0;
    };
    let mut weg = 0;
    for eintrag in eintraege.flatten() {
        let pfad = eintrag.path();
        if !pfad
            .file_name()
            .map(|name| name.to_string_lossy().ends_with(".neu"))
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&pfad) else {
            continue;
        };
        // Symlinks und Ordner fasst niemand an.
        if !meta.file_type().is_file() {
            continue;
        }
        let alt = meta
            .modified()
            .ok()
            .and_then(|zeit| jetzt.duration_since(zeit).ok())
            .is_some_and(|alter| alter > REST_ALTER);
        if !alt {
            continue;
        }
        match std::fs::remove_file(&pfad) {
            Ok(()) => {
                weg += 1;
                tracing::info!("Zwischendatei {} entfernt", pfad.display());
            }
            Err(fehler) => tracing::warn!(
                "Zwischendatei {} nicht entfernbar: {fehler}",
                pfad.display()
            ),
        }
    }
    weg
}

#[cfg(unix)]
fn zu_offen(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o077 != 0
}

#[cfg(not(unix))]
fn zu_offen(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn setze_rechte(pfad: &Path, modus: u32) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(fehler) = std::fs::set_permissions(pfad, std::fs::Permissions::from_mode(modus)) {
        tracing::warn!("Rechte von {} nicht setzbar: {fehler}", pfad.display());
    }
}

#[cfg(not(unix))]
fn setze_rechte(_pfad: &Path, _modus: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner() -> PathBuf {
        static ZAEHLER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pfad = std::env::temp_dir().join(format!(
            "quiver-client-rechte-{}-{stempel}-{nummer}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    #[cfg(unix)]
    #[test]
    fn neues_ist_von_anfang_an_dicht() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = ordner();
        let ziel = ordner.join("profil").join("zustand.json");
        schreiben_atomar(&ziel, b"{}").expect("schreibbar");
        let modus =
            |pfad: &Path| std::fs::metadata(pfad).expect("Datei").permissions().mode() & 0o777;
        assert_eq!(modus(&ordner.join("profil")), ORDNER, "Profilordner");
        assert_eq!(modus(&ziel), DATEI, "Datei");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[cfg(unix)]
    #[test]
    fn vorhandenes_wird_beim_start_nachgezogen() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = ordner();
        let profil = ordner.join("profil");
        for name in ["zustand.json", "oberflaeche.json"] {
            std::fs::create_dir_all(&profil).expect("Ordner");
            let pfad = profil.join(name);
            std::fs::write(&pfad, b"{}").expect("schreibbar");
            std::fs::set_permissions(&pfad, std::fs::Permissions::from_mode(0o644))
                .expect("Rechte");
        }
        let protokoll = profil.join("protokoll");
        std::fs::create_dir_all(&protokoll).expect("Ordner");
        let log = protokoll.join("werkzeug-2026-01-01.jsonl");
        std::fs::write(&log, b"{}\n").expect("schreibbar");
        std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o644)).expect("Rechte");
        std::fs::set_permissions(&profil, std::fs::Permissions::from_mode(0o755)).expect("Rechte");
        std::fs::set_permissions(&protokoll, std::fs::Permissions::from_mode(0o755))
            .expect("Rechte");

        profil_pruefen(&profil).expect("Rechte");

        let modus =
            |pfad: &Path| std::fs::metadata(pfad).expect("Datei").permissions().mode() & 0o777;
        assert_eq!(modus(&profil), ORDNER, "Profilordner");
        assert_eq!(modus(&profil.join("zustand.json")), DATEI);
        assert_eq!(modus(&profil.join("oberflaeche.json")), DATEI);
        assert_eq!(modus(&protokoll), ORDNER, "Protokollordner");
        assert_eq!(modus(&log), DATEI, "Protokoll");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[cfg(unix)]
    #[test]
    fn ein_symlink_am_ziel_wird_ersetzt_und_nicht_verfolgt() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = ordner();
        let fremd = ordner.join("fremd");
        std::fs::write(&fremd, b"unangetastet").expect("schreibbar");
        std::fs::set_permissions(&fremd, std::fs::Permissions::from_mode(0o666)).expect("Rechte");
        let profil = ordner.join("profil");
        std::fs::create_dir_all(&profil).expect("Ordner");
        let ziel = profil.join("zustand.json");
        std::os::unix::fs::symlink(&fremd, &ziel).expect("Symlink");

        // Beim Nachziehen bleibt das Ziel hinter dem Symlink unberührt.
        profil_pruefen(&profil).expect("Rechte");
        assert_eq!(
            std::fs::metadata(&fremd)
                .expect("Ziel")
                .permissions()
                .mode()
                & 0o777,
            0o666
        );

        // Schreiben ersetzt den Symlink, nicht sein Ziel.
        schreiben_atomar(&ziel, b"{\"a\":1}").expect("schreibbar");
        assert_eq!(std::fs::read(&fremd).expect("Ziel"), b"unangetastet");
        assert!(!std::fs::symlink_metadata(&ziel)
            .expect("Datei")
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(&ziel).expect("Datei"), b"{\"a\":1}");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Nach einem Absturz liegen gebliebene Zwischendateien werden beim Start
    /// entfernt, wenn sie älter als eine Stunde sind (Härtung, Punkt 17).
    #[test]
    fn alte_zwischendateien_verschwinden_beim_start() {
        let ordner = ordner();
        let profil = ordner.join("profil");
        std::fs::create_dir_all(&profil).expect("Ordner");
        let alt = profil.join("zustand.json.1234.deadbeef.neu");
        let frisch = profil.join("oberflaeche.json.1234.cafebabe.neu");
        std::fs::write(&alt, b"alt").expect("schreibbar");
        std::fs::write(&frisch, b"frisch").expect("schreibbar");
        let vor_zwei_stunden =
            std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
        std::fs::File::options()
            .write(true)
            .open(&alt)
            .expect("offen")
            .set_modified(vor_zwei_stunden)
            .expect("Zeit setzbar");

        profil_pruefen(&profil).expect("Rechte");

        assert!(!alt.exists(), "die alte Zwischendatei ist weg");
        assert!(frisch.exists(), "die frische bleibt");
        // Der Zwischenname trägt PID **und** Zufall: zwei Aufgaben im selben
        // Prozess kommen sich nicht in die Quere.
        let a = zwischenpfad(&profil.join("zustand.json"));
        let b = zwischenpfad(&profil.join("zustand.json"));
        assert_ne!(a, b, "der Zufall macht den Namen eindeutig");
        assert!(a.to_string_lossy().ends_with(".neu"));
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn schreiben_ist_ganz_oder_gar_nicht() {
        let ordner = ordner();
        let ziel = ordner.join("zustand.json");
        schreiben_atomar(&ziel, b"eins").expect("schreibbar");
        schreiben_atomar(&ziel, b"zwei").expect("schreibbar");
        assert_eq!(std::fs::read(&ziel).expect("Datei"), b"zwei");
        // Keine Zwischendatei zurückgelassen.
        let reste: Vec<_> = std::fs::read_dir(&ordner)
            .expect("Ordner")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".neu"))
            .collect();
        assert!(reste.is_empty(), "Zwischendateien: {reste:?}");
        let _ = std::fs::remove_dir_all(&ordner);
    }
}
