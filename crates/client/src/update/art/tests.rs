// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tests für die Erkennung der Installation (Aufgabe 39). Reine Rechnung über
//! Pfade – kein echtes System, kein Schreibzugriff außer im Testordner.

use super::*;

fn umgebung(system: &str) -> Umgebung {
    Umgebung {
        system: system.to_string(),
        lokal: Some(PathBuf::from("/home/test/AppData/Local")),
        dpkg: true,
    }
}

#[test]
fn unter_windows_ist_es_zip_oder_msi() {
    let win = umgebung("windows");
    // Portabel: irgendwo, etwa im Download-Ordner.
    assert_eq!(
        art_erkennen(
            Path::new("/home/test/Downloads/quiver-windows-x86_64/quiver.exe"),
            &win
        ),
        Installationsart::Zip
    );
    // MSI je Benutzer: `%LOCALAPPDATA%\Programs\mcp-quiver`.
    assert_eq!(
        art_erkennen(
            Path::new("/home/test/AppData/Local/Programs/mcp-quiver/quiver.exe"),
            &win
        ),
        Installationsart::Msi
    );
    // Ein Ordner, der nur ähnlich heißt, ist kein MSI.
    assert_eq!(
        art_erkennen(
            Path::new("/home/test/AppData/Local/ProgramsAnderes/quiver.exe"),
            &win
        ),
        Installationsart::Zip
    );
}

#[test]
fn unter_macos_zaehlt_das_buendel() {
    let mac = umgebung("macos");
    assert_eq!(
        art_erkennen(
            Path::new("/Applications/quiver.app/Contents/MacOS/quiver"),
            &mac
        ),
        Installationsart::AppBundle
    );
    // Neben dem Bündel (etwa aus einem Archiv) erkennt der Client nichts: dann
    // tauscht er nicht, sondern sagt es.
    assert_eq!(
        art_erkennen(Path::new("/Users/test/quiver"), &mac),
        Installationsart::Unbekannt
    );
    assert_eq!(
        bundel_von(Path::new("/Applications/quiver.app/Contents/MacOS/quiver")),
        Some(PathBuf::from("/Applications/quiver.app"))
    );
    assert_eq!(bundel_von(Path::new("/usr/local/bin/quiver")), None);
}

#[test]
fn unter_linux_zaehlt_appimage_tar_und_paket() {
    let linux = umgebung("linux");
    assert_eq!(
        art_erkennen(Path::new("/home/test/Downloads/quiver.AppImage"), &linux),
        Installationsart::AppImage
    );
    assert_eq!(
        art_erkennen(
            Path::new("/home/test/quiver-0.5.0-linux-x86_64/bin/quiver"),
            &linux
        ),
        Installationsart::TarGz
    );
    // Aus einem Paket: /usr gehört root, der Client tauscht dort nicht.
    assert_eq!(
        art_erkennen(Path::new("/usr/bin/quiver"), &linux),
        Installationsart::Deb
    );
    let rpm = Umgebung {
        dpkg: false,
        ..umgebung("linux")
    };
    assert_eq!(
        art_erkennen(Path::new("/usr/bin/quiver"), &rpm),
        Installationsart::Rpm
    );
}

#[test]
fn ein_unbekanntes_system_tauscht_nichts() {
    let anders = umgebung("plan9");
    assert_eq!(
        art_erkennen(Path::new("/quiver"), &anders),
        Installationsart::Unbekannt
    );
    assert!(!Installationsart::Unbekannt.tauschbar());
    assert!(!Installationsart::Deb.tauschbar());
    assert!(!Installationsart::Rpm.tauschbar());
    assert!(Installationsart::Zip.tauschbar());
}

#[test]
fn die_aufstellung_erkennt_fehlendes_schreibrecht() {
    // Ein Testordner ohne Programm: der Ordner ist schreibbar, also ja.
    let ordner = std::env::temp_dir().join(format!(
        "quiver-art-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dauer| dauer.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&ordner).expect("Ordner");
    let programm = ordner.join("quiver");
    std::fs::write(&programm, b"probe").expect("Programm");
    let aufstellung = Aufstellung::aus(&programm, &umgebung("linux"));
    assert!(
        aufstellung.schreibbar,
        "im eigenen Ordner darf getauscht werden"
    );
    assert_eq!(aufstellung.programm, programm);

    // Ein Ordner, den es nicht gibt: kein Schreibrecht.
    assert!(!ordner_schreibbar(
        &ordner.join("gibtsnicht").join("quiver")
    ));
    // Die Probe hinterlässt nichts.
    assert!(!ordner
        .join(format!(".quiver-probe-{}", std::process::id()))
        .exists());
    let _ = std::fs::remove_dir_all(&ordner);
}
