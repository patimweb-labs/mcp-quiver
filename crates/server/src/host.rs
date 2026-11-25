// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der `Host`-Kopf muss zur Basis-URL passen – bei **jeder** Antwort.
//!
//! Der Grund ist die Auflösung von Namen: wer den Server über einen fremden
//! Namen anspricht, soll nichts bekommen, auch nicht die Startseite und nicht
//! die Geräte-API. Geprüft wird, bevor eine Ansicht rechnet.
//!
//! Die einzige Ausnahme ist der Gesundheitsweg [`GESUND`] **über Loopback**:
//! den ruft der Container selbst auf (`quiver-server gesund`), und dort ist der
//! Name nicht die öffentliche Adresse.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::einrichtung::host_passt;
use crate::zustand::Zustand;

/// Der interne Gesundheitsweg: sagt nur, dass der Server antwortet.
pub const GESUND: &str = "/gesund";

/// Kommt die Anfrage über Loopback? Ohne Verbindungsdaten (Tests, eingebettete
/// Aufrufe) gilt: nein.
pub(crate) fn von_loopback(req: &Request) -> bool {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().is_loopback())
        .unwrap_or(false)
}

/// `GET /gesund`: 200, wenn der Server läuft – aber nur über Loopback.
pub(crate) async fn gesund(req: Request) -> Response {
    if !von_loopback(&req) {
        // Nach außen gibt es diesen Weg nicht.
        return StatusCode::NOT_FOUND.into_response();
    }
    (StatusCode::OK, "ok\n").into_response()
}

/// Die Wache: passt `Host` nicht zur Basis-URL, gibt es 421 ohne Inhalt.
pub(crate) async fn host_wache(
    State(zustand): State<Arc<Zustand>>,
    req: Request,
    next: Next,
) -> Response {
    if req.uri().path() == GESUND && von_loopback(&req) {
        return next.run(req).await;
    }
    let passt = req
        .headers()
        .get(header::HOST)
        .and_then(|wert| wert.to_str().ok())
        .is_some_and(|host| host_passt(host, &zustand.konfig.oeffentlich));
    if !passt {
        // 421: an dieser Stelle bin ich nicht gemeint. Ohne Rumpf, ohne Hinweis.
        tracing::debug!("Host passt nicht zur Basis-URL; 421");
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn test_zustand(oeffentlich: &str) -> Arc<Zustand> {
        let mut konfig = crate::Konfig::neu(std::env::temp_dir().join("quiver-host-test"));
        konfig.oeffentlich = oeffentlich.to_string();
        Arc::new(Zustand::neu(
            rusqlite::Connection::open_in_memory().expect("Speicher"),
            konfig,
            crate::Schluessel::aus_bytes(&[3u8; 32]).expect("Schlüssel"),
            None,
            None,
            String::new(),
        ))
    }

    fn app(oeffentlich: &'static str) -> Router {
        Router::new()
            .route("/seite", get(|| async { "ok" }))
            .route(GESUND, get(gesund))
            .layer(axum::middleware::from_fn_with_state(
                test_zustand(oeffentlich),
                host_wache,
            ))
    }

    async fn hole(router: Router, pfad: &str, host: Option<&str>) -> Response {
        let mut bauer = HttpRequest::builder().uri(pfad);
        if let Some(host) = host {
            bauer = bauer.header(header::HOST, host);
        }
        router
            .oneshot(bauer.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn fremder_host_bekommt_421_ohne_inhalt() {
        let antwort = hole(app("http://127.0.0.1:8740"), "/seite", Some("evil.test")).await;
        assert_eq!(antwort.status(), StatusCode::MISDIRECTED_REQUEST);
        let rumpf = axum::body::to_bytes(antwort.into_body(), 1024)
            .await
            .expect("Rumpf");
        assert!(rumpf.is_empty(), "421 antwortet ohne Inhalt");
    }

    #[tokio::test]
    async fn fehlender_host_bekommt_421() {
        let antwort = hole(app("http://127.0.0.1:8740"), "/seite", None).await;
        assert_eq!(antwort.status(), StatusCode::MISDIRECTED_REQUEST);
    }

    #[tokio::test]
    async fn passender_host_darf_durch() {
        let antwort = hole(
            app("http://127.0.0.1:8740"),
            "/seite",
            Some("127.0.0.1:8740"),
        )
        .await;
        assert_eq!(antwort.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn gesundheitsweg_ohne_loopback_ist_nicht_oeffentlich() {
        // Ohne Verbindungsdaten gilt „nicht Loopback“ – und dann greift die
        // Host-Prüfung: der Weg ist nach außen nicht erreichbar.
        let antwort = hole(app("http://127.0.0.1:8740"), GESUND, Some("evil.test")).await;
        assert_eq!(antwort.status(), StatusCode::MISDIRECTED_REQUEST);
        let antwort = hole(app("http://127.0.0.1:8740"), GESUND, Some("127.0.0.1:8740")).await;
        assert_eq!(
            antwort.status(),
            StatusCode::NOT_FOUND,
            "ohne Loopback gibt es den Gesundheitsweg nicht"
        );
    }
}
