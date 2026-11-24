// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Seiten, die der Server selbst ausliefert (Bestätigung, Fehler und die
//! Wege ganz ohne Anmeldung). Aufgabe 26, Punkt 5:
//!
//! - eigenes Stylesheet `/auth/stil.css` aus den Tokens (`style-src 'self'`),
//!   dunkel/hell über `prefers-color-scheme`,
//! - Logo aus `/auth/marke-dunkel.png` bzw. `/auth/marke-hell.png`,
//! - Titel und Texte deutsch oder englisch nach `Accept-Language`,
//! - Hand-Cursor auf allem Klickbaren (im Stylesheet, nicht in der Ansicht).
//!
//! Kein Inline-Stil, kein Inline-Skript: die CSP bleibt ohne Ausnahme.

use std::fmt::Write;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

/// Die Tokens als CSS-Variablen, erzeugt aus `tokens/` (dunkel als Vorgabe,
/// hell über `prefers-color-scheme`).
const SYSTEM: &str = include_str!("../seiten/system.css");
/// Die Regeln der Seiten – nur `var(--wp-…)`, nie ein Farbwert.
const SEITE: &str = include_str!("../seiten/stil.css");
/// Das Zeichen der Marke – dieselbe Quelle wie Verwaltung und Client
/// (`assets/marke`, dort liegt das Original).
const MARKE_DUNKEL: &[u8] = include_bytes!("../../../assets/marke/logo-dunkel.png");
const MARKE_HELL: &[u8] = include_bytes!("../../../assets/marke/logo-hell.png");

/// Die Sprache der Seite. Deutsch ist die Vorgabe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sprache {
    De,
    En,
}

impl Sprache {
    pub(crate) fn kennung(self) -> &'static str {
        match self {
            Sprache::De => "de",
            Sprache::En => "en",
        }
    }

    /// Zwei Fassungen zur Auswahl: deutsch oder englisch.
    pub(crate) fn waehle<'a>(self, deutsch: &'a str, englisch: &'a str) -> &'a str {
        match self {
            Sprache::De => deutsch,
            Sprache::En => englisch,
        }
    }
}

/// Die Sprache aus `Accept-Language`: die höchstbewertete unterstützte.
pub(crate) fn sprache(headers: &HeaderMap) -> Sprache {
    sprache_aus(
        headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|wert| wert.to_str().ok()),
    )
}

/// `de`, `en` oder nichts: Deutsch ist die Vorgabe, `q=0` heißt „nicht“.
pub(crate) fn sprache_aus(accept: Option<&str>) -> Sprache {
    let Some(text) = accept else {
        return Sprache::De;
    };
    let mut beste: Option<(f32, Sprache)> = None;
    for teil in text.split(',') {
        let mut felder = teil.split(';');
        let tag = felder
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let mut gewicht = 1.0f32;
        for feld in felder {
            if let Some(wert) = feld.trim().strip_prefix("q=") {
                gewicht = wert.parse().unwrap_or(0.0);
            }
        }
        if gewicht <= 0.0 {
            continue;
        }
        let sprache = if tag.starts_with("de") {
            Sprache::De
        } else if tag.starts_with("en") {
            Sprache::En
        } else {
            continue;
        };
        if beste.is_none_or(|(bisher, _)| gewicht > bisher) {
            beste = Some((gewicht, sprache));
        }
    }
    beste.map(|(_, sprache)| sprache).unwrap_or(Sprache::De)
}

/// Ein Text in beiden Sprachen; keine Seite ist einsprachig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Paar {
    pub de: String,
    pub en: String,
}

impl Paar {
    pub(crate) fn neu(de: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            de: de.into(),
            en: en.into(),
        }
    }

    pub(crate) fn fuer(&self, sprache: Sprache) -> &str {
        match sprache {
            Sprache::De => &self.de,
            Sprache::En => &self.en,
        }
    }

    /// Ein technischer Text (aus einer Fehlermeldung): in beiden Sprachen
    /// gleich, weil er nichts erklärt, sondern nur benennt.
    pub(crate) fn technisch(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            de: text.clone(),
            en: text,
        }
    }
}

/// Ein Fehler, der als Seite erscheint (nicht als JSON).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SeitenFehler {
    pub status: StatusCode,
    pub titel: Paar,
    pub text: Paar,
}

impl SeitenFehler {
    fn mit(status: StatusCode, titel: Paar, text: Paar) -> Self {
        Self {
            status,
            titel,
            text,
        }
    }

    pub(crate) fn ungueltig(text: Paar) -> Self {
        Self::mit(
            StatusCode::BAD_REQUEST,
            Paar::neu("Anmeldung nicht möglich", "Sign-in failed"),
            text,
        )
    }

    pub(crate) fn verboten(text: Paar) -> Self {
        Self::mit(
            StatusCode::FORBIDDEN,
            Paar::neu("Kein Zugriff", "Access denied"),
            text,
        )
    }

    pub(crate) fn intern(text: Paar) -> Self {
        Self::mit(
            StatusCode::BAD_GATEWAY,
            Paar::neu("Anmeldung gescheitert", "Sign-in failed"),
            text,
        )
    }

    pub(crate) fn nicht_gefunden(text: Paar) -> Self {
        Self::mit(
            StatusCode::NOT_FOUND,
            Paar::neu("Nicht verfügbar", "Not available"),
            text,
        )
    }

    /// Die Seite zum Fehler, in der Sprache des Browsers.
    pub(crate) fn seite(self, sprache: Sprache) -> Response {
        let absatz = format!("<p>{}</p>", html(self.text.fuer(sprache)));
        seite(
            self.status,
            sprache,
            self.titel.fuer(sprache),
            &absatz,
            None,
        )
    }
}

/// HTML escapen; alles Fremde (Fehlertexte von Entra) geht hier durch.
pub(crate) fn html(wert: &str) -> String {
    let mut aus = String::with_capacity(wert.len());
    for zeichen in wert.chars() {
        match zeichen {
            '&' => aus.push_str("&amp;"),
            '<' => aus.push_str("&lt;"),
            '>' => aus.push_str("&gt;"),
            '"' => aus.push_str("&quot;"),
            '\'' => aus.push_str("&#39;"),
            c => aus.push(c),
        }
    }
    aus
}

/// Eine vollständige Seite mit Logo, Stylesheet und Titel.
///
/// `csp` ersetzt die CSP dieser einen Antwort (die Bestätigungsseite braucht
/// `form-action` für genau den Port des Vorgangs, Aufgabe 26 Punkt 3).
pub(crate) fn seite(
    status: StatusCode,
    sprache: Sprache,
    titel: &str,
    inhalt: &str,
    csp: Option<&str>,
) -> Response {
    let mut rumpf = String::with_capacity(1024 + inhalt.len());
    let _ = write!(
        rumpf,
        "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"dark light\">\
         <title>{titel}</title>\
         <link rel=\"icon\" type=\"image/png\" href=\"/auth/marke-dunkel.png\">\
         <link rel=\"stylesheet\" href=\"/auth/stil.css\"></head>\
         <body><main class=\"seite\">\
         <header class=\"seite__kopf\">\
         <picture>\
         <source media=\"(prefers-color-scheme: light)\" srcset=\"/auth/marke-hell.png\">\
         <img class=\"seite__zeichen\" src=\"/auth/marke-dunkel.png\" alt=\"\" width=\"144\" height=\"144\">\
         </picture>\
         <span class=\"seite__marke\">mcp-quiver</span>\
         </header>\
         <h1 class=\"seite__titel\">{titel}</h1>\
         {inhalt}\
         </main></body></html>",
        lang = sprache.kennung(),
        titel = html(titel),
        inhalt = inhalt,
    );
    let mut antwort = (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        rumpf,
    )
        .into_response();
    if let Some(csp) = csp {
        if let Ok(wert) = HeaderValue::from_str(csp) {
            antwort
                .headers_mut()
                .insert(header::CONTENT_SECURITY_POLICY, wert);
        }
    }
    antwort
}

// ------------------------------------------------------------------- Dateien

/// `GET /auth/stil.css`: Tokens (dunkel/hell) plus die Regeln der Seiten.
pub(crate) async fn stil() -> Response {
    css_antwort()
}

fn css_antwort() -> Response {
    let mut css = String::with_capacity(SYSTEM.len() + SEITE.len());
    css.push_str(SYSTEM);
    css.push_str(SEITE);
    // `Cache-Control: no-store` setzt die Sicherheits-Middleware für `/auth`.
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        css,
    )
        .into_response()
}

/// `GET /auth/marke-dunkel.png`
pub(crate) async fn marke_dunkel() -> Response {
    bild_antwort(MARKE_DUNKEL)
}

/// `GET /auth/marke-hell.png`
pub(crate) async fn marke_hell() -> Response {
    bild_antwort(MARKE_HELL)
}

fn bild_antwort(bytes: &'static [u8]) -> Response {
    (StatusCode::OK, [(header::CONTENT_TYPE, "image/png")], bytes).into_response()
}

/// Die CSP einer Seite: die allgemeine, plus `form-action`-Ziele.
pub(crate) fn csp_mit_form_action(zusatz: &[String]) -> String {
    let mut form = String::from("'self'");
    for ziel in zusatz {
        form.push(' ');
        form.push_str(ziel);
    }
    format!(
        "default-src 'none'; script-src 'self'; style-src 'self'; \
         img-src 'self' data:; font-src 'self'; connect-src 'self'; \
         form-action {form}; frame-ancestors 'none'; base-uri 'none'"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprache_aus_accept_language() {
        assert_eq!(sprache_aus(None), Sprache::De);
        assert_eq!(sprache_aus(Some("")), Sprache::De);
        assert_eq!(sprache_aus(Some("de-DE,de;q=0.9")), Sprache::De);
        assert_eq!(sprache_aus(Some("en-GB,en;q=0.9,de;q=0.5")), Sprache::En);
        // Die Bewertung entscheidet, nicht die Reihenfolge.
        assert_eq!(sprache_aus(Some("de;q=0.5,en;q=0.9")), Sprache::En);
        // `q=0` heißt „nicht“.
        assert_eq!(sprache_aus(Some("en;q=0")), Sprache::De);
        // Unbekannte Sprachen fallen auf Deutsch zurück.
        assert_eq!(sprache_aus(Some("fr-FR,fr;q=0.9")), Sprache::De);
    }

    #[test]
    fn html_wird_escaped() {
        assert_eq!(
            html("<script>\"x\"&'</script>"),
            "&lt;script&gt;&quot;x&quot;&amp;&#39;&lt;/script&gt;"
        );
    }

    #[tokio::test]
    async fn css_kommt_aus_den_tokens() {
        let antwort = css_antwort();
        assert_eq!(antwort.status(), StatusCode::OK);
        assert_eq!(
            antwort.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );
        let rumpf = rumpf(antwort).await;
        assert!(rumpf.contains("--wp-color-bg-canvas"));
        assert!(rumpf.contains("--wp-space-4"));
        assert!(
            rumpf.contains("prefers-color-scheme: light"),
            "hell über die Systemeinstellung"
        );
        // Kein Inline-Stil, kein Skript in der Seite (CSP ohne Ausnahme).
        assert!(!rumpf.contains("<script"));
        assert!(!rumpf.contains("style="));
    }

    #[tokio::test]
    async fn seite_hat_stylesheet_logo_und_sprache() {
        let antwort = seite(StatusCode::OK, Sprache::En, "Sign in", "<p>hello</p>", None);
        assert_eq!(antwort.status(), StatusCode::OK);
        let html = rumpf(antwort).await;
        assert!(html.contains("lang=\"en\""));
        assert!(html.contains("href=\"/auth/stil.css\""));
        assert!(html.contains("/auth/marke-hell.png"));
        assert!(html.contains("mcp-quiver"));
        assert!(html.contains("<h1 class=\"seite__titel\">Sign in</h1>"));
    }

    #[tokio::test]
    async fn fehlerseite_ist_zweisprachig() {
        let fehler = SeitenFehler::ungueltig(Paar::neu("kaputt", "broken"));
        let seite_de = rumpf(fehler.clone().seite(Sprache::De)).await;
        assert!(seite_de.contains("Anmeldung nicht möglich"));
        assert!(seite_de.contains("kaputt"));
        let seite_en = rumpf(fehler.seite(Sprache::En)).await;
        assert!(seite_en.contains("Sign-in failed"));
        assert!(seite_en.contains("broken"));
    }

    #[test]
    fn csp_bekommt_genau_den_port() {
        let csp = csp_mit_form_action(&[String::from("http://127.0.0.1:51234")]);
        assert!(csp.contains("form-action 'self' http://127.0.0.1:51234;"));
        assert!(!csp.contains("unsafe-inline"));
        assert!(csp.contains("frame-ancestors 'none'"));
    }

    async fn rumpf(antwort: Response) -> String {
        let bytes = axum::body::to_bytes(antwort.into_body(), 1024 * 1024)
            .await
            .expect("Rumpf lesbar");
        String::from_utf8_lossy(&bytes).to_string()
    }
}
