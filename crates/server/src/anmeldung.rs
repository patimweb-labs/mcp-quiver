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

// ------------------------------------------------------------------ Seiten

struct SeitenFehler {
    status: StatusCode,
    titel: &'static str,
    text: String,
}

impl SeitenFehler {
    fn ungueltig(text: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            titel: "Anmeldung nicht möglich",
            text: text.into(),
        }
    }

    fn verboten(text: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            titel: "Kein Zugriff",
            text: text.into(),
        }
    }

    fn intern(text: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            titel: "Anmeldung gescheitert",
            text: text.into(),
        }
    }

    fn nicht_gefunden(text: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            titel: "Nicht verfügbar",
            text: text.into(),
        }
    }

    fn seite(self) -> Response {
        seite(
            self.status,
            self.titel,
            &format!("<p>{}</p>", html(&self.text)),
        )
    }
}

fn html(wert: &str) -> String {
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

fn seite(status: StatusCode, titel: &str, inhalt: &str) -> Response {
    let rumpf = format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{titel}</title></head><body><main><h1>{titel}</h1>{inhalt}</main></body></html>",
        titel = html(titel)
    );
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        rumpf,
    )
        .into_response()
}

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
    Query(abfrage): Query<HashMap<String, String>>,
) -> Response {
    match start(&zustand, &abfrage).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(),
    }
}

async fn start(
    zustand: &Arc<Zustand>,
    abfrage: &HashMap<String, String>,
) -> Result<Response, SeitenFehler> {
    if zustand.oidc.is_none() {
        return Err(SeitenFehler::nicht_gefunden(
            "Dieser Server nutzt Einmal-Codes statt Entra; /auth/geraet/start gibt es hier nicht.",
        ));
    }

    let challenge = abfrage.get("challenge").cloned().unwrap_or_default();
    if !pkce_gueltig(&challenge) {
        return Err(SeitenFehler::ungueltig(
            "challenge muss ein S256-Wert sein (43 Zeichen base64url).",
        ));
    }
    let port = abfrage
        .get("port")
        .and_then(|wert| port_gueltig(wert))
        .ok_or_else(|| SeitenFehler::ungueltig("port muss zwischen 1024 und 65535 liegen."))?;
    let geraetezustand = abfrage.get("zustand").cloned().unwrap_or_default();
    if !geraetezustand_gueltig(&geraetezustand) {
        return Err(SeitenFehler::ungueltig(
            "zustand fehlt oder enthält unerlaubte Zeichen (höchstens 128).",
        ));
    }
    let name = abfrage.get("name").cloned().unwrap_or_default();
    if !name_gueltig(&name) {
        return Err(SeitenFehler::ungueltig(
            "name fehlt oder ist länger als 80 Zeichen.",
        ));
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
        return Err(SeitenFehler::nicht_gefunden(
            "Entra ist auf diesem Server nicht eingerichtet.",
        ));
    };
    let jetzt = zustand.uhr.jetzt();
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
            speicher::oidc_aufraeumen(conn, jetzt)?;
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
            Ok(())
        }
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?;

    let ziel = oidc
        .anmelde_url(&state, &nonce, &server_challenge)
        .await
        .map_err(|fehler| SeitenFehler::intern(format!("Entra nicht erreichbar: {fehler}")))?;
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
pub(crate) async fn admin_start(State(zustand): State<Arc<Zustand>>) -> Response {
    match starte(&zustand, "admin", "", 0, "", "").await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(),
    }
}

// ---------------------------------------------------------------- Rückkehr

/// `GET /auth/callback?code&state`
pub(crate) async fn callback(
    State(zustand): State<Arc<Zustand>>,
    Query(abfrage): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    match callback_intern(&zustand, &abfrage, &headers).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(),
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
        return Err(SeitenFehler::ungueltig(format!(
            "Entra meldet: {} {}",
            kurz(fehler),
            kurz(text)
        )));
    }

    let state = abfrage.get("state").cloned().unwrap_or_default();
    if state.is_empty() {
        return Err(SeitenFehler::ungueltig("state fehlt."));
    }
    let cookie = sitzung::cookies_aus(headers)
        .remove(sitzung::OIDC_COOKIE)
        .ok_or_else(|| SeitenFehler::ungueltig("Anmeldecookie fehlt (anderer Browser?)."))?;
    let jetzt = zustand.uhr.jetzt();
    let genommen = db(zustand, {
        let hash = hilfen::hash(&state);
        move |conn| speicher::oidc_nehmen(conn, &hash, jetzt)
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?
    .ok_or_else(|| {
        SeitenFehler::ungueltig("state ist unbekannt, abgelaufen oder schon benutzt.")
    })?;
    let (zustand_row, gueltig) = genommen;
    if !gueltig {
        return Err(SeitenFehler::ungueltig(
            "Der Anmeldevorgang ist abgelaufen.",
        ));
    }
    if !hilfen::gleich_konstant(&zustand_row.cookie_hash, &hilfen::hash(&cookie)) {
        return Err(SeitenFehler::ungueltig(
            "state gehört nicht zu diesem Browser.",
        ));
    }

    let code = abfrage
        .get("code")
        .filter(|wert| !wert.is_empty())
        .ok_or_else(|| SeitenFehler::ungueltig("code fehlt."))?;
    let Some(oidc) = zustand.oidc.as_ref() else {
        return Err(SeitenFehler::nicht_gefunden(
            "Entra ist nicht konfiguriert.",
        ));
    };
    let (identitaet, tokens) = oidc
        .code_einloesen(code, &zustand_row.verifier, &zustand_row.nonce, jetzt)
        .await
        .map_err(|fehler| SeitenFehler::intern(format!("Anmeldung abgelehnt: {fehler}")))?;

    // Refresh-Token verschlüsselt ablegen; AAD bindet es an (tid, oid).
    let token_verschluesselt = match tokens.refresh_token.as_deref() {
        Some(refresh) => Some(
            krypto::verschluesseln(
                &zustand.schluessel,
                &krypto::aad(&identitaet.tid, &identitaet.oid),
                refresh.as_bytes(),
            )
            .map_err(|fehler| SeitenFehler::intern(format!("Token nicht speicherbar: {fehler}")))?,
        ),
        None => None,
    };

    db(zustand, {
        let identitaet = identitaet.clone();
        let token = token_verschluesselt;
        move |conn| {
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
            speicher::entra_gruppen_merken(conn, &identitaet.gruppen, jetzt)?;
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
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?;

    if zustand_row.modus == "admin" {
        return verwaltungs_sitzung(zustand, &identitaet, jetzt).await;
    }
    bestaetigungsseite(zustand, &identitaet, &zustand_row, jetzt).await
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
        .map_err(|fehler| SeitenFehler::intern(fehler.text))?
        .ok_or_else(|| SeitenFehler::intern("Benutzer ist nach der Anmeldung verschwunden"))?;
    if benutzer.gesperrt {
        return Err(SeitenFehler::verboten("Dieses Konto ist gesperrt."));
    }
    if !benutzer.rolle.darf_verwalten() {
        return Err(SeitenFehler::verboten(
            "Dieses Konto hat keine Verwaltungsrolle (admin oder root).",
        ));
    }
    let kennung = identitaet.oid.clone();
    let token = db(zustand, move |conn| {
        let token = sitzung::sitzung_anlegen(conn, &kennung, jetzt)?;
        speicher::pruefspur_schreiben(conn, jetzt, "anmeldung", &kennung, "Entra (Verwaltung)")?;
        Ok(token)
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?;

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
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?;

    let konto = identitaet
        .email
        .clone()
        .or_else(|| identitaet.name.clone())
        .unwrap_or_else(|| String::from("unbekanntes Konto"));
    let inhalt = format!(
        "<p>Bitte bestätigen: dieses Gerät für dieses Konto an diesem Server anmelden.</p>\
         <dl><dt>Konto</dt><dd>{konto}</dd>\
         <dt>Gerät</dt><dd>{geraet}</dd>\
         <dt>Server</dt><dd>{server}</dd></dl>\
         <form method=\"post\" action=\"/auth/geraet/bestaetigen\">\
         <input type=\"hidden\" name=\"token\" value=\"{token}\">\
         <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
         <button type=\"submit\">Gerät anmelden</button></form>\
         <form method=\"post\" action=\"/auth/geraet/abbrechen\">\
         <input type=\"hidden\" name=\"token\" value=\"{token}\">\
         <input type=\"hidden\" name=\"csrf\" value=\"{csrf}\">\
         <button type=\"submit\">Abbrechen</button></form>",
        konto = html(&konto),
        geraet = html(&zustand_row.name),
        server = html(&zustand.konfig.oeffentlich),
        token = html(&token),
        csrf = html(&csrf),
    );
    let mut antwort = seite(StatusCode::OK, "Gerät anmelden", &inhalt);
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
        .map_err(|fehler| SeitenFehler::verboten(fehler.text))?;
    let token = abfrage.get("token").cloned().unwrap_or_default();
    let csrf = abfrage.get("csrf").cloned().unwrap_or_default();
    let cookie = sitzung::cookies_aus(headers)
        .remove(sitzung::BESTAETIGUNG_COOKIE)
        .ok_or_else(|| SeitenFehler::ungueltig("Bestätigungscookie fehlt."))?;
    if token.is_empty() || csrf.is_empty() {
        return Err(SeitenFehler::ungueltig("CSRF-Token fehlt."));
    }
    let jetzt = zustand.uhr.jetzt();
    let genommen = db(zustand, {
        let hash = hilfen::hash(&token);
        move |conn| speicher::bestaetigung_nehmen(conn, &hash, jetzt)
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?
    .ok_or_else(|| SeitenFehler::ungueltig("Bestätigung abgelaufen oder schon benutzt."))?;
    if !hilfen::gleich_konstant(&genommen.cookie_hash, &hilfen::hash(&cookie)) {
        return Err(SeitenFehler::ungueltig(
            "Bestätigung gehört nicht zu diesem Browser.",
        ));
    }
    if !hilfen::gleich_konstant(&genommen.csrf, &csrf) {
        return Err(SeitenFehler::verboten("CSRF-Token passt nicht."));
    }
    Ok(genommen)
}

/// `POST /auth/geraet/bestaetigen` (Formular der Bestätigungsseite).
pub(crate) async fn geraet_bestaetigen(
    State(zustand): State<Arc<Zustand>>,
    headers: HeaderMap,
    rumpf: axum::body::Bytes,
) -> Response {
    let abfrage = formular(&rumpf);
    match bestaetigen(&zustand, &headers, &abfrage).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(),
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
            let wirksam = speicher::wirksame_gruppen(conn, &[], &benutzer.entra)?;
            if wirksam.is_empty() {
                return Ok(Stand::KeinZugriff);
            }
            Ok(Stand::Erlaubt)
        }
    })
    .await
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?;
    match stand {
        Stand::Erlaubt => {}
        Stand::Gesperrt => return Err(SeitenFehler::verboten("Dieses Konto ist gesperrt.")),
        Stand::NeuAnmelden => {
            return Err(SeitenFehler::verboten(
                "Die Anmeldung gilt nicht mehr; bitte neu anmelden.",
            ))
        }
        Stand::KeinZugriff => {
            return Err(SeitenFehler::verboten(
                "Diesem Konto ist keine Gruppe zugeordnet; es gibt daher keine Tools.",
            ))
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
    .map_err(|fehler| SeitenFehler::intern(fehler.text))?;

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
    let abfrage = formular(&rumpf);
    match abbrechen(&zustand, &headers, &abfrage).await {
        Ok(antwort) => antwort,
        Err(fehler) => fehler.seite(),
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
                let wirksam = speicher::wirksame_gruppen(conn, &alt.manuell, &benutzer.entra)?;
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
            Ok(Json(AnmeldenAntwort { geraet, token }))
        }
        Err(grund) => Err(grund_fehler(grund)),
    }
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
    let wirksam = speicher::wirksame_gruppen(conn, &[], &benutzer.entra)?;
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
    fn html_wird_escaped() {
        assert_eq!(
            html("<script>\"x\"&'</script>"),
            "&lt;script&gt;&quot;x&quot;&amp;&#39;&lt;/script&gt;"
        );
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
