// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahmetests für den Betrieb über echte Verbindungen: `Host`-Prüfung für
//! **alle** Antworten, der Gesundheitsweg über Loopback und das Zeitlimit für
//! den Rumpf.
//!
//! Hier läuft der Server wirklich auf Loopback (Port 0) mit dem eigenen
//! Verbindungsweg [`quiver_server::betrieb`]; die Anfragen sind rohes HTTP,
//! damit auch ein Client geprüft werden kann, der den Kopf verfälscht oder
//! langsam sendet.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use quiver_server::{app, betrieb, Konfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Die Basis-URL, die der Server für sich annimmt.
const OEFFENTLICH: &str = "http://127.0.0.1:8740";
/// Der `Host`, der dazu passt (der Port gehört dazu).
const HOST: &str = "127.0.0.1:8740";

struct Betrieb {
    ordner: PathBuf,
    adresse: SocketAddr,
    aus: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Betrieb {
    /// Server mit gebauter Oberfläche (damit es eine Startseite gibt).
    async fn neu(rumpf_zeit: Duration) -> Self {
        static ZAEHLER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let ordner = std::env::temp_dir().join(format!(
            "quiver-betrieb-{}-{}",
            std::process::id(),
            ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&ordner);
        let dist = ordner.join("dist");
        std::fs::create_dir_all(&dist).expect("Ordner");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>quiver</title><p>Verwaltung</p>",
        )
        .expect("index.html");

        let mut konfig = Konfig::neu(&ordner).mit_admin_dist(Some(dist));
        konfig.oeffentlich = OEFFENTLICH.to_string();
        konfig.rumpf_zeit = rumpf_zeit;
        let router = app(konfig).await.expect("App baut");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Port frei");
        let adresse = listener.local_addr().expect("Adresse");
        let (aus, ein) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = betrieb::bedienen(listener, router, async {
                let _ein = ein.await;
            })
            .await;
        });
        Self {
            ordner,
            adresse,
            aus: Some(aus),
        }
    }

    /// Rohes HTTP senden und die ganze Antwort lesen (mit `Connection: close`).
    async fn roh(&self, anfrage: &str) -> (u16, String) {
        let mut strom = tokio::net::TcpStream::connect(self.adresse)
            .await
            .expect("verbunden");
        strom.write_all(anfrage.as_bytes()).await.expect("gesendet");
        let mut antwort = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), strom.read_to_end(&mut antwort))
            .await
            .expect("Antwort kommt")
            .expect("lesbar");
        auspacken(&String::from_utf8_lossy(&antwort))
    }

    async fn hole(&self, pfad: &str, host: &str) -> (u16, String) {
        self.roh(&format!(
            "GET {pfad} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        ))
        .await
    }
}

impl Drop for Betrieb {
    fn drop(&mut self) {
        if let Some(aus) = self.aus.take() {
            let _ = aus.send(());
        }
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

/// Status und Rumpf aus einer rohen Antwort.
fn auspacken(roh: &str) -> (u16, String) {
    let (kopf, rumpf) = match roh.split_once("\r\n\r\n") {
        Some((kopf, rumpf)) => (kopf, rumpf),
        None => (roh, ""),
    };
    let status = kopf
        .lines()
        .next()
        .and_then(|zeile| zeile.split_whitespace().nth(1))
        .and_then(|zahl| zahl.parse().ok())
        .unwrap_or(0);
    (status, rumpf.to_string())
}

#[tokio::test]
async fn fremder_host_bekommt_421_ohne_inhalt() {
    let p = Betrieb::neu(Duration::from_secs(5)).await;

    // Die Startseite (die Oberfläche) – nicht nur /auth.
    let (status, rumpf) = p.hole("/", "evil.example.test").await;
    assert_eq!(status, 421, "Startseite: {rumpf}");
    assert!(rumpf.is_empty(), "421 antwortet ohne Inhalt");

    // Die Geräte-API.
    let (status, rumpf) = p.hole("/api/v1/geraet/soll", "evil.example.test").await;
    assert_eq!(status, 421, "Geräte-API: {rumpf}");
    assert!(rumpf.is_empty());

    // Ein Host ohne Port passt nicht zur Basis-URL mit Port.
    let (status, _) = p.hole("/", "127.0.0.1").await;
    assert_eq!(status, 421);

    // Mit passendem Host ist alles wie immer.
    let (status, rumpf) = p.hole("/", HOST).await;
    assert_eq!(status, 200, "{rumpf}");
    assert!(rumpf.contains("Verwaltung"), "die Oberfläche kommt an");
    let (status, _) = p.hole("/api/v1/geraet/soll", HOST).await;
    assert_eq!(status, 401, "ohne Token wie bisher");
}

#[tokio::test]
async fn gesundheitsweg_antwortet_nur_ueber_loopback() {
    let p = Betrieb::neu(Duration::from_secs(5)).await;
    // Der Name im Kopf ist hier egal: der Weg ist die Ausnahme, weil er über
    // Loopback kommt.
    let (status, rumpf) = p.hole(quiver_server::GESUND, "quiver").await;
    assert_eq!(status, 200);
    assert_eq!(rumpf.trim(), "ok");
}

#[tokio::test]
async fn langsamer_rumpf_laeuft_in_das_zeitlimit() {
    let p = Betrieb::neu(Duration::from_millis(200)).await;
    // Kopfzeilen sind da, der Rumpf kommt nie: nach dem Zeitlimit 408.
    let (status, _) = p
        .roh(&format!(
            "POST /api/v1/anmelden HTTP/1.1\r\nHost: {HOST}\r\nContent-Type: application/json\r\n\
             Content-Length: 40\r\nConnection: close\r\n\r\n"
        ))
        .await;
    assert_eq!(status, 408, "langsamer Client bekommt 408");
}
