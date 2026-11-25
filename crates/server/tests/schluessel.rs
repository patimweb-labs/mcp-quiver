// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahmetests für Aufgabe 23 am echten Programm: `quiver-server
//! entra-schluessel`, die Konfigurationsregeln (genau ein Ausweis) und der
//! Start mit Zertifikat.
//!
//! Alles auf Loopback mit freiem Port und temporärem Datenordner; keine
//! Firmendaten, kein echter Mandant. Hängen ist ein Fehlschlag (Zeitlimit).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const FRIST: Duration = Duration::from_secs(120);

struct Ausgabe {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn ordner(name: &str) -> PathBuf {
    static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
    let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let pfad = std::env::temp_dir().join(format!(
        "quiver-schluessel-{name}-{}-{nummer}",
        std::process::id()
    ));
    std::fs::create_dir_all(&pfad).expect("Ordner");
    pfad
}

fn befehl_bauen(args: &[&str], envs: &[(&str, &str)]) -> Command {
    let mut kind = Command::new(env!("CARGO_BIN_EXE_quiver-server"));
    kind.args(args)
        .env_remove("QUIVER_ENTRA_SECRET_DATEI")
        .env_remove("QUIVER_ENTRA_SCHLUESSEL_DATEI")
        .env_remove("QUIVER_ENTRA_AUTHORITY")
        .env_remove("QUIVER_ADMIN_TOKEN")
        .env("RUST_LOG", "warn")
        // Nie den Standardport 8740 berühren (der gehört dem Betrieb); ein
        // Test, der hier versehentlich startet, bindet nur an Loopback-Port 0.
        .env("QUIVER_ADRESSE", "127.0.0.1:0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, wert) in envs {
        kind.env(name, wert);
    }
    kind
}

fn ausfuehren(args: &[&str], envs: &[(&str, &str)]) -> Ausgabe {
    let mut kind = befehl_bauen(args, envs).spawn().expect("Programm startet");
    let ende = Instant::now() + FRIST;
    loop {
        if let Some(status) = kind.try_wait().expect("Status") {
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut rohr) = kind.stdout.take() {
                let _ = rohr.read_to_string(&mut stdout);
            }
            if let Some(mut rohr) = kind.stderr.take() {
                let _ = rohr.read_to_string(&mut stderr);
            }
            return Ausgabe {
                status,
                stdout,
                stderr,
            };
        }
        if Instant::now() > ende {
            let _ = kind.kill();
            panic!("Zeitlimit: das Programm hängt seit {} s", FRIST.as_secs());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn zeile(text: &str, anfang: &str) -> String {
    text.lines()
        .find_map(|zeile| zeile.strip_prefix(anfang))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn string(pfad: &Path) -> String {
    pfad.to_string_lossy().to_string()
}

/// Ein freier Port, nie 8740–8742 (die gehören dem Betrieb).
fn freier_port() -> u16 {
    loop {
        let probe = TcpListener::bind("127.0.0.1:0").expect("Port frei");
        let port = probe.local_addr().expect("Adresse").port();
        drop(probe);
        if !(8740..=8742).contains(&port) {
            return port;
        }
    }
}

fn http_ok(port: u16) -> bool {
    let Ok(mut strom) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = strom.set_read_timeout(Some(Duration::from_secs(5)));
    write!(
        strom,
        "GET /auth/stand HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\n\
         Connection: close\r\n\r\n"
    )
    .expect("Anfrage");
    let mut antwort = String::new();
    let _ = strom.read_to_string(&mut antwort);
    antwort.starts_with("HTTP/1.1 200")
}

fn server_stoppen(mut kind: Child) -> (String, String) {
    let _ = kind.kill();
    let _ = kind.wait();
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut rohr) = kind.stdout.take() {
        let _ = rohr.read_to_string(&mut stdout);
    }
    if let Some(mut rohr) = kind.stderr.take() {
        let _ = rohr.read_to_string(&mut stderr);
    }
    (stdout, stderr)
}

#[test]
fn entra_schluessel_gibt_nur_das_zertifikat_aus_und_ueberschreibt_nichts() {
    let daten = ordner("cli");
    let daten_text = string(&daten);
    let umgebung = [("QUIVER_DATEN", daten_text.as_str())];
    let schluessel = daten.join("entra-schluessel.pem");
    let zertifikat = daten.join("entra-schluessel.crt.pem");

    let erst = ausfuehren(&["entra-schluessel"], &umgebung);
    assert!(erst.status.success(), "{}", erst.stderr);
    assert!(erst.stdout.contains("BEGIN CERTIFICATE"), "{}", erst.stdout);
    assert!(erst.stdout.contains("Fingerabdruck (SHA-256):"));
    assert!(erst.stdout.contains("Gültig bis:"));
    assert_eq!(zeile(&erst.stdout, "x5t#S256:").len(), 43);
    // Nur das Öffentliche: kein privater Schlüssel in der Ausgabe.
    assert!(!erst.stdout.contains("PRIVATE KEY"), "{}", erst.stdout);
    assert!(!erst.stderr.contains("PRIVATE KEY"));
    assert!(schluessel.exists());
    assert!(zertifikat.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&schluessel)
            .expect("Datei")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "Rechte des Schlüssels: {mode:o}");
    }
    let vorher = std::fs::read_to_string(&schluessel).expect("lesbar");
    let x5t_erst = zeile(&erst.stdout, "x5t#S256:");

    // Ein zweiter Aufruf ändert nichts (nie überschreiben).
    let zweit = ausfuehren(&["entra-schluessel"], &umgebung);
    assert!(zweit.status.success(), "{}", zweit.stderr);
    assert!(
        zweit.stdout.contains("nichts wurde geändert"),
        "{}",
        zweit.stdout
    );
    assert_eq!(std::fs::read_to_string(&schluessel).unwrap(), vorher);

    // --erneuern legt einen zweiten an, ohne den aktiven anzufassen.
    let erneuert = ausfuehren(&["entra-schluessel", "--erneuern"], &umgebung);
    assert!(erneuert.status.success(), "{}", erneuert.stderr);
    assert!(
        erneuert.stdout.contains("noch nicht aktiv"),
        "{}",
        erneuert.stdout
    );
    let zweiter = daten.join("entra-schluessel-1.pem");
    assert!(zweiter.exists(), "zweiter Schlüssel fehlt");
    assert_eq!(
        std::fs::read_to_string(&schluessel).unwrap(),
        vorher,
        "der aktive Schlüssel bleibt unberührt"
    );
    assert_ne!(
        zeile(&erneuert.stdout, "x5t#S256:"),
        x5t_erst,
        "der neue Schlüssel ist ein anderer"
    );

    // --umschalten macht ihn aktiv, ohne Ausfall und ohne Überschreiben.
    let gewechselt = ausfuehren(&["entra-schluessel", "--umschalten"], &umgebung);
    assert!(gewechselt.status.success(), "{}", gewechselt.stderr);
    assert!(
        gewechselt.stdout.contains("jetzt aktiv"),
        "{}",
        gewechselt.stdout
    );
    assert_ne!(zeile(&gewechselt.stdout, "x5t#S256:"), x5t_erst);
    // Der alte Schlüssel liegt weiter auf einem Platz, nichts ist verloren.
    let plaetze: Vec<PathBuf> = std::fs::read_dir(&daten)
        .expect("Ordner")
        .flatten()
        .map(|eintrag| eintrag.path())
        .filter(|pfad| {
            pfad.file_name()
                .map(|name| name.to_string_lossy().starts_with("entra-schluessel-"))
                .unwrap_or(false)
        })
        .collect();
    assert!(!plaetze.is_empty(), "der alte Schlüssel ist verschwunden");

    // Beides gleichzeitig ist ein klarer Fehler.
    let beides = ausfuehren(
        &["entra-schluessel", "--erneuern", "--umschalten"],
        &umgebung,
    );
    assert!(!beides.status.success());
    assert!(beides.stderr.contains("nicht beides"), "{}", beides.stderr);

    let _ = std::fs::remove_dir_all(&daten);
}

#[test]
fn schluesselpfad_kommt_aus_der_umgebung() {
    let daten = ordner("pfad");
    let eigener = daten.join("mein-schluessel.pem");
    let daten_text = string(&daten);
    let eigener_text = string(&eigener);
    let umgebung = [
        ("QUIVER_DATEN", daten_text.as_str()),
        ("QUIVER_ENTRA_SCHLUESSEL_DATEI", eigener_text.as_str()),
    ];
    let lauf = ausfuehren(&["entra-schluessel"], &umgebung);
    assert!(lauf.status.success(), "{}", lauf.stderr);
    assert!(eigener.exists());
    assert!(daten.join("mein-schluessel.crt.pem").exists());
    assert!(
        !daten.join("entra-schluessel.pem").exists(),
        "der Standardpfad bleibt unberührt"
    );
    let _ = std::fs::remove_dir_all(&daten);
}

#[test]
fn beide_ausweise_oder_keiner_starten_nicht() {
    // Geheimnis und Schlüssel zugleich: Start scheitert mit klarer Meldung.
    let daten = ordner("beides");
    std::fs::write(daten.join("entra-secret"), "kein-echtes-geheimnis").expect("Geheimnis");
    let daten_text = string(&daten);
    let erzeugt = ausfuehren(
        &["entra-schluessel"],
        &[("QUIVER_DATEN", daten_text.as_str())],
    );
    assert!(erzeugt.status.success(), "{}", erzeugt.stderr);
    let lauf = ausfuehren(
        &[],
        &[
            ("QUIVER_DATEN", daten_text.as_str()),
            ("QUIVER_ENTRA_TENANT", TENANT),
            ("QUIVER_ENTRA_CLIENT_ID", CLIENT),
            ("QUIVER_ADRESSE", "127.0.0.1:0"),
        ],
    );
    assert!(!lauf.status.success(), "beides zusammen startet nicht");
    assert!(lauf.stderr.contains("genau eins"), "{}", lauf.stderr);

    // Nur der Mandant: teilweise gesetzt.
    let teil = ordner("teil");
    let teil_text = string(&teil);
    let lauf = ausfuehren(
        &[],
        &[
            ("QUIVER_DATEN", teil_text.as_str()),
            ("QUIVER_ENTRA_TENANT", TENANT),
        ],
    );
    assert!(!lauf.status.success());
    assert!(lauf.stderr.contains("nur teilweise"), "{}", lauf.stderr);

    // Mandant und Client, aber kein Ausweis.
    let ohne = ordner("ohne");
    let ohne_text = string(&ohne);
    let lauf = ausfuehren(
        &[],
        &[
            ("QUIVER_DATEN", ohne_text.as_str()),
            ("QUIVER_ENTRA_TENANT", TENANT),
            ("QUIVER_ENTRA_CLIENT_ID", CLIENT),
        ],
    );
    assert!(!lauf.status.success());
    assert!(lauf.stderr.contains("nur teilweise"), "{}", lauf.stderr);

    // Ein Geheimnis ohne Entra-Angaben: auch teilweise.
    let geheim = ordner("geheim");
    std::fs::write(geheim.join("entra-secret"), "kein-echtes-geheimnis").expect("Geheimnis");
    let geheim_text = string(&geheim);
    let lauf = ausfuehren(&[], &[("QUIVER_DATEN", geheim_text.as_str())]);
    assert!(!lauf.status.success());
    assert!(lauf.stderr.contains("nur teilweise"), "{}", lauf.stderr);

    let _ = std::fs::remove_dir_all(&daten);
    let _ = std::fs::remove_dir_all(&teil);
    let _ = std::fs::remove_dir_all(&ohne);
    let _ = std::fs::remove_dir_all(&geheim);
}

#[test]
fn server_startet_mit_zertifikat_aus_der_umgebung() {
    let daten = ordner("start");
    let daten_text = string(&daten);
    let erzeugt = ausfuehren(
        &["entra-schluessel"],
        &[("QUIVER_DATEN", daten_text.as_str())],
    );
    assert!(erzeugt.status.success(), "{}", erzeugt.stderr);

    let port = freier_port();
    let basis = format!("http://127.0.0.1:{port}");
    let umgebung = [
        ("QUIVER_DATEN".to_string(), string(&daten)),
        ("QUIVER_ADRESSE".to_string(), format!("127.0.0.1:{port}")),
        ("QUIVER_OEFFENTLICH".to_string(), basis.clone()),
        ("QUIVER_ENTRA_TENANT".to_string(), TENANT.to_string()),
        ("QUIVER_ENTRA_CLIENT_ID".to_string(), CLIENT.to_string()),
    ];
    let paare: Vec<(&str, &str)> = umgebung
        .iter()
        .map(|(name, wert)| (name.as_str(), wert.as_str()))
        .collect();
    let mut kind = befehl_bauen(&[], &paare).spawn().expect("Server startet");
    let ende = Instant::now() + Duration::from_secs(30);
    let mut bereit = false;
    while Instant::now() < ende {
        if http_ok(port) {
            bereit = true;
            break;
        }
        if kind.try_wait().ok().flatten().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (stdout, stderr) = server_stoppen(kind);
    assert!(
        bereit,
        "Server mit Zertifikat antwortet nicht auf /auth/stand:\n{stdout}\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&daten);
}
