// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Sicherheits-Köpfe überall (Aufgabe 20): CSP ohne Inline, kein Einbetten,
//! kein MIME-Raten, HSTS bei https, kein Zwischenspeicher für `/auth` und
//! `/api`.
//!
//! `Referrer-Policy: same-origin` (Aufgabe 26, Punkt 2): mit `no-referrer`
//! schickt der Browser bei einem Formular `Origin: null`, und die Bestätigung
//! scheitert an der Origin-Prüfung. `same-origin` schickt den Referrer nur an
//! uns selbst – nach außen bleibt es bei „nichts“.

use axum::extract::Request;
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// Die CSP für alle Antworten. Skripte und Stile nur aus eigener Quelle – die
/// Verwaltung liefert ihre Dateien selbst, die Seiten des Servers holen ihr
/// Stylesheet unter `/auth/stil.css` (Aufgabe 26, Punkt 5).
///
/// Eine Ansicht darf die CSP dieser Antwort ersetzen (die Bestätigungsseite
/// braucht `form-action` für genau den Port des Vorgangs, Aufgabe 26, Punkt 3):
/// steht schon eine CSP in der Antwort, bleibt sie stehen.
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     img-src 'self' data:; font-src 'self'; connect-src 'self'; form-action 'self'; \
     frame-ancestors 'none'; base-uri 'none'";

/// HSTS nur, wenn der Server hinter https läuft.
pub const HSTS: &str = "max-age=31536000";

/// Kein Fenster von außen ansprechen (`Cross-Origin-Opener-Policy`).
pub const COOP: &str = "same-origin";
/// Keine fremde Seite bindet unsere Antworten ein (`Cross-Origin-Resource-Policy`).
pub const CORP: &str = "same-origin";
/// Namen, die `http` nicht als Konstante kennt.
pub const HEADER_COOP: HeaderName = HeaderName::from_static("cross-origin-opener-policy");
/// Keine fremde Seite bindet unsere Antworten ein.
pub const HEADER_CORP: HeaderName = HeaderName::from_static("cross-origin-resource-policy");
/// Alle Geräte-Schnittstellen des Browsers aus; die Oberfläche braucht keine.
pub const PERMISSIONS: &str = "accelerometer=(), autoplay=(), camera=(), \
     display-capture=(), encrypted-media=(), fullscreen=(), geolocation=(), \
     gyroscope=(), magnetometer=(), microphone=(), midi=(), payment=(), \
     picture-in-picture=(), publickey-credentials-get=(), screen-wake-lock=(), \
     serial=(), usb=(), xr-spatial-tracking=()";

pub async fn sicherheits_kopfzeilen(ist_https: bool, req: Request, next: Next) -> Response {
    let pfad = req.uri().path().to_string();
    let mut antwort = next.run(req).await;
    // Hat die Ansicht selbst eine CSP gesetzt, gilt ihre (genauere).
    let eigene_csp = antwort
        .headers()
        .contains_key(header::CONTENT_SECURITY_POLICY);
    let kopf = antwort.headers_mut();
    let setzen = |kopf: &mut header::HeaderMap, name: HeaderName, wert: &'static str| {
        if let Ok(wert) = HeaderValue::from_str(wert) {
            kopf.insert(name, wert);
        }
    };
    if !eigene_csp {
        setzen(kopf, header::CONTENT_SECURITY_POLICY, CSP);
    }
    setzen(kopf, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    setzen(kopf, header::REFERRER_POLICY, "same-origin");
    setzen(kopf, header::X_FRAME_OPTIONS, "DENY");
    setzen(kopf, HEADER_COOP.clone(), COOP);
    setzen(kopf, HEADER_CORP.clone(), CORP);
    setzen(
        kopf,
        header::HeaderName::from_static("permissions-policy"),
        PERMISSIONS,
    );
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

    /// Die CSP der Bestätigungsseite: dieselbe, nur mit `form-action` für genau
    /// den Loopback-Port des Vorgangs (Aufgabe 26, Punkt 3).
    const CSP_MIT_PORT: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
         img-src 'self' data:; font-src 'self'; connect-src 'self'; \
         form-action 'self' http://127.0.0.1:51234; frame-ancestors 'none'; base-uri 'none'";
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
            "same-origin",
            "sonst schickt der Browser beim Formular Origin: null"
        );
        assert_eq!(
            antwort
                .headers()
                .get(header::STRICT_TRANSPORT_SECURITY)
                .unwrap(),
            HSTS
        );
        assert_eq!(
            antwort.headers().get(HEADER_COOP.clone()).unwrap(),
            "same-origin"
        );
        assert_eq!(
            antwort.headers().get(HEADER_CORP.clone()).unwrap(),
            "same-origin"
        );
        let richtlinien = antwort
            .headers()
            .get("permissions-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        for aus in ["camera=()", "microphone=()", "geolocation=()"] {
            assert!(richtlinien.contains(aus), "{aus} fehlt: {richtlinien}");
        }
        assert!(!richtlinien.contains("*"), "nichts wird erlaubt");
        assert_eq!(
            antwort.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert!(!CSP.contains("unsafe-inline"));
        assert!(CSP.contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn eigene_csp_bleibt_stehen() {
        let app = Router::new()
            .route(
                "/auth/seite",
                get(|| async { ([(header::CONTENT_SECURITY_POLICY, CSP_MIT_PORT)], "ok") }),
            )
            .layer(axum::middleware::from_fn(|req: Request, next: Next| {
                sicherheits_kopfzeilen(false, req, next)
            }));
        let antwort = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/auth/seite")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            antwort
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .unwrap(),
            CSP_MIT_PORT
        );
        assert!(CSP_MIT_PORT.contains("form-action 'self' http://127.0.0.1:51234"));
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
