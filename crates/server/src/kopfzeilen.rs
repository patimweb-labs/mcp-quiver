// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Sicherheits-Köpfe überall (Aufgabe 20): CSP ohne Inline, kein Einbetten,
//! kein MIME-Raten, kein Referrer, HSTS bei https, kein Zwischenspeicher für
//! `/auth` und `/api`.

use axum::extract::Request;
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// Die CSP für alle Antworten. Skripte und Stile nur aus eigener Quelle – die
/// Verwaltung liefert ihre Dateien selbst, die Anmeldeseiten brauchen beides
/// nicht.
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     img-src 'self' data:; font-src 'self'; connect-src 'self'; form-action 'self'; \
     frame-ancestors 'none'; base-uri 'none'";

/// HSTS nur, wenn der Server hinter https läuft.
pub const HSTS: &str = "max-age=31536000";

pub async fn sicherheits_kopfzeilen(ist_https: bool, req: Request, next: Next) -> Response {
    let pfad = req.uri().path().to_string();
    let mut antwort = next.run(req).await;
    let kopf = antwort.headers_mut();
    let setzen = |kopf: &mut header::HeaderMap, name: HeaderName, wert: &'static str| {
        if let Ok(wert) = HeaderValue::from_str(wert) {
            kopf.insert(name, wert);
        }
    };
    setzen(kopf, header::CONTENT_SECURITY_POLICY, CSP);
    setzen(kopf, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    setzen(kopf, header::REFERRER_POLICY, "no-referrer");
    setzen(kopf, header::X_FRAME_OPTIONS, "DENY");
    if ist_https {
        setzen(kopf, header::STRICT_TRANSPORT_SECURITY, HSTS);
    }
    if pfad.starts_with("/api") || pfad.starts_with("/auth") {
        setzen(kopf, header::CACHE_CONTROL, "no-store");
    }
    antwort
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    async fn hole(ist_https: bool, pfad: &str) -> Response {
        let app = Router::new()
            .route("/api/x", get(|| async { "ok" }))
            .route("/seite", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(
                move |req: Request, next: Next| sicherheits_kopfzeilen(ist_https, req, next),
            ));
        app.oneshot(
            HttpRequest::builder()
                .uri(pfad)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn kopfzeilen_sind_da() {
        let antwort = hole(true, "/api/x").await;
        assert_eq!(
            antwort
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .unwrap()
                .to_str()
                .unwrap(),
            CSP
        );
        assert_eq!(
            antwort
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .unwrap(),
            "nosniff"
        );
        assert_eq!(
            antwort.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
        assert_eq!(
            antwort
                .headers()
                .get(header::STRICT_TRANSPORT_SECURITY)
                .unwrap(),
            HSTS
        );
        assert_eq!(
            antwort.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert!(!CSP.contains("unsafe-inline"));
        assert!(CSP.contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn ohne_https_kein_hsts_und_seite_ohne_no_store() {
        let antwort = hole(false, "/seite").await;
        assert!(antwort
            .headers()
            .get(header::STRICT_TRANSPORT_SECURITY)
            .is_none());
        assert!(antwort.headers().get(header::CACHE_CONTROL).is_none());
    }
}
