// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Sitzungen der Verwaltung: Cookie, Absolute- und Leerlaufgrenze, Origin-Prüfung.
//!
//! Die Sitzung ist ein 256-Bit-Zufallstoken; in der Datenbank steht nur sein
//! Hash. Rechte werden bei **jeder** Anfrage frisch aus der Datenbank gelesen,
//! damit ein Rollenentzug sofort wirkt (Aufgabe 20).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::State;
use axum::extract::{ConnectInfo, Request};
use axum::http::{header, HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;
use quiver_protocol::{Anmeldeart, Rolle};
use time::OffsetDateTime;
use url::Url;

use crate::config::aufrufer_ip;
use crate::db;
use crate::fehler::{Ergebnis, FehlerAntwort};
use crate::hilfen;
use crate::speicher::{self, Benutzer};
use crate::zustand::Zustand;

/// Name des Sitzungscookies ohne https.
pub const SITZUNG: &str = "quiver_sitzung";
/// Name des Sitzungscookies mit https (an den Host gebunden).
pub const SITZUNG_SICHER: &str = "__Host-quiver_sitzung";
/// Cookie, das den laufenden Entra-Vorgang an diesen Browser bindet.
pub const OIDC_COOKIE: &str = "quiver_oidc";
/// Cookie für die Bestätigungsseite.
pub const BESTAETIGUNG_COOKIE: &str = "quiver_bestaetigung";

/// Wie lange eine Sitzung höchstens lebt.
pub const ABSOLUT_STUNDEN: i64 = 12;
/// Wie lange eine Sitzung ohne Nutzung lebt.
pub const LEERLAUF_MINUTEN: i64 = 60;

/// Eine geprüfte Sitzung samt Benutzer (Rechte frisch aus der DB).
#[derive(Debug, Clone)]
pub(crate) struct SitzungKontext {
    pub benutzer: Benutzer,
    pub art: Anmeldeart,
}

fn cookie_name(https: bool) -> &'static str {
    if https {
        SITZUNG_SICHER
    } else {
        SITZUNG
    }
}

/// Alle Cookies einer Anfrage (einfach, ohne zusätzliche Abhängigkeit).
pub fn cookies(req: &Request) -> std::collections::HashMap<String, String> {
    cookies_aus(req.headers())
}

pub fn cookies_aus(headers: &HeaderMap) -> std::collections::HashMap<String, String> {
    let mut aus = std::collections::HashMap::new();
    for wert in headers.get_all(header::COOKIE) {
        let Ok(text) = wert.to_str() else {
            continue;
        };
        for teil in text.split(';') {
            if let Some((name, wert)) = teil.split_once('=') {
                aus.insert(name.trim().to_string(), wert.trim().to_string());
            }
        }
    }
    aus
}

/// Sitzungstoken aus dem Cookie der Anfrage.
pub fn sitzungstoken(req: &Request, https: bool) -> Option<String> {
    cookies(req).remove(cookie_name(https))
}

/// `Set-Cookie` für die Sitzung.
pub fn sitzung_cookie(token: &str, https: bool) -> String {
    let mut kopf = format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        cookie_name(https),
        ABSOLUT_STUNDEN * 3600
    );
    if https {
        kopf.push_str("; Secure");
    }
    kopf
}

/// `Set-Cookie`, das ein Cookie löscht.
pub fn loesch_cookie(name: &str, pfad: &str, https: bool) -> String {
    let mut kopf = format!("{name}=; Path={pfad}; HttpOnly; SameSite=Lax; Max-Age=0");
    if https {
        kopf.push_str("; Secure");
    }
    kopf
}

/// Ein kurzlebiges HttpOnly-Cookie (OIDC-Zustand, Bestätigung).
pub fn kurz_cookie(name: &str, wert: &str, https: bool, sekunden: i64) -> String {
    let mut kopf = format!("{name}={wert}; Path=/auth; HttpOnly; SameSite=Lax; Max-Age={sekunden}");
    if https {
        kopf.push_str("; Secure");
    }
    kopf
}

/// Sitzung anlegen; gibt das Klartext-Token zurück (nur einmal).
pub fn sitzung_anlegen(
    conn: &rusqlite::Connection,
    oid: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<String> {
    let token = hilfen::token_erzeugen();
    speicher::sitzung_anlegen(conn, &hilfen::hash(&token), oid, jetzt)?;
    Ok(token)
}

/// Sitzung aus dem Cookie prüfen. `None` = keine gültige Sitzung.
///
/// Bei Entra-Benutzern gilt zusätzlich (Aufgabe 26, Punkt 1):
/// - `token_stand` muss `gueltig` sein,
/// - gesperrt, gelöscht oder `neu_anmelden` beendet **alle** Sitzungen des
///   Benutzers sofort (löschen, nicht nur ablehnen),
/// - ist die letzte Nachprüfung länger als 60 Minuten her, prüft die Wache
///   vor dem Durchlassen nach.
pub async fn sitzung_aus_cookie(
    zustand: &Arc<Zustand>,
    token: &str,
) -> Ergebnis<Option<SitzungKontext>> {
    let hash = hilfen::hash(token);
    let jetzt = zustand.uhr.jetzt();
    let treffer = db(zustand, {
        let hash = hash.clone();
        move |conn| {
            let Some(sitzung) = speicher::sitzung_holen(conn, &hash, jetzt)? else {
                return Ok(None);
            };
            let Some(benutzer) = speicher::benutzer_holen(conn, &sitzung.oid)? else {
                conn.execute(
                    "DELETE FROM sitzungen WHERE hash = ?1",
                    rusqlite::params![hash],
                )?;
                return Ok(None);
            };
            Ok(Some(benutzer))
        }
    })
    .await?;
    let Some(mut benutzer) = treffer else {
        return Ok(None);
    };

    // Der Root hat kein Entra; für ihn gilt nur das Passwort (und damit die
    // Sitzungen selbst).
    if benutzer.oid != speicher::ROOT_OID {
        if !benutzer.anmeldung_gueltig() {
            sitzungen_beenden(zustand, &benutzer.oid).await?;
            return Ok(None);
        }
        if pruefung_faellig(&benutzer, jetzt) {
            if crate::refresh::bei_verbindung(zustand, &benutzer.oid)
                .await
                .is_err()
            {
                sitzungen_beenden(zustand, &benutzer.oid).await?;
                return Ok(None);
            }
            // Die Prüfung kann Rollen und Gruppen geändert haben: neu lesen.
            let kennung = benutzer.oid.clone();
            let Some(frisch) = db(zustand, move |conn| {
                speicher::benutzer_holen(conn, &kennung)
            })
            .await?
            else {
                sitzungen_beenden(zustand, &benutzer.oid).await?;
                return Ok(None);
            };
            if !frisch.anmeldung_gueltig() {
                sitzungen_beenden(zustand, &frisch.oid).await?;
                return Ok(None);
            }
            benutzer = frisch;
        }
    }

    Ok(Some(SitzungKontext {
        art: if benutzer.rolle == Rolle::Root {
            Anmeldeart::Root
        } else {
            Anmeldeart::Entra
        },
        benutzer,
    }))
}

/// Ist die letzte Nachprüfung länger als 60 Minuten her (oder gab es keine)?
fn pruefung_faellig(benutzer: &Benutzer, jetzt: OffsetDateTime) -> bool {
    benutzer
        .token_erneuert
        .as_deref()
        .and_then(hilfen::zeit_lesen)
        .is_none_or(|zuletzt| {
            jetzt - zuletzt > time::Duration::minutes(crate::refresh::BEI_VERBINDUNG_MINUTEN)
        })
}

/// Alle Sitzungen eines Benutzers am Server löschen.
async fn sitzungen_beenden(zustand: &Arc<Zustand>, oid: &str) -> Ergebnis<()> {
    if oid == speicher::ROOT_OID {
        return Ok(());
    }
    let kennung = oid.to_string();
    db(zustand, move |conn| {
        let anzahl = speicher::sitzungen_von_user_loeschen(conn, &kennung)?;
        if anzahl > 0 {
            tracing::info!("{anzahl} Sitzung(en) von {kennung} beendet");
        }
        Ok(())
    })
    .await?;
    Ok(())
}

/// Geschlossene Sitzung am Server löschen.
pub async fn sitzung_beenden(zustand: &Arc<Zustand>, token: &str) -> Ergebnis<()> {
    let hash = hilfen::hash(token);
    db(zustand, move |conn| {
        speicher::sitzung_loeschen(conn, &hash)?;
        Ok(())
    })
    .await?;
    Ok(())
}

/// Ist die Methode ändernd? (Dann gelten Origin und `X-Quiver`.)
pub fn ist_aendernd(methode: &Method) -> bool {
    matches!(
        *methode,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Ist die `Origin` genau die Basis-URL?
pub fn origin_passt(origin: &str, oeffentlich: &str) -> bool {
    let (Ok(a), Ok(b)) = (Url::parse(origin), Url::parse(oeffentlich)) else {
        return false;
    };
    a.scheme().eq_ignore_ascii_case(b.scheme())
        && a.host_str()
            .zip(b.host_str())
            .is_some_and(|(x, y)| x.eq_ignore_ascii_case(y))
        && a.port_or_known_default() == b.port_or_known_default()
        && a.path() == "/"
        && a.query().is_none()
        && a.fragment().is_none()
}

/// Nur den Origin prüfen (für Formulare im Browser, die keinen eigenen Kopf
/// setzen können).
pub fn origin_pruefen(headers: &HeaderMap, oeffentlich: &str) -> Ergebnis<()> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|wert| wert.to_str().ok())
        .ok_or_else(|| FehlerAntwort::verboten("Origin fehlt"))?;
    if !origin_passt(origin, oeffentlich) {
        return Err(FehlerAntwort::verboten(
            "Origin passt nicht zur Basis-URL des Servers",
        ));
    }
    Ok(())
}

/// Origin **und** `X-Quiver: 1` prüfen; fehlt eins, gibt es 403.
pub fn aenderung_pruefen(headers: &HeaderMap, oeffentlich: &str) -> Ergebnis<()> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|wert| wert.to_str().ok())
        .ok_or_else(|| {
            FehlerAntwort::verboten("Origin fehlt; ändernde Anfragen brauchen den Origin")
        })?;
    if !origin_passt(origin, oeffentlich) {
        return Err(FehlerAntwort::verboten(
            "Origin passt nicht zur Basis-URL des Servers",
        ));
    }
    let kennung = headers
        .get("x-quiver")
        .and_then(|wert| wert.to_str().ok())
        .unwrap_or_default();
    if kennung != "1" {
        return Err(FehlerAntwort::verboten(
            "Kopf X-Quiver: 1 fehlt; die Anfrage kommt nicht aus der Verwaltung",
        ));
    }
    Ok(())
}

/// Die IP des Aufrufers, nur mit vertrauten Proxies aus `X-Forwarded-For`.
///
/// **Alle** Kopfzeilen dieses Namens werden in ihrer Reihenfolge übergeben: ein
/// Proxy kann eine zweite Zeile anhängen; nur die erste zu lesen hieße, dem
/// Client zu glauben (siehe [`crate::config::aufrufer_ip`]).
pub fn aufrufer(req: &Request, zustand: &Zustand) -> Option<IpAddr> {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let koepfe: Vec<&str> = req
        .headers()
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|wert| wert.to_str().ok())
        .collect();
    aufrufer_ip(peer, &koepfe, &zustand.konfig.proxies)
}

/// Ist die Adresse des Aufrufers unterscheidbar?
///
/// Hinter einem vertrauten Proxy nur dann, wenn `X-Forwarded-For` eine Kette
/// nennt; ohne Kopf ist die Adresse die des Proxys und für alle gleich (etwa
/// die Docker-Brücke). Dann darf eine Konto-Drossel nur kurz greifen.
pub fn ip_unterscheidbar(req: &Request, zustand: &Zustand) -> bool {
    let Some(peer) = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0)
    else {
        return false;
    };
    if zustand
        .konfig
        .proxies
        .iter()
        .any(|netz| netz.enthaelt(peer.ip()))
    {
        return req.headers().get("x-forwarded-for").is_some();
    }
    true
}

/// Middleware für die Verwaltung: Sitzung, Rolle frisch, Origin bei Änderungen.
pub(crate) async fn verwaltung_wache(
    State(zustand): State<Arc<Zustand>>,
    mut req: Request,
    next: Next,
) -> Ergebnis<Response> {
    // Vor der Ersteinrichtung ist die Verwaltung ganz gesperrt (Aufgabe 25);
    // Geräte laufen weiter.
    if !db(&zustand, |conn| speicher::root_ist_gesetzt(conn)).await? {
        return Err(FehlerAntwort::verboten(
            "dieser Server ist noch nicht eingerichtet; bitte die Ersteinrichtung im Browser ausfüllen",
        ));
    }
    let token = sitzungstoken(&req, zustand.konfig.ist_https())
        .ok_or_else(|| FehlerAntwort::nicht_angemeldet("Sitzung fehlt"))?;
    let Some(kontext) = sitzung_aus_cookie(&zustand, &token).await? else {
        return Err(FehlerAntwort::nicht_angemeldet(
            "Sitzung abgelaufen oder beendet",
        ));
    };
    if !kontext.benutzer.rolle.darf_verwalten() {
        return Err(FehlerAntwort::verboten(
            "diese Rolle darf die Verwaltung nicht benutzen",
        ));
    }
    if kontext.benutzer.gesperrt {
        return Err(FehlerAntwort::verboten("Benutzer ist gesperrt"));
    }
    if ist_aendernd(req.method()) {
        aenderung_pruefen(req.headers(), &zustand.konfig.oeffentlich)?;
    }
    req.extensions_mut().insert(kontext);
    Ok(next.run(req).await)
}

/// Middleware für angemeldete Benutzer ohne Verwaltungsrecht (`/auth/ich`).
pub(crate) async fn angemeldet_wache(
    State(zustand): State<Arc<Zustand>>,
    mut req: Request,
    next: Next,
) -> Ergebnis<Response> {
    let token = sitzungstoken(&req, zustand.konfig.ist_https())
        .ok_or_else(|| FehlerAntwort::nicht_angemeldet("Sitzung fehlt"))?;
    let Some(kontext) = sitzung_aus_cookie(&zustand, &token).await? else {
        return Err(FehlerAntwort::nicht_angemeldet(
            "Sitzung abgelaufen oder beendet",
        ));
    };
    if kontext.benutzer.gesperrt {
        return Err(FehlerAntwort::verboten("Benutzer ist gesperrt"));
    }
    req.extensions_mut().insert(kontext);
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ohne Peer-Adresse und hinter einem vertrauten Proxy ohne Kopf ist die
    /// Adresse nicht unterscheidbar (Härtung Punkt 5).
    #[test]
    fn die_adresse_ist_nicht_immer_unterscheidbar() {
        use crate::config::{Konfig, Netz};
        let mut konfig = Konfig::neu(".");
        let zustand = crate::zustand::Zustand::neu(
            rusqlite::Connection::open_in_memory().expect("Speicher"),
            konfig.clone(),
            crate::krypto::Schluessel::aus_bytes(&[7u8; 32]).expect("Schlüssel"),
            None,
            None,
            String::from("dummy"),
        );
        let mut anfrage = Request::builder()
            .uri("/x")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!ip_unterscheidbar(&anfrage, &zustand), "ohne Peer nicht");

        anfrage.extensions_mut().insert(ConnectInfo(
            "203.0.113.9:5000".parse::<SocketAddr>().unwrap(),
        ));
        assert!(ip_unterscheidbar(&anfrage, &zustand), "einzelne Adresse");

        // Hinter einem vertrauten Proxy ohne Kopf: die Adresse ist die des
        // Proxys, für alle gleich.
        konfig.proxies = vec![Netz::lesen("10.0.0.0/8").unwrap()];
        let zustand = crate::zustand::Zustand::neu(
            rusqlite::Connection::open_in_memory().expect("Speicher"),
            konfig,
            crate::krypto::Schluessel::aus_bytes(&[7u8; 32]).expect("Schlüssel"),
            None,
            None,
            String::from("dummy"),
        );
        let mut proxy = Request::builder()
            .uri("/x")
            .body(axum::body::Body::empty())
            .unwrap();
        proxy
            .extensions_mut()
            .insert(ConnectInfo("10.0.0.5:5000".parse::<SocketAddr>().unwrap()));
        assert!(!ip_unterscheidbar(&proxy, &zustand), "Proxy ohne Kette");
        let mut mit_kopf = Request::builder()
            .uri("/x")
            .header("x-forwarded-for", "203.0.113.9")
            .body(axum::body::Body::empty())
            .unwrap();
        mit_kopf
            .extensions_mut()
            .insert(ConnectInfo("10.0.0.5:5000".parse::<SocketAddr>().unwrap()));
        assert!(
            ip_unterscheidbar(&mit_kopf, &zustand),
            "Kette nennt den Absender"
        );
    }

    #[test]
    fn origin_muss_exakt_passen() {
        assert!(origin_passt(
            "https://quiver.example.test",
            "https://quiver.example.test"
        ));
        assert!(origin_passt(
            "https://QUIVER.example.test",
            "https://quiver.example.test"
        ));
        assert!(!origin_passt(
            "https://evil.example.test",
            "https://quiver.example.test"
        ));
        assert!(!origin_passt(
            "http://quiver.example.test",
            "https://quiver.example.test"
        ));
        assert!(!origin_passt(
            "https://quiver.example.test:8443",
            "https://quiver.example.test"
        ));
        assert!(!origin_passt("null", "https://quiver.example.test"));
        assert!(!origin_passt(
            "https://quiver.example.test/pfad",
            "https://quiver.example.test"
        ));
    }

    #[test]
    fn aenderung_braucht_origin_und_kopf() {
        let mut kopf = HeaderMap::new();
        assert!(aenderung_pruefen(&kopf, "https://quiver.example.test").is_err());
        kopf.insert(
            header::ORIGIN,
            "https://quiver.example.test".parse().unwrap(),
        );
        assert!(aenderung_pruefen(&kopf, "https://quiver.example.test").is_err());
        kopf.insert("x-quiver", "1".parse().unwrap());
        assert!(aenderung_pruefen(&kopf, "https://quiver.example.test").is_ok());
        kopf.insert(header::ORIGIN, "https://evil.example.test".parse().unwrap());
        assert!(aenderung_pruefen(&kopf, "https://quiver.example.test").is_err());
    }

    #[test]
    fn cookies_werden_gelesen() {
        let mut kopf = HeaderMap::new();
        kopf.insert(
            header::COOKIE,
            "a=1; quiver_sitzung= abc ; b=2".parse().unwrap(),
        );
        let karte = cookies_aus(&kopf);
        assert_eq!(karte.get("quiver_sitzung").map(String::as_str), Some("abc"));
        assert_eq!(karte.get("a").map(String::as_str), Some("1"));
    }

    #[test]
    fn cookie_kopfzeilen() {
        let sicher = sitzung_cookie("token", true);
        assert!(sicher.starts_with("__Host-quiver_sitzung=token"));
        assert!(sicher.contains("HttpOnly"));
        assert!(sicher.contains("SameSite=Lax"));
        assert!(sicher.contains("Path=/"));
        assert!(sicher.contains("Secure"));
        let offen = sitzung_cookie("token", false);
        assert!(offen.starts_with("quiver_sitzung=token"));
        assert!(!offen.contains("Secure"));
    }
}
