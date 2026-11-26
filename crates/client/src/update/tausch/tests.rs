// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests für den Tausch (Aufgabe 39): im Testordner, mit einer Attrappe als
//! „neue Fassung“. Kein echtes Programm wird ersetzt, kein Netz, kein Profil
//! angefasst.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;

use super::*;

/// Ein frischer Testordner (Prozessnummer und Zähler: ohne Doppelpunkt).
fn testordner() -> PathBuf {
    static ZAEHLER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let ordner =
        std::env::temp_dir().join(format!("quiver-tausch-{}-{nummer}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ordner);
    std::fs::create_dir_all(&ordner).expect("Ordner");
    ordner
}

/// Ein ZIP mit einer Datei an einem beliebigen Ort (gespeichert).
fn zip_mit(name: &str, inhalt: &[u8]) -> Vec<u8> {
    let mut daten = Vec::new();
    let kopf = daten.len();
    daten.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
    daten.extend_from_slice(&20u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u32.to_le_bytes());
    daten.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    daten.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    daten.extend_from_slice(&(name.len() as u16).to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(name.as_bytes());
    daten.extend_from_slice(inhalt);
    let verzeichnis = daten.len();
    let mut mitte = Vec::new();
    mitte.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
    mitte.extend_from_slice(&20u16.to_le_bytes());
    mitte.extend_from_slice(&20u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u32.to_le_bytes());
    mitte.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    mitte.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    mitte.extend_from_slice(&(name.len() as u16).to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u32.to_le_bytes());
    mitte.extend_from_slice(&(kopf as u32).to_le_bytes());
    mitte.extend_from_slice(name.as_bytes());
    daten.extend_from_slice(&mitte);
    daten.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&1u16.to_le_bytes());
    daten.extend_from_slice(&1u16.to_le_bytes());
    daten.extend_from_slice(&(mitte.len() as u32).to_le_bytes());
    daten.extend_from_slice(&(verzeichnis as u32).to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten
}

/// Ein tar.gz mit einer Datei.
fn tar_gz_mit(name: &str, inhalt: &[u8]) -> Vec<u8> {
    let mut archiv = Vec::new();
    let mut kopf = [0u8; 512];
    kopf[..name.len()].copy_from_slice(name.as_bytes());
    let groesse = format!("{:07o}\0", inhalt.len());
    kopf[124..124 + groesse.len()].copy_from_slice(groesse.as_bytes());
    kopf[156] = b'0';
    kopf[257..262].copy_from_slice(b"ustar");
    archiv.extend_from_slice(&kopf);
    archiv.extend_from_slice(inhalt);
    archiv.extend(std::iter::repeat_n(0u8, (512 - inhalt.len() % 512) % 512));
    archiv.extend(std::iter::repeat_n(0u8, 1024));
    let mut bauer = GzEncoder::new(Vec::new(), flate2::Compression::default());
    bauer.write_all(&archiv).expect("tar");
    bauer.finish().expect("gzip")
}

/// Der Selbsttest der Attrappe: sie besteht ihn, wenn die Datei „gut“ heißt.
fn selbsttest_gut(pfad: &Path) -> bool {
    std::fs::read(pfad).is_ok_and(|inhalt| inhalt.starts_with(b"gut"))
}

#[test]
fn ein_zip_wird_getauscht_und_die_alte_fassung_bleibt_liegen() {
    let ordner = testordner();
    let programm = ordner.join(programmname());
    std::fs::write(&programm, b"alt").expect("alte Fassung");
    let paket = ordner.join("quiver.zip");
    std::fs::write(
        &paket,
        zip_mit(&format!("quiver-0.5.0/{}", programmname()), b"gut-neu"),
    )
    .expect("Paket");

    let ergebnis = tauschen(
        Installationsart::Zip,
        &programm,
        &paket,
        &ordner.join("arbeit"),
        selbsttest_gut,
    )
    .expect("getauscht");
    let Ergebnis::Getauscht {
        programm: ziel,
        sicherung,
    } = ergebnis
    else {
        panic!("erwartet: getauscht");
    };
    assert_eq!(ziel, programm);
    assert_eq!(std::fs::read(&programm).expect("neu"), b"gut-neu");
    assert_eq!(std::fs::read(&sicherung).expect("Sicherung"), b"alt");
    assert_eq!(sicherung, sicherung_pfad(&programm));
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_tar_gz_wird_getauscht() {
    let ordner = testordner();
    let programm = ordner.join(programmname());
    std::fs::write(&programm, b"alt").expect("alte Fassung");
    let paket = ordner.join("quiver.tar.gz");
    std::fs::write(
        &paket,
        tar_gz_mit(&format!("quiver-0.5.0/bin/{}", programmname()), b"gut-neu"),
    )
    .expect("Paket");
    let ergebnis = tauschen(
        Installationsart::TarGz,
        &programm,
        &paket,
        &ordner.join("arbeit"),
        selbsttest_gut,
    )
    .expect("getauscht");
    assert!(matches!(ergebnis, Ergebnis::Getauscht { .. }));
    assert_eq!(std::fs::read(&programm).expect("neu"), b"gut-neu");
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_appimage_wird_ersetzt() {
    let ordner = testordner();
    let programm = ordner.join("quiver.AppImage");
    std::fs::write(&programm, b"alt").expect("alte Fassung");
    let paket = ordner.join("quiver-0.5.0.AppImage");
    std::fs::write(&paket, b"gut-neu").expect("Paket");
    let ergebnis = tauschen(
        Installationsart::AppImage,
        &programm,
        &paket,
        &ordner.join("arbeit"),
        selbsttest_gut,
    )
    .expect("getauscht");
    assert!(matches!(ergebnis, Ergebnis::Getauscht { .. }));
    assert_eq!(std::fs::read(&programm).expect("neu"), b"gut-neu");
    assert_eq!(
        std::fs::read(sicherung_pfad(&programm)).expect("Sicherung"),
        b"alt"
    );
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_gescheiterter_selbsttest_tauscht_nichts() {
    let ordner = testordner();
    let programm = ordner.join(programmname());
    std::fs::write(&programm, b"alt").expect("alte Fassung");
    let paket = ordner.join("quiver.zip");
    std::fs::write(&paket, zip_mit(programmname(), b"kaputt")).expect("Paket");
    let fehler = tauschen(
        Installationsart::Zip,
        &programm,
        &paket,
        &ordner.join("arbeit"),
        selbsttest_gut,
    )
    .expect_err("der Selbsttest scheitert");
    assert!(format!("{fehler:#}").contains("Selbsttest"), "{fehler:#}");
    // Alles ist, wie es war: keine Sicherung, kein neues Programm.
    assert_eq!(std::fs::read(&programm).expect("alt"), b"alt");
    assert!(!sicherung_pfad(&programm).exists());
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_deb_oder_rpm_wird_nicht_getauscht() {
    let ordner = testordner();
    for art in [
        Installationsart::Deb,
        Installationsart::Rpm,
        Installationsart::Unbekannt,
    ] {
        let fehler = tauschen(
            art,
            &ordner.join("quiver"),
            &ordner.join("paket"),
            &ordner.join("arbeit"),
            selbsttest_gut,
        )
        .expect_err("nicht tauschbar");
        assert!(format!("{fehler:#}").contains("nicht selbst"), "{fehler:#}");
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_msi_wird_dem_installationsprogramm_gegeben() {
    let ordner = testordner();
    let programm = ordner.join(programmname());
    let paket = ordner.join("quiver.msi");
    std::fs::write(&paket, b"msi").expect("Paket");
    let ergebnis = tauschen(
        Installationsart::Msi,
        &programm,
        &paket,
        &ordner.join("arbeit"),
        |_| panic!("ein MSI hat keinen Selbsttest"),
    )
    .expect("MSI");
    assert_eq!(
        ergebnis,
        Ergebnis::Installer {
            programm,
            paket: paket.clone()
        }
    );
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Nur auf macOS: das `.app` aus einem `.dmg` – mit `hdiutil` und `ditto`,
/// also mit einem wirklich gebauten Datenträger (kein Netz, kein Profil).
#[cfg(target_os = "macos")]
#[test]
fn ein_app_buendel_wird_getauscht() {
    let ordner = testordner();
    // Ein Bündel bauen und als .dmg verpacken.
    let quelle = ordner.join("quelle");
    let app = quelle.join("quiver.app/Contents/MacOS");
    std::fs::create_dir_all(&app).expect("Bündel");
    std::fs::write(app.join("quiver"), b"gut-neu").expect("Programm");
    let dmg = ordner.join("quiver-0.5.0-macos-arm64.dmg");
    let status = std::process::Command::new("hdiutil")
        .arg("create")
        .arg("-quiet")
        .arg("-volname")
        .arg("quiver")
        .arg("-srcfolder")
        .arg(quelle.join("quiver.app"))
        .arg(&dmg)
        .status()
        .expect("hdiutil");
    assert!(status.success(), "das Test-DMG ließ sich nicht bauen");

    // Das laufende Bündel.
    let ziel = ordner.join("quiver.app");
    let ziel_app = ziel.join("Contents/MacOS");
    std::fs::create_dir_all(&ziel_app).expect("Ziel");
    std::fs::write(ziel_app.join("quiver"), b"alt").expect("alte Fassung");

    let ergebnis = tauschen(
        Installationsart::AppBundle,
        &ziel,
        &dmg,
        &ordner.join("arbeit"),
        selbsttest_gut,
    )
    .expect("getauscht");
    let Ergebnis::Getauscht {
        programm,
        sicherung,
    } = ergebnis
    else {
        panic!("erwartet: getauscht");
    };
    assert_eq!(programm, ziel);
    assert_eq!(
        std::fs::read(ziel_app.join("quiver")).expect("neu"),
        b"gut-neu"
    );
    assert_eq!(
        std::fs::read(sicherung.join("Contents/MacOS/quiver")).expect("Sicherung"),
        b"alt"
    );
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn die_marke_zaehlt_und_gibt_die_alte_fassung_zurueck() {
    let ordner = testordner();
    let profil = ordner.join("profil");
    let programm = ordner.join(programmname());
    std::fs::write(&programm, b"neu").expect("neu");
    let sicherung = ordner.join(format!("{}.alt", programmname()));
    std::fs::write(&sicherung, b"alt").expect("alt");
    marke_schreiben(&profil, &programm, &sicherung).expect("Marke");

    // Der erste Start: die neue Fassung läuft weiter, die Marke bleibt.
    assert_eq!(rueckfall(&profil), None);
    assert!(profil.join(MARKE).exists());
    assert_eq!(std::fs::read(&programm).expect("neu"), b"neu");

    // Der zweite Start ohne Beweis: die alte Fassung kommt zurück.
    assert_eq!(rueckfall(&profil), Some(programm.clone()));
    assert_eq!(std::fs::read(&programm).expect("alt"), b"alt");
    assert!(!profil.join(MARKE).exists(), "die Marke ist verbraucht");

    // Nach einem Beweis bleibt nichts liegen.
    marke_schreiben(&profil, &programm, &sicherung).expect("Marke");
    bestaetigen(&profil);
    assert!(!profil.join(MARKE).exists());
    assert!(!sicherung.exists(), "die Sicherung ist weg");
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_fremder_neustart_wird_einmal_uebernommen() {
    let ordner = testordner();
    let profil = ordner.join("profil");
    let programm = ordner.join(programmname());
    neustart_vormerken(&profil, &programm).expect("Neustart");
    assert_eq!(neustart_uebernehmen(&profil), Some(programm));
    assert_eq!(neustart_uebernehmen(&profil), None, "nur einmal");
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn die_sicherung_liegt_neben_dem_programm() {
    assert_eq!(
        sicherung_pfad(Path::new("/a/b/quiver")),
        PathBuf::from("/a/b/quiver.alt")
    );
    assert_eq!(
        sicherung_pfad(Path::new("/a/b/quiver.exe")),
        PathBuf::from("/a/b/quiver.exe.alt")
    );
}
