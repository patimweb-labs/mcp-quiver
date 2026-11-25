// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Anmeldung **dieses Geräts** am Server (Aufgabe 22).
//!
//! Der Client spricht nie mit Entra. Er fragt zuerst
//! `GET /api/v1/anmeldearten`; mit Entra öffnet er die Anmeldung des Servers
//! im Browser. Der Server schickt nach der Bestätigung genau einen Einmal-Code
//! an einen Lauscher auf `127.0.0.1` (Pfad `/quiver`). Diesen Code tauscht der
//! Client mit seinem eigenen PKCE-Verifier gegen ein Geräte-Token
//! (`POST /api/v1/auth/geraet/tausch`).
//!
//! Der Lauscher lauscht nur auf Loopback, nimmt **genau eine** Antwort an und
//! gibt nach fünf Minuten auf. `zustand` wird in konstanter Zeit verglichen.
//! Der Browser ist ein Trait, damit der Ende-zu-Ende-Test ihn ersetzen kann.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use quiver_protocol::{AnmeldenAntwort, GeraeteTausch, PRAEFIX};
use tokio::sync::oneshot;

use crate::hilfen::{self, Abbruch};

/// So lange darf der User im Browser brauchen.
pub const BROWSER_FRIST: Duration = Duration::from_secs(5 * 60);

/// So lange bleibt der Lauscher nach der Antwort noch da, um die Abschlussseite
/// auszuliefern; weitere Antworten nimmt er dabei nicht an.
const NACHLAUF: Duration = Duration::from_secs(2);

/// Der Pfad, auf den der Server zurückleitet. Er ist im Server fest
/// (`http://127.0.0.1:<port>/quiver`).
pub const PFAD: &str = "/quiver";

/// Wer einen Browser öffnen kann. Im Betrieb das System, im Test eine
/// Attrappe, die den ganzen Weg selbst geht.
pub trait Browser: Send + Sync {
    /// Die Anmeldeseite des Servers öffnen.
    fn oeffnen(&self, adresse: &str) -> anyhow::Result<()>;
}

/// Der Browser des Systems.
///
/// Die Adresse geht direkt an den Systemdienst, **nie** durch eine Shell: in
/// der Adresse stehen `&`, die eine Windows-Shell als Befehlstrenner läse.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBrowser;

impl Browser for SystemBrowser {
    fn oeffnen(&self, adresse: &str) -> anyhow::Result<()> {
        open::that_detached(adresse).map_err(|fehler| anyhow::anyhow!("{fehler}"))
    }
}

/// Was der Lauscher bekommen hat.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Antwort {
    Code(String),
    Fehler(String),
}

/// Der Zustand des Lauschers: einmal annehmen, `zustand` vergleichen.
struct LauscherZustand {
    zustand: String,
    genommen: AtomicBool,
    sender: Mutex<Option<oneshot::Sender<Antwort>>>,
}

/// Die Anmeldung des Servers im Browser durchführen und das Geräte-Token
/// holen.
///
/// `bei_adresse` wird aufgerufen, sobald die Anmeldeseite feststeht — das
/// Fenster zeigt dann „Im Browser fortfahren …“ und kann sie erneut öffnen.
/// `altes_token` lässt dasselbe Gerät weiterführen (sofern es keinem oder
/// demselben Benutzer gehört).
pub(crate) async fn anmelden(
    http: &reqwest::Client,
    browser: &dyn Browser,
    server: &str,
    name: &str,
    altes_token: Option<&str>,
    abbruch: &Abbruch,
    bei_adresse: impl FnOnce(String),
) -> anyhow::Result<AnmeldenAntwort> {
    let verifier = hilfen::zufall_b64(32);
    let challenge = hilfen::s256(&verifier);
    let zustand = hilfen::zufall_b64(32);

    let (lauscher, adresse, mut eingehend) = Lauscher::starten(zustand.clone()).await?;
    let startseite = format!(
        "{}{}?challenge={}&port={}&zustand={}&name={}",
        server.trim_end_matches('/'),
        "/auth/geraet/start",
        urlencode(&challenge),
        adresse.port(),
        urlencode(&zustand),
        urlencode(name),
    );
    bei_adresse(startseite.clone());
    if let Err(fehler) = browser.oeffnen(&startseite) {
        lauscher.beenden();
        anyhow::bail!("Browser lässt sich nicht öffnen: {fehler}");
    }

    let antwort = tokio::select! {
        _ = abbruch.warten() => {
            lauscher.beenden();
            anyhow::bail!("Anmeldung abgebrochen");
        }
        _ = tokio::time::sleep(BROWSER_FRIST) => {
            lauscher.beenden();
            anyhow::bail!("Zeit abgelaufen – bitte erneut anmelden");
        }
        antwort = &mut eingehend => antwort.unwrap_or(Antwort::Fehler(
            "Der Lauscher hat ohne Antwort geendet".to_string(),
        )),
    };
    lauscher.beenden();

    let code = match antwort {
        Antwort::Code(code) => code,
        Antwort::Fehler(fehler) => {
            let fehler = fehler.trim();
            anyhow::bail!(
                "{}",
                if fehler.is_empty() {
                    "Anmeldung im Browser abgebrochen".to_string()
                } else {
                    format!("Anmeldung im Browser abgebrochen: {fehler}")
                }
            );
        }
    };

    // Code gegen Geräte-Token tauschen.
    let antwort = http
        .post(format!(
            "{}{PRAEFIX}/auth/geraet/tausch",
            server.trim_end_matches('/')
        ))
        .json(&GeraeteTausch {
            code,
            verifier,
            altes_token: altes_token.map(str::to_string),
        })
        .send()
        .await
        .map_err(|fehler| anyhow::anyhow!("Server nicht erreichbar: {fehler}"))?;
    let antwort = crate::abgleich::pruefen(antwort).await?;
    antwort
        .json()
        .await
        .map_err(|fehler| anyhow::anyhow!("Antwort der Anmeldung: {fehler}"))
}

/// Ein Lauscher auf `127.0.0.1` mit freiem Port, Pfad `/quiver`.
struct Lauscher {
    aufgabe: tokio::task::JoinHandle<()>,
}

impl Lauscher {
    /// Starten; liefert die lokale Adresse und den Kanal für die eine Antwort.
    async fn starten(
        zustand: String,
    ) -> anyhow::Result<(Self, SocketAddr, oneshot::Receiver<Antwort>)> {
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .map_err(|fehler| anyhow::anyhow!("Kein freier Port auf 127.0.0.1: {fehler}"))?;
        let adresse = listener.local_addr()?;
        let (sender, empfaenger) = oneshot::channel();
        let zustand = Arc::new(LauscherZustand {
            zustand,
            genommen: AtomicBool::new(false),
            sender: Mutex::new(Some(sender)),
        });
        let router = Router::new().route(PFAD, get(quiver)).with_state(zustand);
        let aufgabe = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok((Self { aufgabe }, adresse, empfaenger))
    }

    /// Nichts mehr annehmen (die eine Antwort ist da oder der Vorgang zu Ende),
    /// aber die Abschlussseite noch ausliefern: ein sofortiger Abbruch zeigte im
    /// Browser „Verbindung getrennt“ statt „Fertig“.
    fn beenden(self) {
        let aufgabe = self.aufgabe;
        tokio::spawn(async move {
            tokio::time::sleep(NACHLAUF).await;
            aufgabe.abort();
        });
    }
}

/// `GET /quiver?code=…&zustand=…` (oder `?fehler=…`).
async fn quiver(
    State(z): State<Arc<LauscherZustand>>,
    Query(abfrage): Query<HashMap<String, String>>,
) -> Response {
    let bekommen = abfrage.get("zustand").map(String::as_str).unwrap_or("");
    if !hilfen::gleich_konstant(&z.zustand, bekommen) {
        return (StatusCode::BAD_REQUEST, Html(seite(false))).into_response();
    }
    // Erst nach der Prüfung von `zustand`: eine fremde Antwort darf die eine
    // echte Antwort nicht verbrauchen.
    if z.genommen.swap(true, Ordering::SeqCst) {
        return (StatusCode::CONFLICT, Html(seite(false))).into_response();
    }
    let antwort = match (abfrage.get("code"), abfrage.get("fehler")) {
        (Some(code), _) if !code.trim().is_empty() => Antwort::Code(code.clone()),
        (_, Some(fehler)) => Antwort::Fehler(fehler.chars().take(200).collect()),
        _ => Antwort::Fehler("ohne Code und ohne Fehler".to_string()),
    };
    if let Some(sender) = z
        .sender
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
        .take()
    {
        let _ = sender.send(antwort);
    }
    let kopf = [
        (header::CACHE_CONTROL, "no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
    ];
    (kopf, Html(seite(true))).into_response()
}

/// Die schlichte Abschlussseite. Keine Skripte, keine Farben aus dem Code —
/// der Browser des Systems entscheidet über Schrift und Farbe.
fn seite(erfolg: bool) -> String {
    let (titel, text) = if erfolg {
        (
            "mcp-quiver · Anmeldung",
            "Fertig. Du kannst dieses Fenster schließen und zu mcp-quiver zurückkehren.",
        )
    } else {
        (
            "mcp-quiver · Anmeldung",
            "Diese Antwort gilt nicht (mehr). Bitte starte die Anmeldung in mcp-quiver erneut.",
        )
    };
    format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{titel}</title><style>\
         body{{font-family:system-ui,sans-serif;margin:3rem auto;max-width:36rem;padding:0 1rem}}\
         h1{{font-size:1.5rem;margin:0 0 1rem}}\
         p{{line-height:1.5;margin:0}}\
         </style></head><body><h1>{titel}</h1><p>{text}</p></body></html>"
    )
}

/// Prozentkodierung für Abfragewerte (nur die Zeichen, die wirklich stören).
fn urlencode(wert: &str) -> String {
    let mut aus = String::with_capacity(wert.len());
    for byte in wert.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            aus.push(byte as char);
        } else {
            aus.push('%');
            aus.push_str(&format!("{byte:02X}"));
        }
    }
    aus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencode_laesst_sichere_zeichen_stehen() {
        assert_eq!(urlencode("abc-123_x.y~z"), "abc-123_x.y~z");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("ü"), "%C3%BC");
        assert_eq!(urlencode("&=/?"), "%26%3D%2F%3F");
    }

    #[test]
    fn die_abschlussseite_ist_schlicht_und_ohne_skript() {
        let fertig = seite(true);
        assert!(fertig.contains("Fertig"), "{fertig}");
        assert!(!fertig.contains("<script"), "kein Skript: {fertig}");
        assert!(!fertig.contains('#'), "keine Farbwerte: {fertig}");
        let fehler = seite(false);
        assert!(fehler.contains("erneut"), "{fehler}");
        assert!(!fehler.contains("<script"), "kein Skript: {fehler}");
    }

    /// Der Lauscher nimmt genau eine Antwort an und prüft `zustand` in
    /// konstanter Zeit.
    #[tokio::test]
    async fn lauscher_nimmt_genau_eine_antwort_und_prueft_zustand() {
        let (lauscher, adresse, mut empfaenger) = Lauscher::starten("richtig".to_string())
            .await
            .expect("Lauscher");
        let basis = format!("http://{adresse}");
        let http = reqwest::Client::new();

        // Falscher zustand: 400, keine Antwort.
        let falsch = http
            .get(format!("{basis}{PFAD}?code=abc&zustand=falsch"))
            .send()
            .await
            .expect("erreichbar");
        assert_eq!(falsch.status(), StatusCode::BAD_REQUEST);

        // Richtig: 200, Code kommt an.
        let richtig = http
            .get(format!("{basis}{PFAD}?code=abc&zustand=richtig"))
            .send()
            .await
            .expect("erreichbar");
        assert_eq!(richtig.status(), StatusCode::OK);
        assert_eq!(
            empfaenger.try_recv().expect("Antwort"),
            Antwort::Code("abc".to_string())
        );

        // Ein zweiter Aufruf wird abgewiesen und ändert nichts.
        let zweite = http
            .get(format!("{basis}{PFAD}?code=def&zustand=richtig"))
            .send()
            .await
            .expect("erreichbar");
        assert_eq!(zweite.status(), StatusCode::CONFLICT);
        assert!(empfaenger.try_recv().is_err(), "keine zweite Antwort");

        lauscher.beenden();
    }

    /// Wer die Antwort sofort abholt und beendet, schneidet dem Browser die
    /// Abschlussseite nicht ab; danach ist der Port zu.
    #[tokio::test]
    async fn die_abschlussseite_kommt_auch_bei_sofortigem_ende_an() {
        let (lauscher, adresse, empfaenger) =
            Lauscher::starten("z".to_string()).await.expect("Lauscher");
        let abholen = tokio::spawn(async move {
            let antwort = empfaenger.await.expect("Antwort");
            lauscher.beenden();
            antwort
        });
        let http = reqwest::Client::new();
        let url = format!("http://{adresse}{PFAD}?code=abc&zustand=z");
        let antwort = http.get(&url).send().await.expect("erreichbar");
        assert!(antwort.text().await.expect("Seite").contains("Fertig"));
        assert_eq!(abholen.await.unwrap(), Antwort::Code("abc".to_string()));
        tokio::time::sleep(NACHLAUF + Duration::from_millis(500)).await;
        assert!(reqwest::Client::new().get(&url).send().await.is_err());
    }

    #[tokio::test]
    async fn lauscher_meldet_abbruch_weiter() {
        let (lauscher, adresse, mut empfaenger) =
            Lauscher::starten("z".to_string()).await.expect("Lauscher");
        let http = reqwest::Client::new();
        let antwort = http
            .get(format!(
                "http://{adresse}{PFAD}?fehler=abgebrochen&zustand=z"
            ))
            .send()
            .await
            .expect("erreichbar");
        assert_eq!(antwort.status(), StatusCode::OK);
        assert_eq!(
            empfaenger.try_recv().expect("Antwort"),
            Antwort::Fehler("abgebrochen".to_string())
        );
        lauscher.beenden();
    }
}
