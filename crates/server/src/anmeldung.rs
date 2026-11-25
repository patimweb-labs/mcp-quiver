// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Anmeldung: der Browser-Weg über Entra und der Tausch Code → Geräte-Token.
//!
//! Der Server ist der einzige OIDC-Client (ADR 0002). Das Gerät schickt einen
//! PKCE-Challenge, bekommt nach der Bestätigung im Browser einen Einmal-Code
//! über `http://127.0.0.1:<port>/quiver` und tauscht ihn mit seinem Verifier
//! gegen ein Geräte-Token.
//!
//! Die Seiten sind bewusst schmucklos: kein Inline-Skript, kein Inline-Stil,
//! damit die CSP ohne Ausnahme gilt. Alles, was von außen kommt, wird escaped.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use quiver_domain::{Geraet, GeraetId};
use quiver_protocol::{AnmeldeGrund, AnmeldenAntwort, GeraeteTausch};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use url::form_urlencoded;

use crate::db;
use crate::fehler::{Ergebnis, FehlerAntwort, Koerper};
use crate::hilfen;
use crate::krypto;
use crate::oidc::Identitaet;
use crate::seiten::{self, Paar, SeitenFehler, Sprache};
use crate::sitzung;
use crate::speicher::{self, Bestaetigung, GeraeteCode, OidcZustand};
use crate::zustand::Zustand;

/// Gültigkeit eines Anmeldevorgangs im Browser.
const ZUSTAND_MINUTEN: i64 = 10;
/// Gültigkeit des Einmalcodes für den Tausch.
const CODE_SEKUNDEN: i64 = 60;
/// Länge eines PKCE-Werts (32 Byte → 43 Zeichen base64url).
const PKCE_ZEICHEN: usize = 43;
/// Längste `zustand`-Zeichenkette.
const ZUSTAND_MAX: usize = 128;
/// Längster Gerätename.
const NAME_MAX: usize = 80;
/// Längster Fehlertext von Entra, der angezeigt wird.
const FEHLERTEXT_MAX: usize = 200;

/// So viele Anmeldevorgänge dürfen gleichzeitig offen sein (Härtung, Punkt 1).
///
/// Jeder Start legt eine Zeile in `oidc_zustand` an. Die Zahl der offenen
/// Vorgänge bleibt begrenzt, damit Tabelle und Datenbankverbindung im Rahmen
/// bleiben; darüber antwortet der Start mit 503 und einem Hinweis.
pub const OFFENE_VORGAENGE_MAX: usize = 2_000;

/// Ein Weiterleitung, wahlweise mit einem Cookie (Set-Cookie).
fn weiter(status: StatusCode, ziel: &str, cookie: Option<String>) -> Response {
    let mut antwort = Response::builder()
        .status(status)
        .header(header::LOCATION, ziel)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    if let Some(cookie) = cookie {
        if let Ok(wert) = HeaderValue::from_str(&cookie) {
            antwort.headers_mut().insert(header::SET_COOKIE, wert);
        }
    }
    antwort
}

fn formular(bytes: &[u8]) -> HashMap<String, String> {
    form_urlencoded::parse(bytes).into_owned().collect()
}

// ----------------------------------------------------------------- Prüfungen

fn pkce_gueltig(wert: &str) -> bool {
    wert.len() == PKCE_ZEICHEN
        && URL_SAFE_NO_PAD
            .decode(wert)
            .map(|bytes| bytes.len() == 32)
            .unwrap_or(false)
}

fn s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn geraetezustand_gueltig(wert: &str) -> bool {
    !wert.is_empty()
        && wert.len() <= ZUSTAND_MAX
        && wert
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~'))
}

fn name_gueltig(wert: &str) -> bool {
    let wert = wert.trim();
    !wert.is_empty() && wert.chars().count() <= NAME_MAX && !wert.chars().any(char::is_control)
}

fn port_gueltig(wert: &str) -> Option<u16> {
    let port: u16 = wert.parse().ok()?;
    (1024..=65535).contains(&port).then_some(port)
}

/// Kürzt einen fremden Fehlertext auf eine kurze, escaped Anzeige.
fn kurz(text: &str) -> String {
    let mut aus: String = text.chars().take(FEHLERTEXT_MAX).collect();
    if text.chars().count() > FEHLERTEXT_MAX {
        aus.push('…');
    }
    aus
}

// ------------------------------------------------------------------- Start

/// `GET /auth/geraet/start?challenge&port&zustand&name`
pub(crate) async fn geraet_start(
    State(zustand): State<Arc<Zustand>>,
    req: axum::extract::Request,
) -> Response {
    let sprache = seiten::sprache(req.headers());
    let abfrage = abfrage_aus(&req);
    // Drossel je Adresse (Härtung, Punkt 1).
    let ip = sitzung::aufrufer(&req, &zustand);
    if let Some(fehler) = start_drosseln(&zustand, ip) {
        return fehler.seite(sprache);
    }
    match start(&zustand, &abfrage).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(sprache),
    }
}

/// Die Abfragewerte aus der Adresse (ohne zustätzliche Abhängigkeit).
fn abfrage_aus(req: &axum::extract::Request) -> HashMap<String, String> {
    req.uri()
        .query()
        .map(|query| {
            form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default()
}

async fn start(
    zustand: &Arc<Zustand>,
    abfrage: &HashMap<String, String>,
) -> Result<Response, SeitenFehler> {
    if zustand.oidc.is_none() {
        return Err(SeitenFehler::nicht_gefunden(
            Paar::neu(
                "Dieser Server nutzt Einmal-Codes statt Entra; /auth/geraet/start gibt es hier nicht.",
                "This server uses one-time codes instead of Entra; /auth/geraet/start does not exist here.",
            ),
        ));
    }

    let challenge = abfrage.get("challenge").cloned().unwrap_or_default();
    if !pkce_gueltig(&challenge) {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "challenge muss ein S256-Wert sein (43 Zeichen base64url).",
            "challenge must be an S256 value (43 base64url characters).",
        )));
    }
    let port = abfrage
        .get("port")
        .and_then(|wert| port_gueltig(wert))
        .ok_or_else(|| {
            SeitenFehler::ungueltig(Paar::neu(
                "port muss zwischen 1024 und 65535 liegen.",
                "port must be between 1024 and 65535.",
            ))
        })?;
    let geraetezustand = abfrage.get("zustand").cloned().unwrap_or_default();
    if !geraetezustand_gueltig(&geraetezustand) {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "zustand fehlt oder enthält unerlaubte Zeichen (höchstens 128).",
            "zustand is missing or contains characters that are not allowed (at most 128).",
        )));
    }
    let name = abfrage.get("name").cloned().unwrap_or_default();
    if !name_gueltig(&name) {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "name fehlt oder ist länger als 80 Zeichen.",
            "name is missing or longer than 80 characters.",
        )));
    }

    starte(zustand, "geraet", &challenge, port, &geraetezustand, &name).await
}

/// Den OIDC-Vorgang anlegen und den Browser zu Entra schicken.
async fn starte(
    zustand: &Arc<Zustand>,
    modus: &str,
    challenge: &str,
    port: u16,
    geraetezustand: &str,
    name: &str,
) -> Result<Response, SeitenFehler> {
    let Some(oidc) = zustand.oidc.as_ref() else {
        return Err(SeitenFehler::nicht_gefunden(Paar::neu(
            "Entra ist auf diesem Server nicht eingerichtet.",
            "Entra is not configured on this server.",
        )));
    };
    let jetzt = zustand.uhr.jetzt();
    let offene_max = zustand.konfig.offene_vorgaenge_max;
    let state = krypto::zufall_b64(32);
    let nonce = krypto::zufall_b64(32);
    let verifier = krypto::zufall_b64(32);
    let cookie = krypto::zufall_b64(32);
    let server_challenge = s256(&verifier);

    let challenge = challenge.to_string();
    let geraetezustand = geraetezustand.to_string();
    let name = name.to_string();
    let modus = modus.to_string();
    db(zustand, {
        let state = state.clone();
        let cookie = cookie.clone();
        let verifier = verifier.clone();
        let nonce = nonce.clone();
        let challenge = challenge.clone();
        let geraetezustand = geraetezustand.clone();
        let name = name.clone();
        let modus = modus.clone();
        move |conn| {
            // Obergrenze offener Vorgänge: darüber 503 statt weiterer Zeilen.
            // Aufgeräumt wird im Hintergrund (`refresh::hintergrund`), nicht
            // hier im Anfragepfad.
            if speicher::oidc_offen(conn)? >= offene_max {
                return Ok(Err(SeitenFehler::nicht_verfuegbar(Paar::neu(
                    "gerade laufen zu viele Anmeldungen; bitte in einigen Minuten erneut versuchen",
                    "too many sign-in attempts are pending; please try again in a few minutes",
                ))));
            }
            speicher::oidc_anlegen(
                conn,
                &hilfen::hash(&state),
                &OidcZustand {
                    cookie_hash: hilfen::hash(&cookie),
                    verifier,
                    nonce,
                    challenge,
                    modus,
                    port,
                    geraetezustand,
                    name,
                    gueltig_bis: hilfen::als_text(jetzt + time::Duration::minutes(ZUSTAND_MINUTEN)),
                },
            )?;
            Ok(Ok(()))
        }
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))??;

    let ziel = oidc
        .anmelde_url(&state, &nonce, &server_challenge, jetzt)
        .await
        .map_err(|fehler| {
            SeitenFehler::intern(Paar::neu(
                format!("Entra nicht erreichbar: {fehler}"),
                format!("Entra is not reachable: {fehler}"),
            ))
        })?;
    Ok(weiter(
        StatusCode::SEE_OTHER,
        &ziel,
        Some(sitzung::kurz_cookie(
            sitzung::OIDC_COOKIE,
            &cookie,
            zustand.konfig.ist_https(),
            ZUSTAND_MINUTEN * 60,
        )),
    ))
}

/// `GET /auth/admin/start`: Anmeldung der Verwaltung über Entra.
pub(crate) async fn admin_start(
    State(zustand): State<Arc<Zustand>>,
    req: axum::extract::Request,
) -> Response {
    let sprache = seiten::sprache(req.headers());
    // Derselbe Weg wie beim Gerät: Drossel je Adresse, Obergrenze offener
    // Vorgänge (Härtung, Punkt 1).
    let ip = sitzung::aufrufer(&req, &zustand);
    if let Some(fehler) = start_drosseln(&zustand, ip) {
        return fehler.seite(sprache);
    }
    match starte(&zustand, "admin", "", 0, "", "").await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(sprache),
    }
}

/// Prüft die Drossel für einen Anmelde-Start; `Some` heißt: abweisen.
fn start_drosseln(zustand: &Arc<Zustand>, ip: Option<std::net::IpAddr>) -> Option<SeitenFehler> {
    let jetzt = zustand.uhr.jetzt();
    let schluessel = format!("start:{}", crate::drossel::ip_schluessel(ip));
    let wartezeit = zustand.drossel.treffer(&schluessel, jetzt);
    wartezeit.map(|sekunden| {
        SeitenFehler::nicht_verfuegbar(Paar::neu(
            format!("zu viele Anmeldeversuche; bitte {sekunden} Sekunden warten"),
            format!("too many sign-in attempts; please wait {sekunden} seconds"),
        ))
    })
}

// ---------------------------------------------------------------- Rückkehr

/// `GET /auth/callback?code&state`
pub(crate) async fn callback(
    State(zustand): State<Arc<Zustand>>,
    Query(abfrage): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let sprache = seiten::sprache(&headers);
    match callback_intern(&zustand, &abfrage, &headers).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(sprache),
    }
}

async fn callback_intern(
    zustand: &Arc<Zustand>,
    abfrage: &HashMap<String, String>,
    headers: &HeaderMap,
) -> Result<Response, SeitenFehler> {
    // Ein Fehler von Entra: nur Code und kurzer Text, escaped.
    if let Some(fehler) = abfrage.get("error") {
        let text = abfrage
            .get("error_description")
            .map(String::as_str)
            .unwrap_or_default();
        return Err(SeitenFehler::ungueltig(Paar::neu(
            format!("Entra meldet: {} {}", kurz(fehler), kurz(text)),
            format!("Entra reports: {} {}", kurz(fehler), kurz(text)),
        )));
    }

    let state = abfrage.get("state").cloned().unwrap_or_default();
    if state.is_empty() {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "state fehlt.",
            "state is missing.",
        )));
    }
    let cookie = sitzung::cookies_aus(headers)
        .remove(sitzung::OIDC_COOKIE)
        .ok_or_else(|| {
            SeitenFehler::ungueltig(Paar::neu(
                "Anmeldecookie fehlt (anderer Browser?).",
                "the sign-in cookie is missing (a different browser?).",
            ))
        })?;
    let jetzt = zustand.uhr.jetzt();
    let genommen = db(zustand, {
        let hash = hilfen::hash(&state);
        move |conn| speicher::oidc_nehmen(conn, &hash, jetzt)
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?
    .ok_or_else(|| {
        SeitenFehler::ungueltig(Paar::neu(
            "state ist unbekannt, abgelaufen oder schon benutzt.",
            "state is unknown, expired or already used.",
        ))
    })?;
    let (zustand_row, gueltig) = genommen;
    if !gueltig {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "Der Anmeldevorgang ist abgelaufen.",
            "The sign-in attempt has expired.",
        )));
    }
    if !hilfen::gleich_konstant(&zustand_row.cookie_hash, &hilfen::hash(&cookie)) {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "state gehört nicht zu diesem Browser.",
            "state does not belong to this browser.",
        )));
    }

    let code = abfrage
        .get("code")
        .filter(|wert| !wert.is_empty())
        .ok_or_else(|| SeitenFehler::ungueltig(Paar::neu("code fehlt.", "code is missing.")))?;
    let Some(oidc) = zustand.oidc.as_ref() else {
        return Err(SeitenFehler::nicht_gefunden(Paar::neu(
            "Entra ist nicht konfiguriert.",
            "Entra is not configured.",
        )));
    };
    let (identitaet, tokens) = oidc
        .code_einloesen(code, &zustand_row.verifier, &zustand_row.nonce, jetzt)
        .await
        .map_err(|fehler| {
            SeitenFehler::intern(Paar::neu(
                format!("Anmeldung abgelehnt: {fehler}"),
                format!("Sign-in rejected: {fehler}"),
            ))
        })?;

    // Refresh-Token verschlüsselt ablegen; AAD bindet es an (tid, oid).
    let access_token = tokens.access_token.clone();
    let token_verschluesselt = match tokens.refresh_token.as_deref() {
        Some(refresh) => Some(
            krypto::verschluesseln(
                &zustand.schluessel,
                &krypto::aad(&identitaet.tid, &identitaet.oid),
                refresh.as_bytes(),
            )
            .map_err(|fehler| {
                SeitenFehler::intern(Paar::neu(
                    format!("Token nicht speicherbar: {fehler}"),
                    format!("token cannot be stored: {fehler}"),
                ))
            })?,
        ),
        None => None,
    };

    db(zustand, {
        let identitaet = identitaet.clone();
        let token = token_verschluesselt;
        move |conn| {
            // Erst die gesehenen Entra-Ids als quiver-Gruppen (einmalig,
            // ohne Sätze), dann den Benutzer, dann die Geräte neu rechnen.
            speicher::entra_gruppen_merken_und_spur(
                conn,
                &identitaet.gruppen,
                jetzt,
                "Anmeldung",
                identitaet.email.as_deref().unwrap_or(&identitaet.oid),
            )?;
            speicher::benutzer_anmelden(
                conn,
                &identitaet.oid,
                &identitaet.tid,
                identitaet.email.as_deref(),
                identitaet.name.as_deref(),
                &identitaet.gruppen,
                token.as_deref(),
                jetzt,
            )?;
            // Mitgliedschaften pflegt nur Entra: auch das Entfernen wirkt
            // sofort auf die verbundenen Geräte (Aufgabe 23).
            speicher::geraete_gruppen_neu(conn, &identitaet.oid, &identitaet.gruppen)?;
            speicher::pruefspur_schreiben(
                conn,
                jetzt,
                "anmeldung",
                identitaet.email.as_deref().unwrap_or(&identitaet.oid),
                "Entra",
            )?;
            Ok(())
        }
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?;
    // Die Geräte sollen den neuen Soll-Zustand sofort sehen.
    zustand.melde_aenderung();
    // Gruppennamen im Hintergrund (Aufgabe 30): hält die Anmeldung nie auf,
    // und ein Fehler dort ändert hier nichts.
    crate::graph::starte(zustand, &identitaet.gruppen, access_token);

    if zustand_row.modus == "admin" {
        return verwaltungs_sitzung(zustand, &identitaet, jetzt).await;
    }
    bestaetigungsseite(
        zustand,
        &identitaet,
        &zustand_row,
        jetzt,
        seiten::sprache(headers),
    )
    .await
}

/// Nach der Entra-Anmeldung: Sitzung für die Verwaltung anlegen.
async fn verwaltungs_sitzung(
    zustand: &Arc<Zustand>,
    identitaet: &Identitaet,
    jetzt: OffsetDateTime,
) -> Result<Response, SeitenFehler> {
    let oid = identitaet.oid.clone();
    let benutzer = db(zustand, move |conn| speicher::benutzer_holen(conn, &oid))
        .await
        .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?
        .ok_or_else(|| {
            SeitenFehler::intern(Paar::neu(
                "Benutzer ist nach der Anmeldung verschwunden",
                "the user vanished after sign-in",
            ))
        })?;
    if benutzer.gesperrt {
        return Err(SeitenFehler::verboten(Paar::neu(
            "Dieses Konto ist gesperrt.",
            "This account is blocked.",
        )));
    }
    if !benutzer.rolle.darf_verwalten() {
        return Err(SeitenFehler::verboten(Paar::neu(
            "Dieses Konto hat keine Verwaltungsrolle (admin oder root).",
            "This account has no administration role (admin or root).",
        )));
    }
    let kennung = identitaet.oid.clone();
    let token = db(zustand, move |conn| {
        let token = sitzung::sitzung_anlegen(conn, &kennung, jetzt)?;
        speicher::pruefspur_schreiben(conn, jetzt, "anmeldung", &kennung, "Entra (Verwaltung)")?;
        Ok(token)
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?;

    let mut antwort = weiter(
        StatusCode::SEE_OTHER,
        "/",
        Some(sitzung::sitzung_cookie(&token, zustand.konfig.ist_https())),
    );
    if let Ok(wert) = HeaderValue::from_str(&sitzung::loesch_cookie(
        sitzung::OIDC_COOKIE,
        "/auth",
        zustand.konfig.ist_https(),
    )) {
        antwort.headers_mut().append(header::SET_COOKIE, wert);
    }
    Ok(antwort)
}

async fn bestaetigungsseite(
    zustand: &Arc<Zustand>,
    identitaet: &Identitaet,
    zustand_row: &OidcZustand,
    jetzt: OffsetDateTime,
    sprache: Sprache,
) -> Result<Response, SeitenFehler> {
    let token = krypto::zufall_b64(32);
    let csrf = krypto::zufall_b64(32);
    let cookie = krypto::zufall_b64(32);
    let gueltig_bis = hilfen::als_text(jetzt + time::Duration::minutes(ZUSTAND_MINUTEN));

    db(zustand, {
        let token = token.clone();
        let csrf = csrf.clone();
        let cookie = cookie.clone();
        let identitaet = identitaet.clone();
        let zustand_row = zustand_row.clone();
        move |conn| {
            speicher::bestaetigung_anlegen(
                conn,
                &hilfen::hash(&token),
                &Bestaetigung {
                    cookie_hash: hilfen::hash(&cookie),
                    csrf,
                    oid: identitaet.oid.clone(),
                    tid: identitaet.tid.clone(),
                    email: identitaet.email.clone(),
                    name: identitaet.name.clone(),
                    entra: identitaet.gruppen.clone(),
                    challenge: zustand_row.challenge.clone(),
                    geraete_name: zustand_row.name.clone(),
                    port: zustand_row.port,
                    geraetezustand: zustand_row.geraetezustand.clone(),
                    gueltig_bis,
                },
            )?;
            Ok(())
        }
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?;

    let konto = identitaet
        .email
        .clone()
        .or_else(|| identitaet.name.clone())
        .unwrap_or_else(|| String::from(sprache.waehle("unbekanntes Konto", "unknown account")));
    let (titel, text, konto_wort, geraet_wort, server_wort, anmelden, abbrechen) = match sprache {
        Sprache::De => (
            "Gerät anmelden",
            "Bitte bestätigen: dieses Gerät für dieses Konto an diesem Server anmelden.",
            "Konto",
            "Gerät",
            "Server",
            "Gerät anmelden",
            "Abbrechen",
        ),
        Sprache::En => (
            "Sign in this device",
            "Please confirm: sign this device in for this account on this server.",
            "Account",
            "Device",
            "Server",
            "Sign in device",
            "Cancel",
        ),
    };
    let inhalt = format!(
        "<p>{text}</p>\
         <div class=\"seite__karte\">\
         <dl><dt>{konto_wort}</dt><dd>{konto}</dd>\
         <dt>{geraet_wort}</dt><dd>{geraet}</dd>\
         <dt>{server_wort}</dt><dd>{server}</dd></dl>\
         <form method=\"post\" action=\"/auth/geraet/bestaetigen\">\
         <input type=\"hidden\" name=\"token\" value=\"{token}\">\
         <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
         <button type=\"submit\">{anmelden}</button></form>\
         <form method=\"post\" action=\"/auth/geraet/abbrechen\">\
         <input type=\"hidden\" name=\"token\" value=\"{token}\">\
         <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
         <button type=\"submit\" data-ruhig>{abbrechen}</button></form>\
         </div>",
        text = seiten::html(text),
        konto_wort = seiten::html(konto_wort),
        geraet_wort = seiten::html(geraet_wort),
        server_wort = seiten::html(server_wort),
        konto = seiten::html(&konto),
        geraet = seiten::html(&zustand_row.name),
        server = seiten::html(&zustand.konfig.oeffentlich),
        token = seiten::html(&token),
        csrf = seiten::html(&csrf),
        anmelden = seiten::html(anmelden),
        abbrechen = seiten::html(abbrechen),
    );
    // Nur diese Seite darf ein Formular an genau diesen Loopback-Port schicken
    // (Aufgabe 26, Punkt 3): Chrome prüft auch die Weiterleitung nach dem
    // Absenden gegen `form-action`.
    let ziel = format!("http://127.0.0.1:{}", zustand_row.port);
    let csp = seiten::csp_mit_form_action(&[ziel]);
    let mut antwort = seiten::seite(StatusCode::OK, sprache, titel, &inhalt, Some(&csp));
    let set = sitzung::kurz_cookie(
        sitzung::BESTAETIGUNG_COOKIE,
        &cookie,
        zustand.konfig.ist_https(),
        ZUSTAND_MINUTEN * 60,
    );
    if let Ok(wert) = HeaderValue::from_str(&set) {
        antwort.headers_mut().insert(header::SET_COOKIE, wert);
    }
    Ok(antwort)
}

/// Eine wartende Bestätigung übernehmen (einmalig, an Cookie und CSRF gebunden).
async fn bestaetigung_nehmen(
    zustand: &Arc<Zustand>,
    headers: &HeaderMap,
    abfrage: &HashMap<String, String>,
) -> Result<Bestaetigung, SeitenFehler> {
    sitzung::origin_pruefen(headers, &zustand.konfig.oeffentlich)
        .map_err(|fehler| SeitenFehler::verboten(Paar::technisch(fehler.text)))?;
    let token = abfrage.get("token").cloned().unwrap_or_default();
    let csrf = abfrage.get("csrf").cloned().unwrap_or_default();
    let cookie = sitzung::cookies_aus(headers)
        .remove(sitzung::BESTAETIGUNG_COOKIE)
        .ok_or_else(|| {
            SeitenFehler::ungueltig(Paar::neu(
                "Bestätigungscookie fehlt.",
                "the confirmation cookie is missing.",
            ))
        })?;
    if token.is_empty() || csrf.is_empty() {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "CSRF-Token fehlt.",
            "the CSRF token is missing.",
        )));
    }
    let jetzt = zustand.uhr.jetzt();
    let genommen = db(zustand, {
        let hash = hilfen::hash(&token);
        move |conn| speicher::bestaetigung_nehmen(conn, &hash, jetzt)
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?
    .ok_or_else(|| {
        SeitenFehler::ungueltig(Paar::neu(
            "Bestätigung abgelaufen oder schon benutzt.",
            "The confirmation has expired or was already used.",
        ))
    })?;
    if !hilfen::gleich_konstant(&genommen.cookie_hash, &hilfen::hash(&cookie)) {
        return Err(SeitenFehler::ungueltig(Paar::neu(
            "Bestätigung gehört nicht zu diesem Browser.",
            "The confirmation does not belong to this browser.",
        )));
    }
    if !hilfen::gleich_konstant(&genommen.csrf, &csrf) {
        return Err(SeitenFehler::verboten(Paar::neu(
            "CSRF-Token passt nicht.",
            "the CSRF token does not match.",
        )));
    }
    Ok(genommen)
}

/// `POST /auth/geraet/bestaetigen` (Formular der Bestätigungsseite).
pub(crate) async fn geraet_bestaetigen(
    State(zustand): State<Arc<Zustand>>,
    headers: HeaderMap,
    rumpf: axum::body::Bytes,
) -> Response {
    let sprache = seiten::sprache(&headers);
    let abfrage = formular(&rumpf);
    match bestaetigen(&zustand, &headers, &abfrage).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(sprache),
    }
}

async fn bestaetigen(
    zustand: &Arc<Zustand>,
    headers: &HeaderMap,
    abfrage: &HashMap<String, String>,
) -> Result<Response, SeitenFehler> {
    let b = bestaetigung_nehmen(zustand, headers, abfrage).await?;
    let jetzt = zustand.uhr.jetzt();

    // Der Benutzer muss weiter gültig sein (nicht gesperrt, Token gültig,
    // mindestens eine zugeordnete Gruppe).
    let stand = db(zustand, {
        let oid = b.oid.clone();
        move |conn| {
            let Some(benutzer) = speicher::benutzer_holen(conn, &oid)? else {
                return Ok(Stand::NeuAnmelden);
            };
            if benutzer.gesperrt {
                return Ok(Stand::Gesperrt);
            }
            if benutzer.token_stand != "gueltig" {
                return Ok(Stand::NeuAnmelden);
            }
            let wirksam = speicher::wirksame_gruppen(conn, Some(&oid), &[], &benutzer.entra)?;
            if wirksam.is_empty() {
                return Ok(Stand::KeinZugriff);
            }
            Ok(Stand::Erlaubt)
        }
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?;
    match stand {
        Stand::Erlaubt => {}
        Stand::Gesperrt => {
            return Err(SeitenFehler::verboten(Paar::neu(
                "Dieses Konto ist gesperrt.",
                "This account is blocked.",
            )))
        }
        Stand::NeuAnmelden => {
            return Err(SeitenFehler::verboten(Paar::neu(
                "Die Anmeldung gilt nicht mehr; bitte neu anmelden.",
                "The sign-in is no longer valid; please sign in again.",
            )))
        }
        Stand::KeinZugriff => {
            return Err(SeitenFehler::verboten(Paar::neu(
                "Diesem Konto ist keine Gruppe zugeordnet; es gibt daher keine Tools.",
                "No group is assigned to this account; there are no tools for it.",
            )))
        }
    }

    let code = krypto::zufall_b64(32);
    db(zustand, {
        let b = b.clone();
        let code = code.clone();
        move |conn| {
            speicher::geraete_code_anlegen(
                conn,
                &hilfen::hash(&code),
                &GeraeteCode {
                    oid: b.oid.clone(),
                    tid: b.tid.clone(),
                    entra: b.entra.clone(),
                    challenge: b.challenge.clone(),
                    name: b.geraete_name.clone(),
                    gueltig_bis: hilfen::als_text(jetzt + time::Duration::seconds(CODE_SEKUNDEN)),
                },
            )?;
            speicher::pruefspur_schreiben(
                conn,
                jetzt,
                "geraet",
                b.email.as_deref().unwrap_or(&b.oid),
                "Gerät bestätigt",
            )?;
            Ok(())
        }
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(Paar::technisch(fehler.text)))?;

    let ziel = ziel_url(b.port, Some(&code), None, &b.geraetezustand);
    Ok(weiter(
        StatusCode::SEE_OTHER,
        &ziel,
        Some(sitzung::loesch_cookie(
            sitzung::BESTAETIGUNG_COOKIE,
            "/auth",
            zustand.konfig.ist_https(),
        )),
    ))
}

/// `POST /auth/geraet/abbrechen`
pub(crate) async fn geraet_abbrechen(
    State(zustand): State<Arc<Zustand>>,
    headers: HeaderMap,
    rumpf: axum::body::Bytes,
) -> Response {
    let sprache = seiten::sprache(&headers);
    let abfrage = formular(&rumpf);
    match abbrechen(&zustand, &headers, &abfrage).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(sprache),
    }
}

async fn abbrechen(
    zustand: &Arc<Zustand>,
    headers: &HeaderMap,
    abfrage: &HashMap<String, String>,
) -> Result<Response, SeitenFehler> {
    let b = bestaetigung_nehmen(zustand, headers, abfrage).await?;
    let ziel = ziel_url(b.port, None, Some("abgebrochen"), &b.geraetezustand);
    Ok(weiter(
        StatusCode::SEE_OTHER,
        &ziel,
        Some(sitzung::loesch_cookie(
            sitzung::BESTAETIGUNG_COOKIE,
            "/auth",
            zustand.konfig.ist_https(),
        )),
    ))
}

/// Genau `http://127.0.0.1:<port>/quiver` mit Code **oder** Fehler und `zustand`.
fn ziel_url(port: u16, code: Option<&str>, fehler: Option<&str>, zustand: &str) -> String {
    let mut paare = form_urlencoded::Serializer::new(String::new());
    if let Some(code) = code {
        paare.append_pair("code", code);
    }
    if let Some(fehler) = fehler {
        paare.append_pair("fehler", fehler);
    }
    paare.append_pair("zustand", zustand);
    format!("http://127.0.0.1:{port}/quiver?{}", paare.finish())
}

// ------------------------------------------------------------------ Tausch

/// `POST /auth/geraet/tausch`: Code plus Verifier gegen ein Geräte-Token.
pub(crate) async fn geraet_tausch(
    State(zustand): State<Arc<Zustand>>,
    Koerper(anfrage): Koerper<GeraeteTausch>,
) -> Ergebnis<Json<AnmeldenAntwort>> {
    if !pkce_gueltig(&anfrage.verifier) {
        return Err(FehlerAntwort::ungueltig(
            "verifier muss ein PKCE-Verifier sein (43 Zeichen base64url)",
        ));
    }
    let challenge = s256(&anfrage.verifier);
    let jetzt = zustand.uhr.jetzt();
    let altes_hash = anfrage
        .altes_token
        .as_deref()
        .map(str::trim)
        .filter(|wert| !wert.is_empty())
        .map(hilfen::hash);

    // 1. Code verbrauchen (auch bei falschem Verifier).
    let code_hash = hilfen::hash(&anfrage.code);
    let genommen = db(&zustand, move |conn| {
        speicher::geraete_code_nehmen(conn, &code_hash, jetzt)
    })
    .await?
    .ok_or_else(|| FehlerAntwort::ungueltig("Code unbekannt oder schon benutzt"))?;
    let (code, gueltig) = genommen;
    if !gueltig {
        return Err(FehlerAntwort::abgelaufen("Code ist abgelaufen"));
    }
    if !hilfen::gleich_konstant(&code.challenge, &challenge) {
        return Err(FehlerAntwort::ungueltig(
            "verifier passt nicht zum Challenge",
        ));
    }

    // 2. Beim Verbinden nach mehr als 60 Minuten nachprüfen.
    if let Err(grund) = crate::refresh::bei_verbindung(&zustand, &code.oid).await {
        return Err(grund_fehler(grund));
    }

    // 3. Benutzer prüfen, Gruppen rechnen, Gerät anlegen oder weiterführen.
    let token = hilfen::token_erzeugen();
    let token_hash = hilfen::hash(&token);
    let name = code.name.clone();
    let oid = code.oid.clone();
    let tid = code.tid.clone();
    let jetzt_text = hilfen::als_text(jetzt);
    let ergebnis = db(&zustand, {
        let oid = oid.clone();
        move |conn| {
            let Some(benutzer) = speicher::benutzer_holen(conn, &oid)? else {
                return Ok(Err(AnmeldeGrund::NeuAnmelden));
            };
            if benutzer.gesperrt {
                return Ok(Err(AnmeldeGrund::Gesperrt));
            }
            if benutzer.token_stand != "gueltig" {
                return Ok(Err(AnmeldeGrund::NeuAnmelden));
            }
            let alt = match &altes_hash {
                Some(hash) => speicher::geraet_per_hash(conn, hash)?,
                None => None,
            };
            if let Some(alt) = &alt {
                if alt.geraet.gesperrt {
                    return Ok(Err(AnmeldeGrund::Gesperrt));
                }
                if alt.oid.as_deref().is_some_and(|alt_oid| alt_oid != oid) {
                    // Das alte Token gehört einem anderen Benutzer: neues Gerät.
                    return neu_anlegen(conn, &oid, &tid, &name, &token_hash, &benutzer, jetzt);
                }
                let wirksam =
                    speicher::wirksame_gruppen(conn, Some(&oid), &alt.manuell, &benutzer.entra)?;
                if wirksam.is_empty() {
                    return Ok(Err(AnmeldeGrund::KeinZugriff));
                }
                let mut geraet = alt.geraet.clone();
                geraet.name = name.clone();
                geraet.gruppen = wirksam;
                speicher::erneuere_geraet(
                    conn,
                    geraet.id.als_str(),
                    &token_hash,
                    &oid,
                    &tid,
                    &geraet,
                )?;
                speicher::setze_gesehen(conn, geraet.id.als_str(), &jetzt_text)?;
                speicher::pruefspur_schreiben(
                    conn,
                    jetzt,
                    "geraet",
                    benutzer.email.as_deref().unwrap_or(&oid),
                    &format!("Gerät {} weitergeführt", geraet.id),
                )?;
                return Ok(Ok(geraet));
            }
            neu_anlegen(conn, &oid, &tid, &name, &token_hash, &benutzer, jetzt)
        }
    })
    .await?;

    match ergebnis {
        Ok(geraet) => {
            zustand.melde_aenderung();
            let benutzer = benutzer_anzeige(&zustand, &oid).await;
            Ok(Json(AnmeldenAntwort {
                geraet,
                token,
                benutzer,
            }))
        }
        Err(grund) => Err(grund_fehler(grund)),
    }
}

/// Anzeigename des Benutzers für „Angemeldet als …“: E-Mail, sonst Name.
/// Kein Schlüssel und kein Geheimnis; fehlt beides, bleibt das Feld leer.
async fn benutzer_anzeige(zustand: &Arc<Zustand>, oid: &str) -> Option<String> {
    let oid = oid.to_string();
    db(zustand, move |conn| {
        Ok(speicher::benutzer_holen(conn, &oid)?
            .and_then(|benutzer| benutzer.email.or(benutzer.name)))
    })
    .await
    .ok()
    .flatten()
}

fn neu_anlegen(
    conn: &rusqlite::Connection,
    oid: &str,
    tid: &str,
    name: &str,
    token_hash: &str,
    benutzer: &speicher::Benutzer,
    jetzt: OffsetDateTime,
) -> anyhow::Result<Result<Geraet, AnmeldeGrund>> {
    let wirksam = speicher::wirksame_gruppen(conn, Some(oid), &[], &benutzer.entra)?;
    if wirksam.is_empty() {
        return Ok(Err(AnmeldeGrund::KeinZugriff));
    }
    let geraet = Geraet {
        id: GeraetId::neu(hilfen::geraet_id_erzeugen()),
        name: name.to_string(),
        gruppen: wirksam,
        gesperrt: false,
    };
    speicher::lege_geraet_an(conn, &geraet, token_hash, Some(oid), Some(tid), Some(&[]))?;
    speicher::pruefspur_schreiben(
        conn,
        jetzt,
        "geraet",
        benutzer.email.as_deref().unwrap_or(oid),
        &format!("Gerät {} eingebunden", geraet.id),
    )?;
    Ok(Ok(geraet))
}

fn grund_fehler(grund: AnmeldeGrund) -> FehlerAntwort {
    let text = match grund {
        AnmeldeGrund::Gesperrt => "Konto oder Gerät ist gesperrt",
        AnmeldeGrund::NeuAnmelden => "Anmeldung gilt nicht mehr; bitte neu anmelden",
        AnmeldeGrund::KeinZugriff => "dem Konto ist keine Gruppe zugeordnet",
    };
    FehlerAntwort::mit_grund(StatusCode::FORBIDDEN, text, grund)
}

enum Stand {
    Erlaubt,
    Gesperrt,
    NeuAnmelden,
    KeinZugriff,
}

/// Nur für Tests: ein PKCE-Paar (Verifier, Challenge) bauen.
#[cfg(test)]
pub(crate) fn pkce_paar() -> (String, String) {
    let verifier = krypto::zufall_b64(32);
    let challenge = s256(&verifier);
    (verifier, challenge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_und_zustand() {
        let (verifier, challenge) = pkce_paar();
        assert!(pkce_gueltig(&verifier));
        assert!(pkce_gueltig(&challenge));
        assert_eq!(s256(&verifier), challenge);
        assert!(!pkce_gueltig("zu-kurz"));
        assert!(!pkce_gueltig(&format!("{challenge}=")));
        assert!(geraetezustand_gueltig("abc-123_~."));
        assert!(!geraetezustand_gueltig("mit leer"));
        assert!(!geraetezustand_gueltig(""));
    }

    #[test]
    fn ziel_ist_genau_loopback() {
        let ziel = ziel_url(51234, Some("code-1"), None, "abc");
        assert_eq!(
            ziel,
            "http://127.0.0.1:51234/quiver?code=code-1&zustand=abc"
        );
        let ziel = ziel_url(51234, None, Some("abgebrochen"), "abc");
        assert_eq!(
            ziel,
            "http://127.0.0.1:51234/quiver?fehler=abgebrochen&zustand=abc"
        );
    }

    #[test]
    fn namen_und_ports() {
        assert!(name_gueltig("Mein Rechner"));
        assert!(!name_gueltig(""));
        assert!(!name_gueltig(&"x".repeat(81)));
        assert!(!name_gueltig("mit\nsteuer"));
        assert_eq!(port_gueltig("1024"), Some(1024));
        assert_eq!(port_gueltig("65535"), Some(65535));
        assert_eq!(port_gueltig("1023"), None);
        assert_eq!(port_gueltig("65536"), None);
        assert_eq!(port_gueltig("abc"), None);
    }
}
