// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahmetest für das Beenden mit einem **echten** Prozess (Härtung, Punkt 16).
//!
//! Im Container ist der Server PID 1; dort beachtet er SIGTERM nur, wenn er es
//! selbst behandelt. `docker stop` schickt SIGTERM – wird es ignoriert, endet
//! der Container nach der Frist hart mit Exit 137. Hier wird deshalb das
//! gebaute Programm gestartet, bis es antwortet, dann SIGTERM geschickt und der
//! Exit-Code geprüft.

#![cfg(unix)]

use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Ein freier Port auf Loopback (kurz belegt, dann freigegeben).
fn freier_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Port frei");
    listener.local_addr().expect("Adresse").port()
}

fn ordner() -> PathBuf {
    static ZAEHLER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let ordner =
        std::env::temp_dir().join(format!("quiver-ende-test-{}-{nummer}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ordner);
    std::fs::create_dir_all(&ordner).expect("Ordner");
    ordner
}

fn warte_auf_bereitschaft(adresse: SocketAddr) -> bool {
    let ende = Instant::now() + Duration::from_secs(20);
    while Instant::now() < ende {
        if std::net::TcpStream::connect_timeout(&adresse, Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn sigterm(child: &Child) {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .expect("kill lässt sich starten");
    assert!(status.success(), "kill -TERM ist gelungen");
}

/// Wartet auf das Ende und räumt zur Not selbst auf.
fn ende_mit_code(mut child: Child) -> Option<i32> {
    let ende = Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code(),
            Ok(None) if Instant::now() < ende => std::thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => return None,
        }
    }
}

#[test]
fn sigterm_beendet_mit_null_und_gibt_die_datenbank_frei() {
    let ordner = ordner();
    let port = freier_port();
    let adresse: SocketAddr = format!("127.0.0.1:{port}").parse().expect("Adresse");
    let mut child = Command::new(env!("CARGO_BIN_EXE_quiver-server"))
        .env("QUIVER_DATEN", &ordner)
        .env("QUIVER_ADRESSE", adresse.to_string())
        .env("QUIVER_ADMIN_DIST", ordner.join("kein-dist"))
        .env_remove("QUIVER_OEFFENTLICH")
        .env_remove("QUIVER_EINRICHTUNG")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("quiver-server startet");

    if !warte_auf_bereitschaft(adresse) {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&ordner);
        panic!("der Server wurde nicht bereit");
    }

    sigterm(&child);
    let code = ende_mit_code(child);
    let datenbank_frei = !ordner.join("quiver.db-wal").exists()
        || std::fs::metadata(ordner.join("quiver.db-wal"))
            .map(|meta| meta.len() == 0)
            .unwrap_or(true);
    let _ = std::fs::remove_dir_all(&ordner);

    assert_eq!(code, Some(0), "SIGTERM endet mit 0, nicht hart mit 137");
    assert!(
        datenbank_frei,
        "das WAL ist beim Ende zurückgeschrieben (leer oder weg)"
    );
}
