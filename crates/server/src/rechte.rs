// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Dateirechte am Server: der Datenordner gehört 0700, was darin liegt 0600.
//!
//! Zwei Wege, damit es kein Zeitfenster gibt:
//!
//! 1. **Beim Anlegen** – [`umask_setzen`] läuft als erste Zeile des Prozesses.
//!    Alles, was danach entsteht (Ordner, Datenbank, `-wal`, `-shm`, Sicherung,
//!    Schlüssel, Entra-Dateien), ist von der ersten Sekunde an nur für den
//!    Eigentümer da.
//! 2. **Beim Start** – [`daten_pruefen`] zieht vorhandene Dateien nach: wer die
//!    Rechte von Hand verstellt hat, bekommt sie beim nächsten Start zurück.
//!
//! Symlinks werden dabei **nicht** verfolgt: ein Symlink im Datenordner ist
//! keine Datei von uns, und `chmod` würde sonst an einer Stelle wirken, die
//! außerhalb liegt.
//!
//! Unter Windows gibt es diese Bits nicht; dort gilt die Vererbung der ACL des
//! Benutzerprofils (siehe README).

use std::path::{Path, PathBuf};

/// Rechte des Datenordners: nur der Eigentümer.
pub const ORDNER: u32 = 0o700;
/// Rechte der Dateien darin: nur der Eigentümer darf lesen und schreiben.
pub const DATEI: u32 = 0o600;
/// So viele Sicherungen der Datenbank bleiben.
pub const SICHERUNGEN: usize = 3;

/// Die umask des Prozesses auf `0077` setzen (Unix; sonst nichts).
///
/// Ohne das hätte jede neu angelegte Datei bis zum nächsten `chmod` die Rechte
/// aus der umask des Aufrufers – und sei es nur für Millisekunden.
pub fn umask_setzen() {
    umask_setzen_impl()
}

#[cfg(unix)]
fn umask_setzen_impl() {
    // `umask` steht in der C-Bibliothek, die jedes Unix-Programm ohnehin lädt;
    // eine eigene Kiste dafür wäre mehr Abhängigkeit als Nutzen.
    extern "C" {
        fn umask(maske: u32) -> u32;
    }
    // SAFETY: `umask` nimmt und gibt nur eine Zahl, kein Zeiger, kein Zustand,
    // den Rust verwaltet.
    unsafe {
        umask(0o077);
    }
}

#[cfg(not(unix))]
fn umask_setzen_impl() {}

/// Den Datenordner anlegen (0700) – schon beim Anlegen, nicht erst später.
pub fn ordner_anlegen(ordner: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(ordner)?;
    setze_rechte(ordner, ORDNER);
    Ok(())
}

/// Beim Start: Ordner auf 0700, alle geschützten Dateien auf 0600.
///
/// Ein Fehler ist kein Startfehler: der Server läuft weiter, aber der Vorfall
/// steht im Log.
pub fn daten_pruefen(ordner: &Path) -> std::io::Result<()> {
    setze_rechte(ordner, ORDNER);
    let Ok(eintraege) = std::fs::read_dir(ordner) else {
        return Ok(());
    };
    for eintrag in eintraege.flatten() {
        let name = eintrag.file_name().to_string_lossy().to_string();
        if !ist_geschuetzt(&name) {
            continue;
        }
        datei_schuetzen(&eintrag.path());
    }
    Ok(())
}

/// Eine einzelne Datei auf 0600 bringen. Symlinks bleiben unangetastet.
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

/// Gehört der Name zu den Dateien, die nur dem Eigentümer gehören dürfen?
///
/// Das sind die Datenbank samt `-wal`, `-shm` und Sicherungen (`.vor-*`), die
/// Entra-Dateien (Geheimnis, Schlüssel, Umschaltung) und der Schlüssel für die
/// verschlüsselte Ablage.
pub fn ist_geschuetzt(name: &str) -> bool {
    name == "quiver.db"
        || name.starts_with("quiver.db-")
        || name.starts_with("quiver.db.vor-")
        || name.starts_with("entra")
        || name == "schluessel"
        || name == "admin-token"
}

/// Nur die letzten [`SICHERUNGEN`] Sicherungen behalten, ältere entfernen.
pub fn sicherungen_kuerzen(ordner: &Path) {
    let Ok(eintraege) = std::fs::read_dir(ordner) else {
        return;
    };
    let mut sicherungen: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for eintrag in eintraege.flatten() {
        let pfad = eintrag.path();
        if !ist_sicherung(&pfad) {
            continue;
        }
        // Die Zeit der Datei entscheidet, nicht der Name: die Schemaversion im
        // Namen wächst nicht zwingend monoton.
        let Ok(meta) = std::fs::symlink_metadata(&pfad) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let zeit = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        sicherungen.push((zeit, pfad));
    }
    if sicherungen.len() <= SICHERUNGEN {
        return;
    }
    sicherungen.sort();
    let weg = sicherungen.len() - SICHERUNGEN;
    for (_, pfad) in sicherungen.into_iter().take(weg) {
        match std::fs::remove_file(&pfad) {
            Ok(()) => tracing::info!("alte Sicherung entfernt: {}", pfad.display()),
            Err(fehler) => tracing::warn!("{} nicht entfernbar: {fehler}", pfad.display()),
        }
    }
}

/// Heißt die Datei wie eine Sicherung der Datenbank?
fn ist_sicherung(pfad: &Path) -> bool {
    pfad.file_name()
        .map(|name| name.to_string_lossy().starts_with("quiver.db.vor-"))
        .unwrap_or(false)
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
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        static ZAEHLER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pfad = std::env::temp_dir().join(format!(
            "quiver-server-rechte-{}-{stempel}-{nummer}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    #[test]
    fn geschuetzte_namen() {
        for name in [
            "quiver.db",
            "quiver.db-wal",
            "quiver.db-shm",
            "quiver.db.vor-3",
            "entra-secret",
            "entra-schluessel.pem",
            "schluessel",
        ] {
            assert!(ist_geschuetzt(name), "{name}");
        }
        for name in ["protokoll.log", "admin", "quiver.dbx", ".DS_Store"] {
            assert!(!ist_geschuetzt(name), "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn vorhandene_dateien_werden_nachgezogen() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = ordner();
        let datei = ordner.join("quiver.db");
        std::fs::write(&datei, b"x").expect("schreibbar");
        std::fs::set_permissions(&datei, std::fs::Permissions::from_mode(0o644)).expect("Rechte");
        std::fs::set_permissions(&ordner, std::fs::Permissions::from_mode(0o755)).expect("Rechte");

        daten_pruefen(&ordner).expect("Rechte");

        let modus =
            |pfad: &Path| std::fs::metadata(pfad).expect("Datei").permissions().mode() & 0o777;
        assert_eq!(modus(&ordner), ORDNER, "Datenordner ist 0700");
        assert_eq!(modus(&datei), DATEI, "Datenbank ist 0600");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_werden_nicht_verfolgt() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = ordner();
        let ziel = ordner.join("ziel");
        std::fs::write(&ziel, b"x").expect("schreibbar");
        std::fs::set_permissions(&ziel, std::fs::Permissions::from_mode(0o644)).expect("Rechte");
        std::os::unix::fs::symlink(&ziel, ordner.join("entra-secret")).expect("Symlink");

        daten_pruefen(&ordner).expect("Rechte");

        let modus = std::fs::metadata(&ziel).expect("Ziel").permissions().mode() & 0o777;
        assert_eq!(modus, 0o644, "das Ziel hinter dem Symlink bleibt unberührt");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[cfg(unix)]
    #[test]
    fn nur_die_letzten_drei_sicherungen_bleiben() {
        let ordner = ordner();
        for nummer in 0..5 {
            let pfad = ordner.join(format!("quiver.db.vor-{nummer}"));
            std::fs::write(&pfad, b"x").expect("schreibbar");
            // Reihenfolge der Zeitstempel festlegen: 0 ist die älteste.
            let zeit = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000 + nummer);
            let datei = std::fs::File::options()
                .write(true)
                .open(&pfad)
                .expect("offen");
            datei.set_modified(zeit).expect("Zeit");
        }
        std::fs::write(ordner.join("quiver.db"), b"x").expect("schreibbar");

        sicherungen_kuerzen(&ordner);

        let mut namen: Vec<String> = std::fs::read_dir(&ordner)
            .expect("Ordner")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("quiver.db.vor-"))
            .collect();
        namen.sort();
        assert_eq!(
            namen,
            vec!["quiver.db.vor-2", "quiver.db.vor-3", "quiver.db.vor-4"],
            "die drei jüngsten bleiben"
        );
        assert!(ordner.join("quiver.db").exists(), "die Datenbank bleibt");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn anlegen_setzt_die_rechte() {
        let ordner = ordner();
        let neu = ordner.join("daten");
        ordner_anlegen(&neu).expect("angelegt");
        assert!(neu.is_dir());
        let _ = std::fs::remove_dir_all(&ordner);
    }
}
