// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Autostart im Zusammenspiel: [`aktiv`] und [`setzen`] arbeiten auf der Datei,
//! die `QUIVER_AUTOSTART_DIR` nennt — **nicht** im echten Autostart-Ordner des
//! Rechners. Ein eigener Testprozess, damit die Umgebungsvariable niemanden
//! sonst stört.

use std::path::PathBuf;

use quiver_fenster::autostart;

fn umgebung() -> PathBuf {
    let pfad = std::env::temp_dir().join(format!("quiver-autostart-test-{}", std::process::id()));
    std::fs::remove_dir_all(&pfad).ok();
    std::fs::create_dir_all(&pfad).expect("Ordner");
    pfad
}

#[test]
fn schalter_legt_und_entfernt_den_eintrag() {
    let ordner = umgebung();
    std::env::set_var(autostart::UMGEBUNG, &ordner);

    // Ausgangslage: nichts eingetragen.
    assert!(!autostart::aktiv(), "frischer Ordner ist leer");
    let datei = autostart::datei().expect("Ziel im Testordner");
    assert!(datei.starts_with(&ordner), "{datei:?} liegt im Testordner");

    // An: die Datei liegt da und startet mit `--hintergrund`.
    autostart::setzen(true).expect("einschaltbar");
    assert!(autostart::aktiv(), "Zustand kommt aus dem System");
    let text = std::fs::read_to_string(&datei).expect("lesbar");
    assert!(
        text.contains(autostart::ARGUMENT),
        "ohne --hintergrund startet quiver mit Fenster: {text}"
    );
    // An bleibt an (idempotent).
    autostart::setzen(true).expect("einschaltbar");
    assert!(autostart::aktiv());

    // Aus: die Datei ist weg.
    autostart::setzen(false).expect("ausschaltbar");
    assert!(!autostart::aktiv());
    assert!(!datei.is_file());
    // Aus bleibt aus.
    autostart::setzen(false).expect("ausschaltbar");

    std::fs::remove_dir_all(&ordner).ok();
}
