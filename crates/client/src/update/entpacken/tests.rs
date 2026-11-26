// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Patrick Weppelmann

//! Tests für das Entpacken (Aufgabe 39): ZIP gespeichert und deflate, tar.gz,
//! und die Wege, die abgelehnt werden (Pfad aus dem Ordner, Verweis, zu groß).

use std::io::Write as _;
use std::path::PathBuf;

use flate2::write::{DeflateEncoder, GzEncoder};

use super::*;

fn ziel() -> PathBuf {
    let pfad = std::env::temp_dir().join(format!(
        "quiver-entpacken-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dauer| dauer.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&pfad).expect("Ordner");
    pfad
}

/// Ein ZIP bauen: je Eintrag (Name, Inhalt, deflate?).
fn zip_bauen(eintraege: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut daten = Vec::new();
    let mut verzeichnis = Vec::new();
    let mut versatz = 0usize;
    for (name, inhalt, deflate) in eintraege {
        let roh = if *deflate {
            let mut bauer = DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            bauer.write_all(inhalt).expect("deflate");
            bauer.finish().expect("deflate")
        } else {
            inhalt.to_vec()
        };
        let methode: u16 = if *deflate { 8 } else { 0 };
        let kopf_anfang = daten.len();
        daten.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        daten.extend_from_slice(&20u16.to_le_bytes()); // nötige Version
        daten.extend_from_slice(&0u16.to_le_bytes()); // Kennzeichen
        daten.extend_from_slice(&methode.to_le_bytes());
        daten.extend_from_slice(&0u16.to_le_bytes()); // Zeit
        daten.extend_from_slice(&0u16.to_le_bytes()); // Datum
        daten.extend_from_slice(&0u32.to_le_bytes()); // CRC (nicht geprüft)
        daten.extend_from_slice(&(roh.len() as u32).to_le_bytes());
        daten.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
        daten.extend_from_slice(&(name.len() as u16).to_le_bytes());
        daten.extend_from_slice(&0u16.to_le_bytes()); // Extra
        daten.extend_from_slice(name.as_bytes());
        daten.extend_from_slice(&roh);

        verzeichnis.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        verzeichnis.extend_from_slice(&20u16.to_le_bytes()); // Version
        verzeichnis.extend_from_slice(&20u16.to_le_bytes()); // nötig
        verzeichnis.extend_from_slice(&0u16.to_le_bytes());
        verzeichnis.extend_from_slice(&methode.to_le_bytes());
        verzeichnis.extend_from_slice(&0u16.to_le_bytes());
        verzeichnis.extend_from_slice(&0u16.to_le_bytes());
        verzeichnis.extend_from_slice(&0u32.to_le_bytes()); // CRC
        verzeichnis.extend_from_slice(&(roh.len() as u32).to_le_bytes());
        verzeichnis.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
        verzeichnis.extend_from_slice(&(name.len() as u16).to_le_bytes());
        verzeichnis.extend_from_slice(&0u16.to_le_bytes()); // Extra
        verzeichnis.extend_from_slice(&0u16.to_le_bytes()); // Kommentar
        verzeichnis.extend_from_slice(&0u16.to_le_bytes()); // Datenträger
        verzeichnis.extend_from_slice(&0u16.to_le_bytes()); // intern
        verzeichnis.extend_from_slice(&0u32.to_le_bytes()); // extern
        verzeichnis.extend_from_slice(&(kopf_anfang as u32).to_le_bytes());
        verzeichnis.extend_from_slice(name.as_bytes());
        versatz = daten.len();
    }
    let verzeichnis_anfang = daten.len();
    daten.extend_from_slice(&verzeichnis);
    daten.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    daten.extend_from_slice(&0u16.to_le_bytes()); // Datenträger
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&(eintraege.len() as u16).to_le_bytes());
    daten.extend_from_slice(&(eintraege.len() as u16).to_le_bytes());
    let _ = versatz;
    daten.extend_from_slice(&(verzeichnis.len() as u32).to_le_bytes());
    daten.extend_from_slice(&(verzeichnis_anfang as u32).to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes()); // Kommentar
    daten
}

/// Ein tar mit einem Eintrag bauen (nur die Felder, die gelesen werden).
fn tar_bauen(name: &str, inhalt: &[u8]) -> Vec<u8> {
    let mut archiv = Vec::new();
    let mut kopf = [0u8; 512];
    kopf[..name.len()].copy_from_slice(name.as_bytes());
    let groesse = format!("{:07o}\0", inhalt.len());
    kopf[124..124 + groesse.len()].copy_from_slice(groesse.as_bytes());
    kopf[156] = b'0';
    kopf[257..262].copy_from_slice(b"ustar");
    archiv.extend_from_slice(&kopf);
    archiv.extend_from_slice(inhalt);
    let rest = (512 - inhalt.len() % 512) % 512;
    archiv.extend(std::iter::repeat_n(0u8, rest));
    archiv.extend(std::iter::repeat_n(0u8, 1024));
    archiv
}

fn tar_gz_bauen(name: &str, inhalt: &[u8]) -> Vec<u8> {
    let mut bauer = GzEncoder::new(Vec::new(), flate2::Compression::default());
    bauer.write_all(&tar_bauen(name, inhalt)).expect("tar");
    bauer.finish().expect("gzip")
}

#[test]
fn ein_zip_wird_entpackt_gespeichert_und_deflate() {
    let ordner = ziel();
    let bytes = zip_bauen(&[
        ("quiver-0.5.0/bin/quiver", b"programm", false),
        ("quiver-0.5.0/LIESMICH.txt", b"gespeichert", true),
    ]);
    let geschrieben = zip_entpacken(&bytes, &ordner).expect("entpackt");
    assert_eq!(geschrieben.len(), 2);
    assert_eq!(
        std::fs::read(ordner.join("quiver-0.5.0/bin/quiver")).expect("Datei"),
        b"programm"
    );
    assert_eq!(
        std::fs::read(ordner.join("quiver-0.5.0/LIESMICH.txt")).expect("Datei"),
        b"gespeichert"
    );
    // Das Programm wird gefunden, egal wie tief es liegt.
    assert_eq!(
        finde_datei(&ordner, "quiver"),
        Some(ordner.join("quiver-0.5.0/bin/quiver"))
    );
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_tar_gz_wird_entpackt() {
    let ordner = ziel();
    let bytes = tar_gz_bauen("quiver-0.5.0/bin/quiver", b"#!bin/sh\n");
    let geschrieben = tar_gz_entpacken(&bytes, &ordner).expect("entpackt");
    assert_eq!(geschrieben.len(), 1);
    assert_eq!(
        std::fs::read(ordner.join("quiver-0.5.0/bin/quiver")).expect("Datei"),
        b"#!bin/sh\n"
    );
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_pfad_aus_dem_ordner_wird_abgelehnt() {
    for name in [
        "../ausserhalb",
        "a/../../ausserhalb",
        "/absolut",
        "C:/windows/quiver.exe",
        "a/../b",
    ] {
        assert!(!pfad_ist_sicher(name), "{name} müsste abgelehnt werden");
    }
    assert!(pfad_ist_sicher("quiver-0.5.0/bin/quiver"));
    // Auch beim Entpacken selbst: nichts landet außerhalb.
    let ordner = ziel();
    let bytes = zip_bauen(&[("../ausserhalb", b"nein", false)]);
    assert!(zip_entpacken(&bytes, &ordner).is_err());
    assert!(!ordner.join("..").join("ausserhalb").exists());
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_kaputtes_paket_ist_ein_fehler_und_kein_absturz() {
    let ordner = ziel();
    for bytes in [b"kein zip".to_vec(), vec![0x50, 0x4b, 0x03, 0x04]] {
        assert!(zip_entpacken(&bytes, &ordner).is_err());
    }
    assert!(tar_gz_entpacken(b"kein gzip", &ordner).is_err());
    let _ = std::fs::remove_dir_all(&ordner);
}

#[test]
fn ein_zu_grosses_paket_wird_abgelehnt() {
    let ordner = ziel();
    // Viele Einträge: über der Grenze.
    let namen: Vec<String> = (0..EINTRAEGE_MAX + 1).map(|i| format!("f{i}")).collect();
    let eintraege: Vec<(&str, &[u8], bool)> = namen
        .iter()
        .map(|name| (name.as_str(), b"x".as_slice(), false))
        .collect();
    let bytes = zip_bauen(&eintraege);
    assert!(matches!(
        zip_entpacken(&bytes, &ordner),
        Err(EntpackFehler::ZuGross)
    ));
    let _ = std::fs::remove_dir_all(&ordner);
}
