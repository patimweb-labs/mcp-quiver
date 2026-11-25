// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Betrieb: Verbindungen annehmen, begrenzen und die Zeit im Auge behalten.
//!
//! Zwei Grenzen, die `axum::serve` nicht kennt:
//!
//! - **Kopfzeilen** müssen in [`KOPF_ZEIT`] da sein. Ein Client, der nach dem
//!   Verbinden schweigt, belegt sonst dauerhaft einen Platz (langsame Clients).
//! - **Verbindungen** sind begrenzt ([`VERBINDUNGEN`]). Ist kein Platz frei,
//!   wird die neue Verbindung sofort geschlossen; warten würde nur den Stau
//!   verlängern.
//!
//! Der Rumpf hat sein eigenes Zeitlimit (`Konfig::rumpf_zeit`, als Schicht auf
//! dem Router) – es gilt je Anfrage, nicht je Verbindung, weil eine Verbindung
//! mehrere Anfragen tragen kann.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ConnectInfo;
use axum::Router;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

/// Zeitlimit für die Kopfzeilen einer Anfrage.
pub const KOPF_ZEIT: Duration = Duration::from_secs(10);
/// So viele Verbindungen dürfen gleichzeitig offen sein.
pub const VERBINDUNGEN: usize = 256;
/// So lange dürfen offene Anfragen beim Herunterfahren noch laufen.
pub const ENDE_ZEIT: Duration = Duration::from_secs(5);

/// Die Grenzen des Betriebs. In Tests kleiner, im Betrieb die Vorgaben.
#[derive(Debug, Clone, Copy)]
pub struct Grenzen {
    /// Zeitlimit für die Kopfzeilen einer Anfrage.
    pub kopf_zeit: Duration,
    /// Wie viele Verbindungen gleichzeitig offen sein dürfen.
    pub verbindungen: usize,
}

impl Default for Grenzen {
    fn default() -> Self {
        Self {
            kopf_zeit: KOPF_ZEIT,
            verbindungen: VERBINDUNGEN,
        }
    }
}

/// Den Router bedienen, bis `herunterfahren` fertig ist.
pub async fn bedienen(
    listener: TcpListener,
    router: Router,
    herunterfahren: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    bedienen_mit(listener, router, Grenzen::default(), herunterfahren).await
}

/// Wie [`bedienen`], mit eigenen Grenzen (für Tests).
pub async fn bedienen_mit(
    listener: TcpListener,
    router: Router,
    grenzen: Grenzen,
    herunterfahren: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    let plaetze = Arc::new(Semaphore::new(grenzen.verbindungen));
    let mut warten = std::pin::pin!(herunterfahren);
    loop {
        tokio::select! {
            _ = &mut warten => break,
            angenommen = listener.accept() => {
                let (strom, adresse) = angenommen?;
                let Ok(platz) = plaetze.clone().try_acquire_owned() else {
                    tracing::warn!(
                        "{} Verbindungen offen; neue Verbindung wird geschlossen",
                        grenzen.verbindungen
                    );
                    drop(strom);
                    continue;
                };
                // Die Adresse des Aufrufers gehört in die Anfrage (Drossel,
                // Gesundheitsweg): je Verbindung eine eigene Schicht.
                let dienst = router.clone().layer(axum::Extension(ConnectInfo(adresse)));
                tokio::spawn(async move {
                    let _platz = platz;
                    let mut bauer = http1::Builder::new();
                    // Ohne Zeitgeber gibt es kein `header_read_timeout`.
                    bauer.timer(TokioTimer::new());
                    bauer.header_read_timeout(grenzen.kopf_zeit);
                    let verbindung = bauer.serve_connection(
                        TokioIo::new(strom),
                        TowerToHyperService::new(dienst),
                    );
                    if let Err(fehler) = verbindung.await {
                        tracing::debug!("Verbindung von {adresse} beendet: {fehler}");
                    }
                });
            }
        }
    }
    // Kein neuer Zugang mehr (der Lauscher fällt am Ende weg). Die
    // Ereignis-Ströme hat der Aufrufer schon beendet; offene Anfragen bekommen
    // noch [`ENDE_ZEIT`] Platz zum Auslaufen.
    tracing::info!("beende: offene Anfragen laufen höchstens {ENDE_ZEIT:?} aus");
    let alle = u32::try_from(grenzen.verbindungen).unwrap_or(u32::MAX);
    if tokio::time::timeout(ENDE_ZEIT, plaetze.acquire_many(alle))
        .await
        .is_err()
    {
        tracing::warn!("offene Anfragen nach {ENDE_ZEIT:?} nicht fertig; Ende trotzdem");
    }
    Ok(())
}

/// Die Adresse, unter der der Gesundheitsweg zu erreichen ist.
///
/// Nur Loopback, nie die öffentliche Adresse: der Weg ist für den Container
/// selbst gedacht.
pub fn gesundheits_adresse(adresse: SocketAddr) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], adresse.port()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Ein Server auf Port 0; gibt die Adresse und den Abschalter zurück.
    async fn starte(
        router: Router,
        grenzen: Grenzen,
    ) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("Port frei");
        let adresse = listener.local_addr().expect("Adresse");
        let (aus, ein) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = bedienen_mit(listener, router, grenzen, async {
                let _ein = ein.await;
            })
            .await;
        });
        (adresse, aus)
    }

    #[tokio::test]
    async fn antwortet_und_kennt_die_aufrufer_adresse() {
        let router = Router::new().route(
            "/wo",
            get(|req: axum::extract::Request| async move {
                let peer = req
                    .extensions()
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|info| info.0);
                match peer {
                    Some(adresse) if adresse.ip().is_loopback() => "loopback",
                    _ => "fremd",
                }
            }),
        );
        let (adresse, aus) = starte(router, Grenzen::default()).await;
        let mut strom = tokio::net::TcpStream::connect(adresse)
            .await
            .expect("verbunden");
        strom
            .write_all(format!("GET /wo HTTP/1.1\r\nHost: {adresse}\r\n\r\n").as_bytes())
            .await
            .expect("gesendet");
        let mut antwort = vec![0u8; 1024];
        let gelesen = strom.read(&mut antwort).await.expect("gelesen");
        let text = String::from_utf8_lossy(&antwort[..gelesen]).to_string();
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(text.ends_with("loopback"), "{text}");
        let _ = aus.send(());
    }

    #[tokio::test]
    async fn schweigender_client_wird_nach_der_kopfzeit_entlassen() {
        let grenzen = Grenzen {
            kopf_zeit: Duration::from_millis(200),
            ..Grenzen::default()
        };
        let (adresse, aus) = starte(Router::new(), grenzen).await;
        let mut strom = tokio::net::TcpStream::connect(adresse)
            .await
            .expect("verbunden");
        // Nichts senden: nach der Kopfzeit muss die Verbindung zu sein.
        let mut antwort = Vec::new();
        let gelesen = tokio::time::timeout(Duration::from_secs(5), strom.read_to_end(&mut antwort))
            .await
            .expect("Verbindung endet von selbst")
            .expect("lesbar");
        assert_eq!(gelesen, 0, "der Server schickt nichts mehr");
        let _ = aus.send(());
    }

    #[tokio::test]
    async fn ohne_freien_platz_wird_die_verbindung_geschlossen() {
        let grenzen = Grenzen {
            verbindungen: 1,
            ..Grenzen::default()
        };
        let (adresse, aus) =
            starte(Router::new().route("/x", get(|| async { "ok" })), grenzen).await;
        // Die erste Verbindung bleibt offen (kein Request, kein Abschluss).
        let _erste = tokio::net::TcpStream::connect(adresse)
            .await
            .expect("verbunden");
        // Warten, bis der Server die erste angenommen hat.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut zweite = tokio::net::TcpStream::connect(adresse)
            .await
            .expect("verbunden");
        let mut antwort = Vec::new();
        let gelesen =
            tokio::time::timeout(Duration::from_secs(2), zweite.read_to_end(&mut antwort))
                .await
                .expect("zweite Verbindung endet sofort")
                .expect("lesbar");
        assert_eq!(gelesen, 0, "kein Platz: die Verbindung wird geschlossen");
        let _ = aus.send(());
    }

    #[test]
    fn gesundheit_laeuft_ueber_loopback() {
        let adresse: SocketAddr = "0.0.0.0:8740".parse().unwrap();
        assert_eq!(
            gesundheits_adresse(adresse),
            "127.0.0.1:8740".parse::<SocketAddr>().unwrap()
        );
    }
}
