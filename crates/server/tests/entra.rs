// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahmetests für Aufgabe 20: Entra-Anmeldung, Root, Sitzungen, Prüfspur.
//!
//! Gegen den eigenen Entra-Nachbau (axum, eigener RSA-Schlüssel, Loopback,
//! Port 0). Kein echter Mandant, keine Firmendaten, keine Geheimnisse.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Query, State as AxState};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use http_body_util::BodyExt;
use jsonwebtoken::{
    decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use quiver_domain::{
    Eintrag, EintragId, Feld, Herkunft, Laufzeit, Quelle, Satz, SatzEintrag, SatzId, Werte,
};
use quiver_protocol::{
    AnmeldenAntwort, BenutzerAendern, BenutzerSicht, CodeAnlegen, GeraeteTausch,
    GeraeteTausch as Tausch, Rolle,
};
use quiver_server::entra_schluessel::{erzeugen, SchluesselHalter, SCHLUESSEL_DATEI};
use quiver_server::GRAPH_BASIS;
use quiver_server::{app, benutzer, Ausweis, EntraKonfig, Konfig, Uhr};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::traits::PublicKeyParts;
use rsa::PrivateKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tower::ServiceExt;
use x509_cert::der::{DecodePem, Encode};

const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const OID: &str = "99999999-8888-7777-6666-555555555555";
const GRUPPE_ENTRA: &str = "12345678-1234-1234-1234-123456789012";
const GRUPPE_NEU: &str = "abcdefab-1234-1234-1234-123456789012";
const ROOT_EMAIL: &str = "root@example.test";
const ROOT_PASSWORT: &str = "ein-sehr-langes-testpasswort";
const ENTRA_SECRET: &str = "kein-echtes-geheimnis-nur-test";
const OEFFENTLICH: &str = "http://127.0.0.1:8740";
/// Der `Host`-Kopf, der zu [`OEFFENTLICH`] passt (Port gehört dazu).
const HOST: &str = "127.0.0.1:8740";

/// Höchstens zwei Argon2-Prüfungen laufen serverseitig gleichzeitig; damit
/// parallele Tests nicht absichtlich 429 bekommen, melden sie sich nacheinander an.
static ROOT_SPERRE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// --------------------------------------------------------------- Entra-Nachbau

#[derive(Clone)]
struct Mock {
    basis: String,
    kid: String,
    key: Arc<PrivateKey>,
    uhr: Uhr,
    zustand: Arc<Mutex<MockZustand>>,
}

struct MockZustand {
    codes: HashMap<String, CodeInfo>,
    naechster_code: usize,
    discovery_hits: usize,
    discovery_fehler: bool,
    jwks_hits: usize,
    token_hits: usize,
    refresh_hits: usize,
    claims: Value,
    refresh_claims: Value,
    sign_with: Option<Arc<PrivateKey>>,
    token_fehler: Option<(u16, &'static str)>,
    refresh_fehler: Option<&'static str>,
    verzoegerung_ms: u64,
    benutzte_refresh: HashSet<String>,
    /// Zertifikat für `private_key_jwt`; `None` heißt: Geheimnis.
    zertifikat: Option<String>,
    /// Jede `jti` darf nur einmal vorkommen.
    jtis: HashSet<String>,
    assertionen: usize,
    assertion_fehler: Option<String>,
    /// Die `scope`-Werte der Anmelde-URLs (Aufgabe 30).
    scopes: Vec<String>,
    /// Die `scope`-Werte der Refresh-Formulare.
    refresh_scopes: Vec<String>,
    /// Welche Ids bei Graph abgefragt wurden.
    graph_hits: Vec<String>,
    /// Welche Zugriffstoken bei Graph ankamen.
    graph_tokens: Vec<String>,
    /// Wie Graph antwortet.
    graph: GraphVerhalten,
}

struct CodeInfo {
    challenge: String,
    nonce: String,
    claims: Value,
}

/// Was der Graph-Nachbau bei `GET /v1.0/groups/{id}` tut (Aufgabe 30).
#[derive(Debug, Clone, PartialEq, Eq)]
enum GraphVerhalten {
    /// Antwortet mit genau diesem Anzeigenamen.
    Name(String),
    /// Antwortet mit diesem HTTP-Status.
    Status(u16),
    /// Leitet weiter, statt zu antworten.
    Weiterleitung,
    /// Antwortet, aber mit einer anderen Id.
    FremdeId,
    /// Antwortet mit einer riesigen Antwort.
    Riesig,
    /// Antwortet mit Steuer- und Richtungszeichen im Namen.
    BoeserName,
    /// Antwortet erst nach dem Zeitlimit.
    Langsam,
}

/// Das Zugriffstoken, das der Entra-Nachbau ausgibt (nur Testdaten).
const GRAPH_TOKEN: &str = "zugriffstoken-nur-fuer-den-test";

fn graph_verhalten_default() -> GraphVerhalten {
    GraphVerhalten::Name(String::from("Team Alpha"))
}

fn sign(key: &PrivateKey, kid: &str, claims: &Value) -> String {
    let mut kopf = Header::new(Algorithm::RS256);
    kopf.kid = Some(kid.to_string());
    let pem = key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).expect("PEM");
    let schluessel = EncodingKey::from_rsa_pem(pem.as_bytes()).expect("Signierschlüssel");
    encode(&kopf, claims, &schluessel).expect("JWT")
}

fn jwks(key: &PrivateKey, kid: &str) -> Value {
    json!({"keys": [{
        "kty": "RSA",
        "kid": kid,
        "use": "sig",
        "alg": "RS256",
        "n": URL_SAFE_NO_PAD.encode(key.n().to_be_bytes_trimmed_vartime()),
        "e": URL_SAFE_NO_PAD.encode(key.e().to_be_bytes_trimmed_vartime()),
    }]})
}

fn standard_claims(basis: &str, uhr: &Uhr, oid: &str, gruppen: &[&str]) -> Value {
    let jetzt = uhr.jetzt().unix_timestamp();
    json!({
        "iss": basis,
        "aud": CLIENT,
        "tid": TENANT,
        "oid": oid,
        "exp": jetzt + 3600,
        "nbf": jetzt - 10,
        "iat": jetzt - 10,
        "preferred_username": "test@example.test",
        "name": "Test Person",
        "groups": gruppen,
    })
}

impl Mock {
    async fn start(uhr: Uhr) -> Self {
        let mut rng = getrandom::SysRng;
        let key = Arc::new(PrivateKey::new(&mut rng, 2048).expect("RSA"));
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let adresse = listener.local_addr().expect("Adresse");
        let basis = format!("http://{adresse}");
        let claims = standard_claims(&basis, &uhr, OID, &[GRUPPE_ENTRA]);
        let mock = Mock {
            basis,
            kid: String::from("kid-1"),
            key,
            uhr,
            zustand: Arc::new(Mutex::new(MockZustand {
                codes: HashMap::new(),
                naechster_code: 0,
                discovery_hits: 0,
                discovery_fehler: false,
                jwks_hits: 0,
                token_hits: 0,
                refresh_hits: 0,
                claims: claims.clone(),
                refresh_claims: claims,
                sign_with: None,
                token_fehler: None,
                refresh_fehler: None,
                verzoegerung_ms: 0,
                benutzte_refresh: HashSet::new(),
                zertifikat: None,
                jtis: HashSet::new(),
                assertionen: 0,
                assertion_fehler: None,
                scopes: Vec::new(),
                refresh_scopes: Vec::new(),
                graph_hits: Vec::new(),
                graph_tokens: Vec::new(),
                graph: graph_verhalten_default(),
            })),
        };
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .route("/keys", get(keys))
            .route("/v1.0/groups/{id}", get(graph_gruppe))
            .with_state(mock.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        mock
    }

    fn setze_claims(&self, aenderung: impl FnOnce(&mut Value)) {
        let mut z = self.zustand.lock().unwrap();
        aenderung(&mut z.claims);
    }

    fn setze_refresh_claims(&self, aenderung: impl FnOnce(&mut Value)) {
        let mut z = self.zustand.lock().unwrap();
        aenderung(&mut z.refresh_claims);
    }

    fn token_fehler(&self, status: u16) {
        self.zustand.lock().unwrap().token_fehler = Some((status, "temporarily_unavailable"));
    }

    fn refresh_fehler(&self, code: &'static str) {
        self.zustand.lock().unwrap().refresh_fehler = Some(code);
    }

    fn falscher_schluessel(&self) {
        let mut rng = getrandom::SysRng;
        let fremd = Arc::new(PrivateKey::new(&mut rng, 2048).expect("RSA"));
        self.zustand.lock().unwrap().sign_with = Some(fremd);
    }

    fn verzoegern(&self, ms: u64) {
        self.zustand.lock().unwrap().verzoegerung_ms = ms;
    }

    fn setze_graph(&self, verhalten: GraphVerhalten) {
        self.zustand.lock().unwrap().graph = verhalten;
    }

    /// Die bei Graph abgefragten Ids.
    fn graph_ids(&self) -> Vec<String> {
        self.zustand.lock().unwrap().graph_hits.clone()
    }

    /// Die bei Graph angekommenen Zugriffstoken.
    fn graph_tokens(&self) -> Vec<String> {
        self.zustand.lock().unwrap().graph_tokens.clone()
    }

    /// Die `scope`-Werte der Browser-Anmeldungen.
    fn scopes(&self) -> Vec<String> {
        self.zustand.lock().unwrap().scopes.clone()
    }

    /// Die `scope`-Werte der Refresh-Aufrufe.
    fn refresh_scopes(&self) -> Vec<String> {
        self.zustand.lock().unwrap().refresh_scopes.clone()
    }

    fn zaehler(&self) -> (usize, usize, usize) {
        let z = self.zustand.lock().unwrap();
        (z.jwks_hits, z.token_hits, z.refresh_hits)
    }

    /// Wie oft die Discovery geholt wurde (Aufgabe 26, Punkt 6).
    fn discovery_zaehler(&self) -> usize {
        self.zustand.lock().unwrap().discovery_hits
    }

    /// Die Discovery unerreichbar machen (Netzfehler).
    fn discovery_aus(&self, aus: bool) {
        self.zustand.lock().unwrap().discovery_fehler = aus;
    }

    /// Das öffentliche Zertifikat hinterlegen; ab jetzt wird eine
    /// Client-Assertion verlangt und streng geprüft (Aufgabe 23).
    fn setze_zertifikat(&self, pem: &str) {
        self.zustand.lock().unwrap().zertifikat = Some(pem.to_string());
    }

    fn assertionen(&self) -> usize {
        self.zustand.lock().unwrap().assertionen
    }

    fn assertion_fehler(&self) -> Option<String> {
        self.zustand.lock().unwrap().assertion_fehler.clone()
    }
}

/// Prüft den Ausweis am Token-Endpunkt. Mit Zertifikat ist eine
/// `private_key_jwt`-Assertion Pflicht und wird nach allen Regeln geprüft
/// (Signatur gegen das Zertifikat, PS256, Felder, `jti` einmal, Ablauf).
fn pruefe_client(m: &Mock, form: &HashMap<String, String>, jetzt: i64) -> Result<(), String> {
    let zertifikat = m.zustand.lock().unwrap().zertifikat.clone();
    match zertifikat {
        Some(pem) => assertion_pruefen(m, form, jetzt, &pem),
        None => {
            if form.get("client_secret").map(String::as_str) == Some(ENTRA_SECRET) {
                Ok(())
            } else {
                Err(String::from("client_secret fehlt oder ist falsch"))
            }
        }
    }
}

fn assertion_pruefen(
    m: &Mock,
    form: &HashMap<String, String>,
    jetzt: i64,
    zertifikat_pem: &str,
) -> Result<(), String> {
    if form.get("client_assertion_type").map(String::as_str)
        != Some("urn:ietf:params:oauth:client-assertion-type:jwt-bearer")
    {
        return Err(String::from("client_assertion_type fehlt"));
    }
    let token = form
        .get("client_assertion")
        .ok_or_else(|| String::from("client_assertion fehlt"))?;
    let kopf = decode_header(token).map_err(|fehler| format!("Kopf: {fehler}"))?;
    if kopf.alg != Algorithm::PS256 {
        return Err(format!("Algorithmus ist {:?}, nicht PS256", kopf.alg));
    }
    let zertifikat = x509_cert::Certificate::from_pem(zertifikat_pem.as_bytes())
        .map_err(|fehler| format!("Zertifikat: {fehler}"))?;
    let der = zertifikat
        .to_der()
        .map_err(|fehler| format!("Zertifikat als DER: {fehler}"))?;
    let erwartet = URL_SAFE_NO_PAD.encode(Sha256::digest(&der));
    if kopf.x5t_s256.as_deref() != Some(erwartet.as_str()) {
        return Err(String::from("x5t#S256 passt nicht zum Zertifikat"));
    }
    let spki = zertifikat.tbs_certificate().subject_public_key_info();
    let roh = spki
        .subject_public_key
        .as_bytes()
        .ok_or_else(|| String::from("Zertifikat ohne öffentlichen Schlüssel"))?;
    let public = <rsa::PublicKey as rsa::pkcs1::DecodeRsaPublicKey>::from_pkcs1_der(roh)
        .map_err(|fehler| format!("öffentlicher Schlüssel: {fehler}"))?;
    let oeffentlich = public
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|fehler| format!("öffentlicher Schlüssel als PEM: {fehler}"))?;
    let mut pruefung = Validation::new(Algorithm::PS256);
    pruefung.set_audience(&[format!("{}/token", m.basis)]);
    pruefung.set_issuer(&[CLIENT]);
    pruefung.set_required_spec_claims(&["aud", "iss", "sub", "exp", "nbf", "jti"]);
    let daten = decode::<Value>(
        token,
        &DecodingKey::from_rsa_pem(oeffentlich.as_bytes())
            .map_err(|fehler| format!("Prüfschlüssel: {fehler}"))?,
        &pruefung,
    )
    .map_err(|fehler| format!("Assertion abgelehnt: {fehler}"))?;
    let claims = &daten.claims;
    if claims["sub"].as_str() != Some(CLIENT) {
        return Err(String::from("sub ist nicht die Client-Id"));
    }
    if claims["iss"].as_str() != Some(CLIENT) {
        return Err(String::from("iss ist nicht die Client-Id"));
    }
    let exp = claims["exp"]
        .as_i64()
        .ok_or_else(|| String::from("exp fehlt"))?;
    if exp <= jetzt {
        return Err(String::from("Assertion ist abgelaufen"));
    }
    if exp - jetzt > 300 {
        return Err(String::from("Assertion gilt länger als 5 Minuten"));
    }
    let nbf = claims["nbf"]
        .as_i64()
        .ok_or_else(|| String::from("nbf fehlt"))?;
    if nbf > jetzt + 60 {
        return Err(String::from("nbf liegt in der Zukunft"));
    }
    let jti = claims["jti"]
        .as_str()
        .ok_or_else(|| String::from("jti fehlt"))?;
    let mut z = m.zustand.lock().unwrap();
    if !z.jtis.insert(jti.to_string()) {
        return Err(String::from("jti wurde schon benutzt"));
    }
    z.assertionen += 1;
    Ok(())
}

async fn discovery(AxState(m): AxState<Mock>) -> Response {
    {
        let mut z = m.zustand.lock().unwrap();
        z.discovery_hits += 1;
        if z.discovery_fehler {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "temporarily_unavailable"})),
            )
                .into_response();
        }
    }
    Json(json!({
        "issuer": m.basis,
        "authorization_endpoint": format!("{}/authorize", m.basis),
        "token_endpoint": format!("{}/token", m.basis),
        "jwks_uri": format!("{}/keys", m.basis),
    }))
    .into_response()
}

async fn keys(AxState(m): AxState<Mock>) -> Json<Value> {
    m.zustand.lock().unwrap().jwks_hits += 1;
    Json(jwks(&m.key, &m.kid))
}

async fn authorize(
    AxState(m): AxState<Mock>,
    Query(abfrage): Query<HashMap<String, String>>,
) -> Response {
    let mut z = m.zustand.lock().unwrap();
    z.scopes
        .push(abfrage.get("scope").cloned().unwrap_or_default());
    z.naechster_code += 1;
    let code = format!("code-{}", z.naechster_code);
    let mut claims = z.claims.clone();
    // Die Zeitfenster gehören zum Zeitpunkt der Anmeldung, nicht zum Start des
    // Nachbaus: Tests stellen die Uhr, ohne neu zu starten.
    let jetzt = m.uhr.jetzt().unix_timestamp();
    claims["exp"] = json!(jetzt + 3600);
    claims["nbf"] = json!(jetzt - 10);
    claims["iat"] = json!(jetzt - 10);
    z.codes.insert(
        code.clone(),
        CodeInfo {
            challenge: abfrage.get("code_challenge").cloned().unwrap_or_default(),
            nonce: abfrage.get("nonce").cloned().unwrap_or_default(),
            claims,
        },
    );
    let state = abfrage.get("state").cloned().unwrap_or_default();
    let redirect = abfrage.get("redirect_uri").cloned().unwrap_or_default();
    Redirect::to(&format!("{redirect}?code={code}&state={state}")).into_response()
}

async fn token(AxState(m): AxState<Mock>, rumpf: Bytes) -> Response {
    let form: HashMap<String, String> = url::form_urlencoded::parse(&rumpf).into_owned().collect();
    let verzoegerung = {
        let mut z = m.zustand.lock().unwrap();
        z.token_hits += 1;
        if let Some((status, code)) = z.token_fehler {
            return (
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(json!({ "error": code })),
            )
                .into_response();
        }
        z.verzoegerung_ms
    };
    let jetzt = m.uhr.jetzt().unix_timestamp();
    if let Err(fehler) = pruefe_client(&m, &form, jetzt) {
        m.zustand.lock().unwrap().assertion_fehler = Some(fehler);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_client" })),
        )
            .into_response();
    }
    if verzoegerung > 0 {
        tokio::time::sleep(Duration::from_millis(verzoegerung)).await;
    }

    if form.get("grant_type").map(String::as_str) == Some("authorization_code") {
        let Some(code) = form.get("code").cloned() else {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_grant"})),
            )
                .into_response();
        };
        let info = {
            let mut z = m.zustand.lock().unwrap();
            z.codes.remove(&code)
        };
        let Some(info) = info else {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_grant"})),
            )
                .into_response();
        };
        let verifier = form.get("code_verifier").cloned().unwrap_or_default();
        let erwartet = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        if erwartet != info.challenge {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_grant"})),
            )
                .into_response();
        }
        let mut claims = info.claims;
        claims["nonce"] = json!(info.nonce);
        let key = {
            let z = m.zustand.lock().unwrap();
            z.sign_with.clone().unwrap_or_else(|| m.key.clone())
        };
        let id = sign(&key, &m.kid, &claims);
        return Json(json!({
            "id_token": id,
            "refresh_token": "refresh-1",
            "access_token": GRAPH_TOKEN,
            "token_type": "Bearer",
            "expires_in": 3600
        }))
        .into_response();
    }

    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
        m.zustand
            .lock()
            .unwrap()
            .refresh_scopes
            .push(form.get("scope").cloned().unwrap_or_default());
        let token_wert = form.get("refresh_token").cloned().unwrap_or_default();
        let (fehler, mut claims, verzoegerung, doppelt) = {
            let mut z = m.zustand.lock().unwrap();
            z.refresh_hits += 1;
            let doppelt = !z.benutzte_refresh.insert(token_wert);
            (
                z.refresh_fehler,
                z.refresh_claims.clone(),
                z.verzoegerung_ms,
                doppelt,
            )
        };
        if doppelt {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "invalid_grant" })),
            )
                .into_response();
        }
        if verzoegerung > 0 {
            tokio::time::sleep(Duration::from_millis(verzoegerung)).await;
        }
        if let Some(fehler) = fehler {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": fehler }))).into_response();
        }
        let jetzt = m.uhr.jetzt().unix_timestamp();
        claims["exp"] = json!(jetzt + 3600);
        claims["nbf"] = json!(jetzt - 10);
        claims["iat"] = json!(jetzt - 10);
        if let Some(objekt) = claims.as_object_mut() {
            objekt.remove("nonce");
        }
        let key = {
            let z = m.zustand.lock().unwrap();
            z.sign_with.clone().unwrap_or_else(|| m.key.clone())
        };
        let id = sign(&key, &m.kid, &claims);
        return Json(json!({
            "id_token": id,
            "refresh_token": "refresh-2",
            "access_token": GRAPH_TOKEN,
            "token_type": "Bearer",
            "expires_in": 3600
        }))
        .into_response();
    }

    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "unsupported_grant_type"})),
    )
        .into_response()
}

/// `GET /v1.0/groups/{id}` – der Graph-Nachbau (Aufgabe 30).
async fn graph_gruppe(
    AxState(m): AxState<Mock>,
    axum::extract::Path(id): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Response {
    let verhalten = {
        let mut z = m.zustand.lock().unwrap();
        z.graph_hits.push(id.clone());
        if let Some(wert) = headers
            .get(header::AUTHORIZATION)
            .and_then(|w| w.to_str().ok())
        {
            z.graph_tokens
                .push(String::from(wert.trim_start_matches("Bearer ")));
        }
        z.graph.clone()
    };
    match verhalten {
        GraphVerhalten::Status(code) => (
            StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({ "error": "test" })),
        )
            .into_response(),
        GraphVerhalten::Weiterleitung => Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, "https://example.test/woanders")
            .body(axum::body::Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        GraphVerhalten::FremdeId => Json(json!({
            "id": GRUPPE_NEU,
            "displayName": "Fremde Gruppe"
        }))
        .into_response(),
        GraphVerhalten::Riesig => Json(json!({
            "id": id,
            "displayName": "x".repeat(70 * 1024)
        }))
        .into_response(),
        GraphVerhalten::BoeserName => Json(json!({
            "id": id,
            "displayName": "  Team\u{202E}\u{0000} Alpha\u{200F}  "
        }))
        .into_response(),
        GraphVerhalten::Langsam => {
            tokio::time::sleep(Duration::from_secs(6)).await;
            Json(json!({ "id": id, "displayName": "Zu spät" })).into_response()
        }
        GraphVerhalten::Name(name) => {
            Json(json!({ "id": id, "displayName": name })).into_response()
        }
    }
}

// ------------------------------------------------------------------- Prüfung

struct Antwort {
    status: StatusCode,
    json: Value,
    text: String,
    headers: HeaderMap,
}

struct Pruefung {
    router: Router,
    daten: PathBuf,
    oeffentlich: String,
    uhr: Uhr,
    cookie: String,
    mock: Option<Mock>,
    ausweis: Ausweis,
    /// Ordner mit dem Schlüssel (nur beim Zertifikatsweg; wird aufgeräumt).
    key_ordner: Option<PathBuf>,
    /// Basis für Graph, wenn Gruppennamen geholt werden (Aufgabe 30).
    graph_basis: Option<String>,
    http: reqwest::Client,
}

impl Pruefung {
    async fn neu() -> Self {
        Self::bauen(None, Ausweis::geheimnis(""), None).await
    }

    async fn mit_entra() -> Self {
        Self::mit_entra_graph(false, None).await
    }

    /// Entra mit dem Schalter für Gruppennamen; Graph ist der Nachbau (Aufgabe 30).
    async fn mit_entra_namen() -> Self {
        Self::mit_entra_graph(true, None).await
    }

    /// Entra mit Schalter, aber Graph zeigt auf `fremde_basis` (z. B. ins Leere).
    async fn mit_entra_totem_graph(fremde_basis: &str) -> Self {
        Self::mit_entra_graph(true, Some(fremde_basis.to_string())).await
    }

    async fn mit_entra_graph(namen: bool, fremde_basis: Option<String>) -> Self {
        let uhr = Uhr::default();
        let mock = Mock::start(uhr.clone()).await;
        let basis = if namen {
            Some(fremde_basis.unwrap_or_else(|| mock.basis.clone()))
        } else {
            None
        };
        let mut p = Self::bauen(Some(mock.clone()), Ausweis::geheimnis(ENTRA_SECRET), basis).await;
        p.uhr = uhr;
        p
    }

    /// Entra mit Zertifikat statt Geheimnis (Aufgabe 23); `tage` steuert den
    /// Ablauf (für die Warnung).
    async fn mit_entra_schluessel(tage: i64) -> Self {
        static ZAEHLER: AtomicUsize = AtomicUsize::new(1_000);
        let uhr = Uhr::default();
        let mock = Mock::start(uhr.clone()).await;
        let ordner = std::env::temp_dir().join(format!(
            "quiver-entra-key-{}-{}",
            std::process::id(),
            ZAEHLER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&ordner).expect("Ordner");
        let pfad = ordner.join(SCHLUESSEL_DATEI);
        let schluessel = erzeugen(&pfad, uhr.jetzt(), tage).expect("Schlüssel");
        mock.setze_zertifikat(schluessel.zertifikat_pem());
        let halter = SchluesselHalter::laden(pfad).expect("Halter");
        let mut p = Self::bauen(
            Some(mock.clone()),
            Ausweis::Schluessel(Arc::new(halter)),
            None,
        )
        .await;
        p.uhr = uhr;
        p.key_ordner = Some(ordner);
        p
    }

    fn key_pfad(&self) -> PathBuf {
        self.key_ordner
            .as_ref()
            .expect("kein Schlüssel")
            .join(SCHLUESSEL_DATEI)
    }

    async fn bauen(mock: Option<Mock>, ausweis: Ausweis, graph_basis: Option<String>) -> Self {
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Zeit")
            .as_nanos();
        let daten = std::env::temp_dir().join(format!(
            "quiver-entra-test-{}-{nummer}-{stempel}",
            std::process::id()
        ));
        std::fs::create_dir_all(&daten).expect("Ordner");
        let uhr = mock.as_ref().map(|m| m.uhr.clone()).unwrap_or_default();
        let oeffentlich = String::from(OEFFENTLICH);
        benutzer::root_setzen_auf_platte(&daten, ROOT_EMAIL, ROOT_PASSWORT).expect("Root");
        let mut konfig = Konfig::neu(&daten);
        konfig.oeffentlich = oeffentlich.clone();
        konfig.uhr = uhr.clone();
        if let Some(mock) = &mock {
            konfig.entra = Some(EntraKonfig {
                tenant: TENANT.into(),
                client_id: CLIENT.into(),
                ausweis: ausweis.clone(),
                authority: mock.basis.clone(),
                issuer: mock.basis.clone(),
                redirect_uri: format!("{oeffentlich}/auth/callback"),
                gruppen_namen: graph_basis.is_some(),
                graph_basis: graph_basis
                    .clone()
                    .unwrap_or_else(|| String::from(GRAPH_BASIS)),
            });
        }
        let router = app(konfig).await.expect("App baut");
        let mut p = Self {
            router,
            daten,
            oeffentlich,
            uhr,
            cookie: String::new(),
            mock,
            ausweis,
            key_ordner: None,
            graph_basis,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("HTTP"),
        };
        p.cookie = p.root_anmelden().await;
        p
    }

    async fn neu_oeffentlich(&self, oeffentlich: &str, altgeraete_erlaubt: bool) -> Router {
        let mut konfig = Konfig::neu(&self.daten);
        konfig.oeffentlich = oeffentlich.to_string();
        konfig.uhr = self.uhr.clone();
        konfig.altgeraete_erlaubt = altgeraete_erlaubt;
        if let Some(mock) = &self.mock {
            konfig.entra = Some(EntraKonfig {
                tenant: TENANT.into(),
                client_id: CLIENT.into(),
                ausweis: self.ausweis.clone(),
                authority: mock.basis.clone(),
                issuer: mock.basis.clone(),
                redirect_uri: format!("{oeffentlich}/auth/callback"),
                gruppen_namen: self.graph_basis.is_some(),
                graph_basis: self
                    .graph_basis
                    .clone()
                    .unwrap_or_else(|| String::from(GRAPH_BASIS)),
            });
        }
        app(konfig).await.expect("App baut")
    }

    async fn gruppen(&self) -> Vec<quiver_domain::Gruppe> {
        let antwort = self.admin("GET", "/admin/gruppen", None).await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        serde_json::from_value(antwort.json).expect("Gruppen")
    }

    async fn entra_gruppen(&self) -> Vec<quiver_protocol::EntraGruppe> {
        let antwort = self.admin("GET", "/admin/entra-gruppen", None).await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        serde_json::from_value(antwort.json).expect("Entra-Gruppen")
    }

    async fn root_anmelden(&self) -> String {
        let _wache = ROOT_SPERRE.lock().await;
        let antwort = self
            .anfrage(
                "POST",
                "/auth/root",
                None,
                None,
                &[("origin", &self.oeffentlich), ("x-quiver", "1")],
                Some(
                    serde_json::to_vec(&json!({
                        "email": ROOT_EMAIL,
                        "passwort": ROOT_PASSWORT
                    }))
                    .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "Root: {}", antwort.text);
        cookie_aus(
            &antwort,
            if self.oeffentlich.starts_with("https") {
                "__Host-quiver_sitzung"
            } else {
                "quiver_sitzung"
            },
        )
    }

    async fn anfrage(
        &self,
        methode: &str,
        pfad: &str,
        bearer: Option<&str>,
        cookie: Option<&str>,
        extra: &[(&str, &str)],
        rumpf: Option<Vec<u8>>,
    ) -> Antwort {
        let mut bauer = axum::http::Request::builder()
            .method(methode)
            .uri(format!("/api/v1{pfad}"))
            .header("host", HOST)
            .header("content-type", "application/json");
        if let Some(token) = bearer {
            bauer = bauer.header("authorization", format!("Bearer {token}"));
        }
        if let Some(cookie) = cookie {
            bauer = bauer.header("cookie", cookie);
        }
        for (name, wert) in extra {
            bauer = bauer.header(*name, *wert);
        }
        let antwort = self
            .router
            .clone()
            .oneshot(
                bauer
                    .body(
                        rumpf
                            .map(axum::body::Body::from)
                            .unwrap_or_else(axum::body::Body::empty),
                    )
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }

    async fn browsere_anfrage(
        &self,
        pfad: &str,
        cookie: Option<&str>,
        extra: &[(&str, &str)],
        rumpf: Option<Vec<u8>>,
    ) -> Antwort {
        let mut bauer = axum::http::Request::builder()
            .method("GET")
            .uri(pfad)
            .header("host", HOST);
        if let Some(cookie) = cookie {
            bauer = bauer.header("cookie", cookie);
        }
        for (name, wert) in extra {
            bauer = bauer.header(*name, *wert);
        }
        let antwort = self
            .router
            .clone()
            .oneshot(
                bauer
                    .body(
                        rumpf
                            .map(axum::body::Body::from)
                            .unwrap_or_else(axum::body::Body::empty),
                    )
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }

    async fn browsere_post(
        &self,
        pfad: &str,
        cookie: Option<&str>,
        extra: &[(&str, &str)],
        rumpf: Vec<u8>,
    ) -> Antwort {
        let mut bauer = axum::http::Request::builder()
            .method("POST")
            .uri(pfad)
            .header("host", HOST);
        if let Some(cookie) = cookie {
            bauer = bauer.header("cookie", cookie);
        }
        bauer = bauer.header("content-type", "application/x-www-form-urlencoded");
        for (name, wert) in extra {
            bauer = bauer.header(*name, *wert);
        }
        let antwort = self
            .router
            .clone()
            .oneshot(bauer.body(axum::body::Body::from(rumpf)).expect("Anfrage"))
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }

    async fn admin(&self, methode: &str, pfad: &str, rumpf: Option<Value>) -> Antwort {
        self.admin_mit(&self.cookie, methode, pfad, rumpf).await
    }

    async fn admin_mit(
        &self,
        cookie: &str,
        methode: &str,
        pfad: &str,
        rumpf: Option<Value>,
    ) -> Antwort {
        self.anfrage(
            methode,
            pfad,
            None,
            Some(cookie),
            &[("origin", &self.oeffentlich), ("x-quiver", "1")],
            rumpf.map(|wert| serde_json::to_vec(&wert).expect("JSON")),
        )
        .await
    }

    fn verifier() -> String {
        URL_SAFE_NO_PAD.encode([9u8; 32])
    }

    fn challenge(verifier: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    }

    /// Browser-Weg bis zum Einmalcode; `admin` startet die Verwaltungsanmeldung.
    async fn oidc(&self, admin: bool, name: &str, port: u16) -> (String, String, String) {
        let verifier = Self::verifier();
        let challenge = Self::challenge(&verifier);
        let pfad = if admin {
            String::from("/auth/admin/start")
        } else {
            format!(
                "/auth/geraet/start?challenge={challenge}&port={port}&zustand=test-1&name={}",
                name.replace(' ', "%20")
            )
        };
        let start = self.browsere_anfrage(&pfad, None, &[], None).await;
        assert_eq!(start.status, StatusCode::SEE_OTHER, "start: {}", start.text);
        let ziel = kopf(&start, header::LOCATION);
        let location = url::Url::parse(&ziel).expect("Location");
        let abfrage: HashMap<String, String> = location.query_pairs().into_owned().collect();
        let state = abfrage.get("state").cloned().expect("state");
        if !admin {
            assert_eq!(
                abfrage.get("code_challenge").map(String::len),
                Some(43),
                "der Server schickt seinen eigenen PKCE-Challenge"
            );
        }
        let _ = challenge;
        let oidc_cookie = cookie_aus(&start, "quiver_oidc");

        let antwort = self
            .http
            .get(ziel)
            .send()
            .await
            .expect("Entra-Nachbau erreichbar");
        assert_eq!(antwort.status(), StatusCode::SEE_OTHER);
        let redirect = antwort
            .headers()
            .get(header::LOCATION)
            .and_then(|wert| wert.to_str().ok())
            .expect("Rücksprung")
            .to_string();
        let ziel = url::Url::parse(&redirect).expect("Rücksprung-URL");
        let paare: HashMap<String, String> = ziel.query_pairs().into_owned().collect();
        let code = paare.get("code").cloned().expect("code");
        (code, state, oidc_cookie)
    }

    async fn callback(&self, code: &str, state: &str, cookie: &str) -> Antwort {
        self.browsere_anfrage(
            &format!("/auth/callback?code={code}&state={state}"),
            Some(cookie),
            &[],
            None,
        )
        .await
    }

    /// Ganzer Geräteweg bis zum Geräte-Token.
    async fn geraet_anmelden(&self, name: &str, port: u16) -> AnmeldenAntwort {
        let (code, state, oidc_cookie) = self.oidc(false, name, port).await;
        let seite = self.callback(&code, &state, &oidc_cookie).await;
        assert_eq!(
            seite.status,
            StatusCode::OK,
            "Bestätigungsseite: {}",
            seite.text
        );
        let token = feld(&seite.text, "token");
        let csrf = feld(&seite.text, "csrf");
        let cookie = cookie_aus(&seite, "quiver_bestaetigung");
        let bestaetigt = self
            .browsere_post(
                "/auth/geraet/bestaetigen",
                Some(&cookie),
                &[("origin", &self.oeffentlich)],
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("token", &token)
                    .append_pair("csrf", &csrf)
                    .finish()
                    .into_bytes(),
            )
            .await;
        assert_eq!(
            bestaetigt.status,
            StatusCode::SEE_OTHER,
            "{}",
            bestaetigt.text
        );
        let ziel = kopf(&bestaetigt, header::LOCATION);
        assert!(
            ziel.starts_with(&format!("http://127.0.0.1:{port}/quiver?")),
            "genau Loopback: {ziel}"
        );
        let ziel_url = url::Url::parse(&ziel).expect("Ziel");
        let paare: HashMap<String, String> = ziel_url.query_pairs().into_owned().collect();
        let code = paare.get("code").cloned().expect("Einmalcode");
        let antwort = self
            .anfrage(
                "POST",
                "/auth/geraet/tausch",
                None,
                None,
                &[],
                Some(
                    serde_json::to_vec(&json!({
                        "code": code,
                        "verifier": Self::verifier(),
                    }))
                    .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "Tausch: {}", antwort.text);
        serde_json::from_value(antwort.json).expect("AnmeldenAntwort")
    }

    async fn verwaltungs_anmeldung(&self) -> String {
        let (code, state, oidc_cookie) = self.oidc(true, "", 0).await;
        let antwort = self.callback(&code, &state, &oidc_cookie).await;
        assert_eq!(antwort.status, StatusCode::SEE_OTHER, "{}", antwort.text);
        cookie_aus(&antwort, "quiver_sitzung")
    }

    async fn eintrag_anlegen(&self) {
        self.eintrag_und_satz_anlegen().await;
        let gruppe = quiver_domain::Gruppe {
            id: quiver_domain::GruppeId::neu("team"),
            titel: "Team".into(),
            saetze: vec![SatzId::neu("basis")],
            entra: vec![GRUPPE_ENTRA.to_string()],
        };
        let antwort = self
            .admin("PUT", "/admin/gruppen/team", Some(json!(gruppe)))
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
    }

    /// Nur Eintrag und Satz, ohne Gruppe: die Gruppe legt die Anmeldung an.
    async fn eintrag_und_satz_anlegen(&self) {
        let eintrag = Eintrag {
            id: EintragId::neu("wetter"),
            version: "1.0.0".into(),
            titel: "Wetter".into(),
            beschreibung: String::new(),
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: "npx".into(),
                argumente: Vec::new(),
                umgebung: Werte::new(),
            },
            felder: vec![Feld {
                name: "PFAD".into(),
                beschreibung: "Pfad".into(),
                quelle: Quelle::Server,
                pflicht: true,
                standard: None,
                link: None,
                ..Default::default()
            }],
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
            mehrfach: None,
        };
        let satz = Satz {
            id: SatzId::neu("basis"),
            titel: "Basis".into(),
            prioritaet: 0,
            eintraege: vec![SatzEintrag {
                eintrag: EintragId::neu("wetter"),
                werte: serde_json::from_value(json!({"PFAD": "/tmp/wetter"})).expect("Werte"),
            }],
        };
        for (pfad, wert) in [
            ("/admin/eintraege/wetter", json!(eintrag)),
            ("/admin/saetze/basis", json!(satz)),
        ] {
            let antwort = self.admin("PUT", pfad, Some(wert)).await;
            assert_eq!(antwort.status, StatusCode::OK, "{pfad}: {}", antwort.text);
        }
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.daten);
        if let Some(ordner) = &self.key_ordner {
            let _ = std::fs::remove_dir_all(ordner);
        }
    }
}

fn kopf(antwort: &Antwort, name: header::HeaderName) -> String {
    antwort
        .headers
        .get(name)
        .and_then(|wert| wert.to_str().ok())
        .expect("Kopf")
        .to_string()
}

fn cookie_aus(antwort: &Antwort, name: &str) -> String {
    antwort
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|wert| wert.to_str().ok())
        .find_map(|wert| {
            let erstes = wert.split(';').next().unwrap_or_default();
            erstes
                .starts_with(&format!("{name}="))
                .then(|| erstes.to_string())
        })
        .unwrap_or_else(|| panic!("Cookie {name} fehlt: {:?}", antwort.headers))
}

fn feld(html: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let start = html
        .find(&marker)
        .unwrap_or_else(|| panic!("Feld {name} fehlt"))
        + marker.len();
    let rest = &html[start..];
    let ende = rest.find('"').expect("Wertende");
    rest[..ende].to_string()
}

async fn auspacken(antwort: Response) -> Antwort {
    let status = antwort.status();
    let headers = antwort.headers().clone();
    let bytes = antwort
        .into_body()
        .collect()
        .await
        .expect("Rumpf")
        .to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Antwort {
        status,
        json,
        text,
        headers,
    }
}

// -------------------------------------------------------------------- Tests

#[tokio::test]
async fn voller_geraeteweg_ueber_entra() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let antwort = p.geraet_anmelden("Testrechner", 51234).await;
        assert_eq!(antwort.geraet.name, "Testrechner");
        assert_eq!(antwort.token.len(), 64);
        assert_eq!(
            antwort.geraet.gruppen,
            vec![quiver_domain::GruppeId::neu("team")],
            "Entra-Gruppe wird auf die interne Gruppe abgebildet"
        );
        let soll = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(soll.status, StatusCode::OK, "{}", soll.text);
        assert_eq!(soll.json["eintraege"].as_array().map(Vec::len), Some(1));

        // Der Benutzer steht in der Verwaltung, die Geräte-Gruppen sind wirksam.
        let benutzer_liste = p.admin("GET", "/admin/benutzer", None).await;
        let liste: Vec<BenutzerSicht> = serde_json::from_value(benutzer_liste.json).expect("Liste");
        let test = liste.iter().find(|b| b.oid == OID).expect("Benutzer");
        assert_eq!(test.rolle, Rolle::Benutzer);
        assert_eq!(test.geraete, 1);
        assert_eq!(test.entra, vec![GRUPPE_ENTRA.to_string()]);
    })
    .await;
}

#[tokio::test]
async fn start_lehnt_kaputte_werte_ab() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        for pfad in [
            "/auth/geraet/start?challenge=kurz&port=51234&zustand=a&name=x".to_string(),
            format!(
                "/auth/geraet/start?challenge={}&port=80&zustand=a&name=x",
                Pruefung::challenge(&Pruefung::verifier())
            ),
            format!(
                "/auth/geraet/start?challenge={}&port=51234&zustand=mit%20leer&name=x",
                Pruefung::challenge(&Pruefung::verifier())
            ),
            format!(
                "/auth/geraet/start?challenge={}&port=51234&zustand=a&name={}",
                Pruefung::challenge(&Pruefung::verifier()),
                "x".repeat(81)
            ),
        ] {
            let antwort = p.browsere_anfrage(&pfad, None, &[], None).await;
            assert_eq!(antwort.status, StatusCode::BAD_REQUEST, "{pfad}");
            assert!(antwort.text.contains("Anmeldung nicht möglich"));
        }
    })
    .await;
}

#[tokio::test]
async fn state_ist_an_browser_und_einmaligkeit_gebunden() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let (code, state, cookie) = p.oidc(false, "Rechner", 51234).await;

        // Anderer Browser (falsches Cookie) → 400, der Zustand ist verbraucht.
        let fremd = p.callback(&code, &state, "quiver_oidc=falsch").await;
        assert_eq!(fremd.status, StatusCode::BAD_REQUEST);

        let nochmal = p.callback(&code, &state, &cookie).await;
        assert_eq!(nochmal.status, StatusCode::BAD_REQUEST, "einmalig");
    })
    .await;
}

#[tokio::test]
async fn falscher_verifier_verbraucht_den_code() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let (code, state, oidc_cookie) = p.oidc(false, "Rechner", 51234).await;
        let seite = p.callback(&code, &state, &oidc_cookie).await;
        let token = feld(&seite.text, "token");
        let csrf = feld(&seite.text, "csrf");
        let cookie = cookie_aus(&seite, "quiver_bestaetigung");
        let bestaetigt = p
            .browsere_post(
                "/auth/geraet/bestaetigen",
                Some(&cookie),
                &[("origin", &p.oeffentlich)],
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("token", &token)
                    .append_pair("csrf", &csrf)
                    .finish()
                    .into_bytes(),
            )
            .await;
        let ziel = kopf(&bestaetigt, header::LOCATION);
        let ziel_url = url::Url::parse(&ziel).expect("Ziel");
        let paare: HashMap<String, String> = ziel_url.query_pairs().into_owned().collect();
        let code = paare.get("code").cloned().expect("Code");

        let falsch = p
            .anfrage(
                "POST",
                "/auth/geraet/tausch",
                None,
                None,
                &[],
                Some(
                    serde_json::to_vec(&Tausch {
                        code: code.clone(),
                        verifier: URL_SAFE_NO_PAD.encode([1u8; 32]),
                        altes_token: None,
                    })
                    .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(falsch.status, StatusCode::BAD_REQUEST);

        // Auch mit richtigem Verifier: der Code ist verbraucht.
        let nochmal = p
            .anfrage(
                "POST",
                "/auth/geraet/tausch",
                None,
                None,
                &[],
                Some(
                    serde_json::to_vec(&Tausch {
                        code,
                        verifier: Pruefung::verifier(),
                        altes_token: None,
                    })
                    .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(nochmal.status, StatusCode::BAD_REQUEST);
    })
    .await;
}

#[tokio::test]
async fn bestaetigung_braucht_csrf_und_origin_und_abbruch_geht_zurueck() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let (code, state, oidc_cookie) = p.oidc(false, "Rechner", 51234).await;
        let seite = p.callback(&code, &state, &oidc_cookie).await;
        let token = feld(&seite.text, "token");
        let csrf = feld(&seite.text, "csrf");
        let cookie = cookie_aus(&seite, "quiver_bestaetigung");

        // Ohne CSRF → 400.
        let ohne = p
            .browsere_post(
                "/auth/geraet/bestaetigen",
                Some(&cookie),
                &[("origin", &p.oeffentlich)],
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("token", &token)
                    .finish()
                    .into_bytes(),
            )
            .await;
        assert_eq!(ohne.status, StatusCode::BAD_REQUEST);

        // Fremder Origin → 403; der Zustand ist dabei verbraucht.
        let fremd = p
            .browsere_post(
                "/auth/geraet/bestaetigen",
                Some(&cookie),
                &[("origin", "https://evil.example.test")],
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("token", &token)
                    .append_pair("csrf", &csrf)
                    .finish()
                    .into_bytes(),
            )
            .await;
        assert_eq!(fremd.status, StatusCode::FORBIDDEN);

        // Neuer Anlauf für den Abbruch.
        let (code, state, oidc_cookie) = p.oidc(false, "Rechner", 51234).await;
        let seite = p.callback(&code, &state, &oidc_cookie).await;
        let token = feld(&seite.text, "token");
        let csrf = feld(&seite.text, "csrf");
        let cookie = cookie_aus(&seite, "quiver_bestaetigung");
        let abbruch = p
            .browsere_post(
                "/auth/geraet/abbrechen",
                Some(&cookie),
                &[("origin", &p.oeffentlich)],
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("token", &token)
                    .append_pair("csrf", &csrf)
                    .finish()
                    .into_bytes(),
            )
            .await;
        assert_eq!(abbruch.status, StatusCode::SEE_OTHER);
        let ziel = kopf(&abbruch, header::LOCATION);
        assert!(ziel.starts_with("http://127.0.0.1:51234/quiver?"));
        assert!(ziel.contains("fehler=abgebrochen"));
        assert!(ziel.contains("zustand=test-1"));
    })
    .await;
}

#[tokio::test]
async fn falsche_signatur_wird_abgelehnt() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let mock = p.mock.as_ref().expect("Mock");
        mock.falscher_schluessel();
        let (code, state, oidc_cookie) = p.oidc(false, "Rechner", 51234).await;
        let antwort = p.callback(&code, &state, &oidc_cookie).await;
        assert_eq!(antwort.status, StatusCode::BAD_GATEWAY);
        assert!(antwort.text.contains("Anmeldung gescheitert"));
        assert!(!antwort.text.contains(ENTRA_SECRET));
    })
    .await;
}

#[tokio::test]
async fn falscher_tenant_und_gruppen_ueberlauf_werden_abgelehnt() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let mock = p.mock.as_ref().expect("Mock");
        mock.setze_claims(|c| c["tid"] = json!("00000000-0000-0000-0000-000000000000"));
        let (code, state, cookie) = p.oidc(false, "Rechner", 51234).await;
        let antwort = p.callback(&code, &state, &cookie).await;
        assert_eq!(antwort.status, StatusCode::BAD_GATEWAY);

        mock.setze_claims(|c| {
            c["tid"] = json!(TENANT);
            c.as_object_mut().unwrap().remove("groups");
            c["_claim_names"] = json!({"groups": "src1"});
        });
        let (code, state, cookie) = p.oidc(false, "Rechner", 51234).await;
        let antwort = p.callback(&code, &state, &cookie).await;
        assert_eq!(antwort.status, StatusCode::BAD_GATEWAY);
        assert!(antwort.text.contains("Gruppen"));
    })
    .await;
}

#[tokio::test]
async fn altes_token_fuehrt_dasselbe_geraet() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let erst = p.geraet_anmelden("Rechner", 51234).await;

        // Zweite Anmeldung mit altem Token: dieselbe Kennung, neues Token.
        let zweit = p
            .geraet_anmelden_mit_altem("Rechner", 51234, Some(&erst.token))
            .await;
        assert_eq!(zweit.geraet.id, erst.geraet.id);
        assert_ne!(zweit.token, erst.token);
        let alt = p
            .anfrage("GET", "/geraet/soll", Some(&erst.token), None, &[], None)
            .await;
        assert_eq!(
            alt.status,
            StatusCode::UNAUTHORIZED,
            "altes Token gilt nicht mehr"
        );
    })
    .await;
}

impl Pruefung {
    async fn geraet_anmelden_mit_altem(
        &self,
        name: &str,
        port: u16,
        altes: Option<&str>,
    ) -> AnmeldenAntwort {
        let (code, state, oidc_cookie) = self.oidc(false, name, port).await;
        let seite = self.callback(&code, &state, &oidc_cookie).await;
        let token = feld(&seite.text, "token");
        let csrf = feld(&seite.text, "csrf");
        let cookie = cookie_aus(&seite, "quiver_bestaetigung");
        let bestaetigt = self
            .browsere_post(
                "/auth/geraet/bestaetigen",
                Some(&cookie),
                &[("origin", &self.oeffentlich)],
                url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("token", &token)
                    .append_pair("csrf", &csrf)
                    .finish()
                    .into_bytes(),
            )
            .await;
        let ziel = kopf(&bestaetigt, header::LOCATION);
        let ziel_url = url::Url::parse(&ziel).expect("Ziel");
        let paare: HashMap<String, String> = ziel_url.query_pairs().into_owned().collect();
        let code = paare.get("code").cloned().expect("Code");
        let antwort = self
            .anfrage(
                "POST",
                "/auth/geraet/tausch",
                None,
                None,
                &[],
                Some(
                    serde_json::to_vec(&GeraeteTausch {
                        code,
                        verifier: Self::verifier(),
                        altes_token: altes.map(str::to_string),
                    })
                    .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        serde_json::from_value(antwort.json).expect("Antwort")
    }
}

#[tokio::test]
async fn gesperrt_und_kein_zugriff_liefern_grund() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let antwort = p.geraet_anmelden("Rechner", 51234).await;

        let liste = p.admin("GET", "/admin/benutzer", None).await;
        let liste: Vec<BenutzerSicht> = serde_json::from_value(liste.json).expect("Liste");
        let test = liste.iter().find(|b| b.oid == OID).expect("Benutzer");

        // Sperren → Gerät bekommt 403 mit grund=gesperrt.
        let gesperrt = p
            .admin(
                "PUT",
                &format!("/admin/benutzer/{}", test.oid),
                Some(json!(BenutzerAendern {
                    gesperrt: Some(true),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(gesperrt.status, StatusCode::OK);
        let nach = p.admin("GET", "/admin/benutzer", None).await;
        let nach: Vec<BenutzerSicht> = serde_json::from_value(nach.json).expect("Liste");
        assert!(
            nach.iter().any(|b| b.oid == OID && b.gesperrt),
            "Sperre steht in der Datenbank: {nach:?}"
        );
        let abgewiesen = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(abgewiesen.status, StatusCode::FORBIDDEN);
        assert_eq!(abgewiesen.json["grund"], "gesperrt");

        // Entsperren, Gruppe wegnehmen → grund=kein_zugriff.
        let frei = p
            .admin(
                "PUT",
                &format!("/admin/benutzer/{}", test.oid),
                Some(json!(BenutzerAendern {
                    gesperrt: Some(false),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(frei.status, StatusCode::OK);
        let ohne = p
            .admin(
                "PUT",
                "/admin/gruppen/team",
                Some(json!(quiver_domain::Gruppe {
                    id: quiver_domain::GruppeId::neu("team"),
                    titel: "Team".into(),
                    saetze: vec![SatzId::neu("basis")],
                    entra: Vec::new(),
                })),
            )
            .await;
        assert_eq!(ohne.status, StatusCode::OK);
        let abgewiesen = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(abgewiesen.status, StatusCode::FORBIDDEN);
        assert_eq!(abgewiesen.json["grund"], "kein-zugriff");
    })
    .await;
}

#[tokio::test]
async fn refresh_rotation_und_invalid_grant() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let antwort = p.geraet_anmelden("Rechner", 51234).await;
        let mock = p.mock.clone().expect("Mock");

        // Nachprüfen auf Knopf: genau ein Refresh-Aufruf, Token bleibt gültig.
        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::OK, "{}", geprueft.text);
        assert_eq!(mock.zaehler().2, 1);
        let soll = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(soll.status, StatusCode::OK);

        // Gruppen ändern sich → Gerät sieht das neue Soll.
        mock.setze_refresh_claims(|c| c["groups"] = json!([]));
        let wieder = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(wieder.status, StatusCode::FORBIDDEN, "{}", wieder.text);
        let abgewiesen = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(abgewiesen.status, StatusCode::FORBIDDEN);
        assert_eq!(abgewiesen.json["grund"], "neu-anmelden");

        // invalid_grant → neu_anmelden.
        let p2 = Pruefung::mit_entra().await;
        p2.eintrag_anlegen().await;
        let antwort = p2.geraet_anmelden("Rechner", 51234).await;
        p2.mock.as_ref().unwrap().refresh_fehler("invalid_grant");
        let abgelehnt = p2
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(abgelehnt.status, StatusCode::FORBIDDEN);
        let abgewiesen = p2
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(abgewiesen.status, StatusCode::FORBIDDEN);
        assert_eq!(abgewiesen.json["grund"], "neu-anmelden");
    })
    .await;
}

#[tokio::test]
async fn nachpruefen_je_benutzer_nicht_gleichzeitig() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let _antwort = p.geraet_anmelden("Rechner", 51234).await;
        p.mock.as_ref().unwrap().verzoegern(200);

        let pfad = format!("/admin/benutzer/{OID}/pruefen");
        let (a, b) = tokio::join!(p.admin("POST", &pfad, None), p.admin("POST", &pfad, None));
        assert_eq!(a.status, StatusCode::OK, "{}", a.text);
        assert_eq!(b.status, StatusCode::OK, "{}", b.text);
        // Zwei Prüfungen, aber nacheinander: jede hat ein frisches Refresh-Token.
        assert_eq!(p.mock.as_ref().unwrap().zaehler().2, 2);
    })
    .await;
}

#[tokio::test]
async fn netzfehler_haelt_23_stunden_und_gibt_nach_25_auf() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let antwort = p.geraet_anmelden("Rechner", 51234).await;
        p.mock.as_ref().unwrap().token_fehler(503);

        // 23 Stunden: letzter Stand bleibt.
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::hours(23));
        let cookie = p.root_anmelden().await;
        let noch = p
            .admin_mit(
                &cookie,
                "POST",
                &format!("/admin/benutzer/{OID}/pruefen"),
                None,
            )
            .await;
        assert_eq!(noch.status, StatusCode::OK, "23h: {}", noch.text);
        let soll = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(soll.status, StatusCode::OK);

        // 25 Stunden: neu anmelden.
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::hours(25));
        let cookie = p.root_anmelden().await;
        let auf = p
            .admin_mit(
                &cookie,
                "POST",
                &format!("/admin/benutzer/{OID}/pruefen"),
                None,
            )
            .await;
        assert_eq!(auf.status, StatusCode::FORBIDDEN, "25h: {}", auf.text);
        let abgewiesen = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(abgewiesen.status, StatusCode::FORBIDDEN);
        assert_eq!(abgewiesen.json["grund"], "neu-anmelden");
    })
    .await;
}

#[tokio::test]
async fn verwaltungssitzung_nur_fuer_admins_und_rechte_frisch() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        // Der Benutzer existiert nach einer Geräteanmeldung.
        p.geraet_anmelden("Rechner", 51234).await;

        // Ohne Rolle: Verwaltungsanmeldung abgelehnt.
        let ohne = p.oidc(true, "", 0).await;
        let antwort = p.callback(&ohne.0, &ohne.1, &ohne.2).await;
        assert_eq!(antwort.status, StatusCode::FORBIDDEN);

        // Root ernennt den Benutzer zum Admin.
        let ernannt = p
            .admin(
                "PUT",
                &format!("/admin/benutzer/{OID}"),
                Some(json!(BenutzerAendern {
                    rolle: Some(Rolle::Admin),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(ernannt.status, StatusCode::OK, "{}", ernannt.text);

        let cookie = p.verwaltungs_anmeldung().await;
        let ich = p
            .anfrage("GET", "/auth/ich", None, Some(&cookie), &[], None)
            .await;
        assert_eq!(ich.status, StatusCode::OK);
        assert_eq!(ich.json["benutzer"]["rolle"], "admin");
        let ueberblick = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(ueberblick.status, StatusCode::OK);

        // Rolle entzogen: die nächste Anfrage ist 403, ohne neue Anmeldung.
        let entzogen = p
            .admin(
                "PUT",
                &format!("/admin/benutzer/{OID}"),
                Some(json!(BenutzerAendern {
                    rolle: Some(Rolle::Benutzer),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(entzogen.status, StatusCode::OK);
        let danach = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(danach.status, StatusCode::FORBIDDEN);
    })
    .await;
}

#[tokio::test]
async fn sitzungscookie_origin_und_keine_cors_freigabe() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        // Lesen braucht kein Origin; ändern schon.
        let lesen = p
            .anfrage("GET", "/admin/ueberblick", None, Some(&p.cookie), &[], None)
            .await;
        assert_eq!(lesen.status, StatusCode::OK);
        let codes = json!(CodeAnlegen {
            gruppen: Vec::new(),
            minuten: None,
            unbegrenzt: false
        });
        let ohne = p
            .anfrage(
                "POST",
                "/admin/codes",
                None,
                Some(&p.cookie),
                &[],
                Some(serde_json::to_vec(&codes).unwrap()),
            )
            .await;
        assert_eq!(ohne.status, StatusCode::FORBIDDEN);
        let ohne_kopf = p
            .anfrage(
                "POST",
                "/admin/codes",
                None,
                Some(&p.cookie),
                &[("origin", &p.oeffentlich)],
                Some(serde_json::to_vec(&codes).unwrap()),
            )
            .await;
        assert_eq!(ohne_kopf.status, StatusCode::FORBIDDEN);
        let fremd = p
            .anfrage(
                "POST",
                "/admin/codes",
                None,
                Some(&p.cookie),
                &[("origin", "https://evil.example.test"), ("x-quiver", "1")],
                Some(serde_json::to_vec(&codes).unwrap()),
            )
            .await;
        assert_eq!(fremd.status, StatusCode::FORBIDDEN);
        assert!(fremd.headers.get("access-control-allow-origin").is_none());
        // Mit Origin und Kopf kommt die Anfrage durch die Wache (danach 404,
        // weil Entra läuft).
        let richtig = p
            .anfrage(
                "POST",
                "/admin/codes",
                None,
                Some(&p.cookie),
                &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                Some(serde_json::to_vec(&codes).unwrap()),
            )
            .await;
        assert_eq!(richtig.status, StatusCode::NOT_FOUND);

        // Cookie-Flags: HttpOnly, SameSite=Lax, Path=/, ohne Secure bei http.
        let login = p
            .anfrage(
                "POST",
                "/auth/root",
                None,
                None,
                &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                Some(
                    serde_json::to_vec(&json!({
                        "email": ROOT_EMAIL,
                        "passwort": ROOT_PASSWORT
                    }))
                    .expect("JSON"),
                ),
            )
            .await;
        let set = login
            .headers
            .get(header::SET_COOKIE)
            .and_then(|wert| wert.to_str().ok())
            .expect("Cookie");
        assert!(set.starts_with("quiver_sitzung="));
        assert!(set.contains("HttpOnly"));
        assert!(set.contains("SameSite=Lax"));
        assert!(set.contains("Path=/"));
        assert!(!set.contains("Secure"));

        // https: __Host- und Secure.
        let https = p.neu_oeffentlich("https://quiver.example.test", true).await;
        let login = https
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/root")
                    .header("host", "quiver.example.test")
                    .header("content-type", "application/json")
                    .header("origin", "https://quiver.example.test")
                    .header("x-quiver", "1")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&json!({
                            "email": ROOT_EMAIL,
                            "passwort": ROOT_PASSWORT
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let set = login
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|wert| wert.to_str().ok())
            .expect("Cookie")
            .to_string();
        assert!(set.starts_with("__Host-quiver_sitzung="), "{set}");
        assert!(set.contains("Secure"));
        let hsts = login
            .headers()
            .get(header::STRICT_TRANSPORT_SECURITY)
            .is_some();
        assert!(hsts);
    })
    .await;
}

#[tokio::test]
async fn sitzung_laeuft_ab_abmelden_und_neue_kennung() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let ich = p
            .anfrage("GET", "/auth/ich", None, Some(&p.cookie), &[], None)
            .await;
        assert_eq!(ich.status, StatusCode::OK);

        // 61 Minuten ohne Nutzung.
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::minutes(61));
        let alt = p
            .anfrage("GET", "/auth/ich", None, Some(&p.cookie), &[], None)
            .await;
        assert_eq!(alt.status, StatusCode::UNAUTHORIZED);

        // Neue Sitzung, 12 Stunden absolut.
        let cookie = p.root_anmelden().await;
        assert_ne!(cookie, p.cookie);
        p.uhr.setze(
            OffsetDateTime::now_utc() + time::Duration::hours(12) + time::Duration::seconds(1),
        );
        let zu_alt = p
            .anfrage("GET", "/auth/ich", None, Some(&cookie), &[], None)
            .await;
        assert_eq!(zu_alt.status, StatusCode::UNAUTHORIZED);

        // Abmelden löscht am Server.
        let cookie = p.root_anmelden().await;
        let ab = p
            .anfrage(
                "POST",
                "/auth/abmelden",
                None,
                Some(&cookie),
                &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                None,
            )
            .await;
        assert_eq!(ab.status, StatusCode::NO_CONTENT);
        let danach = p
            .anfrage("GET", "/auth/ich", None, Some(&cookie), &[], None)
            .await;
        assert_eq!(danach.status, StatusCode::UNAUTHORIZED);
    })
    .await;
}

#[tokio::test]
async fn passwortwechsel_beendet_alte_sitzungen() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let alt = p.cookie.clone();
        // Neues Root-Passwort setzen: alle Root-Sitzungen enden.
        benutzer::root_setzen_auf_platte(&p.daten, ROOT_EMAIL, "noch-ein-langes-passwort-42")
            .expect("Root neu setzen");
        let danach = p
            .anfrage("GET", "/auth/ich", None, Some(&alt), &[], None)
            .await;
        assert_eq!(danach.status, StatusCode::UNAUTHORIZED);
    })
    .await;
}

#[tokio::test]
async fn refresh_mit_anderer_oid_meldet_neu_an() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let antwort = p.geraet_anmelden("Rechner", 51234).await;
        p.mock
            .as_ref()
            .unwrap()
            .setze_refresh_claims(|c| c["oid"] = json!("00000000-1111-2222-3333-444444444444"));
        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::FORBIDDEN, "{}", geprueft.text);
        let abgewiesen = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(abgewiesen.status, StatusCode::FORBIDDEN);
        assert_eq!(abgewiesen.json["grund"], "neu-anmelden");
    })
    .await;
}

#[tokio::test]
async fn root_drossel_gleiche_antwort_und_passwortlaenge() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let _wache = ROOT_SPERRE.lock().await;
        // Zu kurzes Passwort: 400.
        let kurz = p
            .anfrage(
                "POST",
                "/auth/root",
                None,
                None,
                &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                Some(
                    serde_json::to_vec(&json!({"email": ROOT_EMAIL, "passwort": "kurz"}))
                        .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(kurz.status, StatusCode::BAD_REQUEST);

        let falsch = p
            .anfrage(
                "POST",
                "/auth/root",
                None,
                None,
                &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                Some(
                    serde_json::to_vec(&json!({
                        "email": ROOT_EMAIL,
                        "passwort": "falsches-passwort-123"
                    }))
                    .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(falsch.status, StatusCode::UNAUTHORIZED);

        let unbekannt = p
            .anfrage(
                "POST",
                "/auth/root",
                None,
                None,
                &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                Some(
                    serde_json::to_vec(&json!({
                        "email": "niemand@example.test",
                        "passwort": "falsches-passwort-123"
                    }))
                    .expect("JSON"),
                ),
            )
            .await;
        assert_eq!(unbekannt.status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            falsch.json["fehler"], unbekannt.json["fehler"],
            "unbekannte E-Mail sieht gleich aus"
        );

        // Nach fünf Fehlversuchen: 429, nie dauerhaft (Drossel-Test deckt das ab).
        let mut letzter = StatusCode::OK;
        for _ in 0..6 {
            let antwort = p
                .anfrage(
                    "POST",
                    "/auth/root",
                    None,
                    None,
                    &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                    Some(
                        serde_json::to_vec(&json!({
                            "email": "gesperrt@example.test",
                            "passwort": "falsches-passwort-123"
                        }))
                        .expect("JSON"),
                    ),
                )
                .await;
            letzter = antwort.status;
        }
        assert_eq!(
            letzter,
            StatusCode::TOO_MANY_REQUESTS,
            "nach fünf freien Versuchen gedrosselt"
        );
    })
    .await;
}

#[tokio::test]
async fn root_bleibt_root_und_admin_token_gilt_nicht() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let loeschen = p.admin("DELETE", "/admin/benutzer/root", None).await;
        assert_eq!(loeschen.status, StatusCode::CONFLICT);
        let herab = p
            .admin(
                "PUT",
                "/admin/benutzer/root",
                Some(json!(BenutzerAendern {
                    rolle: Some(Rolle::Benutzer),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(herab.status, StatusCode::CONFLICT);

        // Der alte Admin-Schlüssel öffnet nichts mehr.
        let alt = p
            .anfrage(
                "GET",
                "/admin/ueberblick",
                Some("admin-token-fuer-den-test"),
                None,
                &[],
                None,
            )
            .await;
        assert_eq!(alt.status, StatusCode::UNAUTHORIZED);

        // Die Datei wird beim Root-Setzen entfernt.
        assert!(!p.daten.join("admin-token").exists());
    })
    .await;
}

#[tokio::test]
async fn mit_entra_gibt_es_keine_einmal_codes() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let anmelden = p
            .anfrage(
                "POST",
                "/anmelden",
                None,
                None,
                &[],
                Some(serde_json::to_vec(&json!({"code": "ABCD-2345", "name": "x"})).unwrap()),
            )
            .await;
        assert_eq!(anmelden.status, StatusCode::NOT_FOUND);
        let codes = p
            .admin(
                "POST",
                "/admin/codes",
                Some(json!(CodeAnlegen {
                    gruppen: Vec::new(),
                    minuten: None,
                    unbegrenzt: false
                })),
            )
            .await;
        assert_eq!(codes.status, StatusCode::NOT_FOUND);
        let arten = p
            .anfrage("GET", "/anmeldearten", None, None, &[], None)
            .await;
        assert_eq!(arten.status, StatusCode::OK);
        assert_eq!(arten.json["entra"], true);
        assert_eq!(arten.json["root"], true);
    })
    .await;
}

#[tokio::test]
async fn altgeraete_ohne_benutzer_nur_wenn_erlaubt() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        // Einen Code-Weg ohne Entra benutzen: Gruppe, Code, Anmelden.
        let gruppe = qd_gruppe();
        assert_eq!(
            p.admin("PUT", "/admin/gruppen/team", Some(json!(gruppe)))
                .await
                .status,
            StatusCode::OK
        );
        let code = p
            .admin(
                "POST",
                "/admin/codes",
                Some(json!(CodeAnlegen {
                    gruppen: vec![quiver_domain::GruppeId::neu("team")],
                    minuten: Some(60),
                    unbegrenzt: false
                })),
            )
            .await;
        let code: quiver_protocol::Code = serde_json::from_value(code.json).expect("Code");
        let anmelden = p
            .anfrage(
                "POST",
                "/anmelden",
                None,
                None,
                &[],
                Some(serde_json::to_vec(&json!({"code": code.code, "name": "Alt"})).expect("JSON")),
            )
            .await;
        assert_eq!(anmelden.status, StatusCode::OK);
        let antwort: AnmeldenAntwort = serde_json::from_value(anmelden.json).expect("Antwort");

        // Erlaubt: das Gerät darf.
        let erlaubt = p
            .anfrage("GET", "/geraet/soll", Some(&antwort.token), None, &[], None)
            .await;
        assert_eq!(erlaubt.status, StatusCode::OK);

        // Gesperrt: 403 mit grund=neu_anmelden.
        let gesperrt = p.neu_oeffentlich(&p.oeffentlich, false).await;
        let antwort_gesperrt = gesperrt
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/geraet/soll")
                    .header("host", HOST)
                    .header("authorization", format!("Bearer {}", antwort.token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = {
            let status = antwort_gesperrt.status();
            let text = antwort_gesperrt
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&text).unwrap_or(Value::Null),
            )
        };
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(json["grund"], "neu-anmelden");
    })
    .await;
}

#[derive(Clone, Default)]
struct Mitlesen(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Mitlesen {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Mitlesen {
    type Writer = Mitlesen;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Mitlesen {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn kein_geheimnis_im_log() {
    let puffer = Mitlesen::default();
    let abonnent = tracing_subscriber::fmt()
        .with_writer(puffer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _wache = tracing::subscriber::set_default(abonnent);
    // Der Interesse-Cache entscheidet, ob ein Aufrufpunkt überhaupt schreibt;
    // viele Tests laufen gleichzeitig durch dieselben Zeilen.
    tracing::callsite::rebuild_interest_cache();

    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let _ = p.geraet_anmelden("Rechner", 51234).await;
        // Auch ein Fehlversuch darf kein Geheimnis ins Log schreiben.
        let _ = p
            .anfrage(
                "POST",
                "/auth/root",
                None,
                None,
                &[("origin", &p.oeffentlich), ("x-quiver", "1")],
                Some(
                    serde_json::to_vec(&json!({
                        "email": ROOT_EMAIL,
                        "passwort": "falsches-passwort-123"
                    }))
                    .unwrap(),
                ),
            )
            .await;
    })
    .await;
    let text = puffer.text();
    for geheim in [ENTRA_SECRET, ROOT_PASSWORT, "refresh-1", "refresh-2"] {
        assert!(
            !text.contains(geheim),
            "Geheimnis {geheim} steht im Log: {text}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn kein_schluessel_im_log() {
    let puffer = Mitlesen::default();
    let abonnent = tracing_subscriber::fmt()
        .with_writer(puffer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _wache = tracing::subscriber::set_default(abonnent);
    tracing::callsite::rebuild_interest_cache();

    let mut schluessel_pem = String::new();
    mit_zeitlimit(async {
        // Mit Zertifikat: der private Schlüssel darf nirgends auftauchen
        // (Aufgabe 23), auch nicht im Log.
        let p = Pruefung::mit_entra_schluessel(365).await;
        schluessel_pem = std::fs::read_to_string(p.key_pfad()).expect("Schlüssel");
        let _ = p.geraet_anmelden("Rechner", 51236).await;
        let _ = p.admin("GET", "/admin/entra-schluessel", None).await;
    })
    .await;
    let text = puffer.text();
    assert!(
        !text.contains("PRIVATE KEY"),
        "Schlüssel steht im Log: {text}"
    );
    assert!(
        !text.contains(schluessel_pem.trim()),
        "Schlüsselmaterial steht im Log"
    );
}

fn qd_gruppe() -> quiver_domain::Gruppe {
    quiver_domain::Gruppe {
        id: quiver_domain::GruppeId::neu("team"),
        titel: "Team".into(),
        saetze: Vec::new(),
        entra: Vec::new(),
    }
}

#[tokio::test]
async fn keine_geheimnisse_in_antworten_und_pruefspur() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        let antwort = p.geraet_anmelden("Rechner", 51234).await;

        let mut ausgaben = Vec::new();
        for (methode, pfad) in [
            ("GET", "/admin/benutzer"),
            ("GET", "/admin/entra-gruppen"),
            ("GET", "/admin/pruefspur"),
            ("GET", "/admin/ueberblick"),
        ] {
            let antwort = p.admin(methode, pfad, None).await;
            assert_eq!(antwort.status, StatusCode::OK, "{pfad}");
            ausgaben.push(antwort.text);
        }
        // Die Prüfspur nennt Vorgänge, aber keine Geheimnisse.
        let pruefspur = &ausgaben[2];
        assert!(pruefspur.contains("anmeldung") || pruefspur.contains("geraet"));
        for geheim in [ENTRA_SECRET, ROOT_PASSWORT, "refresh-1", "refresh-2"] {
            for ausgabe in &ausgaben {
                assert!(
                    !ausgabe.contains(geheim),
                    "Geheimnis {geheim} steht in einer Antwort"
                );
            }
        }
        // Das Geräte-Token erscheint nur in der Anmeldeantwort selbst.
        assert!(!ausgaben.iter().any(|a| a.contains(&antwort.token)));
    })
    .await;
}

// ----------------------------------------------- Aufgabe 26, Punkt 1 und 6

/// Zeitlimit je Test, der auf Server oder Nachbau wartet: Hängen ist ein
/// Fehlschlag (Aufgabe 26, Punkt 7).
async fn mit_zeitlimit<T>(was: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(120), was)
        .await
        .expect("Test hängt: Zeitlimit überschritten")
}

#[tokio::test]
async fn gesperrter_benutzer_verliert_seine_sitzungen_sofort() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        p.geraet_anmelden("Rechner", 51234).await;
        let ernannt = p
            .admin(
                "PUT",
                &format!("/admin/benutzer/{OID}"),
                Some(json!(BenutzerAendern {
                    rolle: Some(Rolle::Admin),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(ernannt.status, StatusCode::OK, "{}", ernannt.text);
        let cookie = p.verwaltungs_anmeldung().await;
        let vorher = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(vorher.status, StatusCode::OK, "{}", vorher.text);

        // Sperren: die Sitzung endet sofort am Server (löschen, nicht nur
        // ablehnen) – die nächste Anfrage ist 401, nicht 403.
        let gesperrt = p
            .admin(
                "PUT",
                &format!("/admin/benutzer/{OID}"),
                Some(json!(BenutzerAendern {
                    gesperrt: Some(true),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(gesperrt.status, StatusCode::OK, "{}", gesperrt.text);

        let danach = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(
            danach.status,
            StatusCode::UNAUTHORIZED,
            "Sitzung ist gelöscht: {}",
            danach.text
        );

        // Auch `/auth/ich` kennt die Sitzung nicht mehr.
        let ich = p
            .anfrage("GET", "/auth/ich", None, Some(&cookie), &[], None)
            .await;
        assert_eq!(ich.status, StatusCode::UNAUTHORIZED);

        // In der Datenbank steht keine Sitzung dieses Benutzers mehr.
        let conn = rusqlite::Connection::open(p.daten.join("quiver.db")).expect("DB");
        let anzahl: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sitzungen WHERE oid = ?1",
                rusqlite::params![OID],
                |zeile| zeile.get(0),
            )
            .expect("Zahl");
        assert_eq!(anzahl, 0, "gesperrter Benutzer hat keine Sitzung mehr");
    })
    .await;
}

#[tokio::test]
async fn wache_prueft_nach_60_minuten_nach_und_beendet_dann() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        p.geraet_anmelden("Rechner", 51234).await;
        let ernannt = p
            .admin(
                "PUT",
                &format!("/admin/benutzer/{OID}"),
                Some(json!(BenutzerAendern {
                    rolle: Some(Rolle::Admin),
                    ..Default::default()
                })),
            )
            .await;
        assert_eq!(ernannt.status, StatusCode::OK);
        let cookie = p.verwaltungs_anmeldung().await;
        let mock = p.mock.clone().expect("Nachbau");
        let vorher = mock.zaehler().2;

        // Kurz danach: keine Nachprüfung, die Wache lässt durch.
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::minutes(1));
        let frueh = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(frueh.status, StatusCode::OK, "{}", frueh.text);
        assert_eq!(mock.zaehler().2, vorher, "noch keine Nachprüfung");

        // Mehr als 60 Minuten seit der letzten Prüfung (die Sitzung selbst lebt
        // im Leerlauf 60 Minuten, deshalb in zwei Schritten): die Wache prüft
        // selbst nach und lässt durch.
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::minutes(50));
        let zwischen = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(zwischen.status, StatusCode::OK, "{}", zwischen.text);
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::minutes(100));
        let nach = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(nach.status, StatusCode::OK, "{}", nach.text);
        assert_eq!(
            mock.zaehler().2,
            vorher + 1,
            "die Wache hat vor dem Durchlassen nachgeprüft"
        );

        // Verweigert Entra die Auffrischung, enden die Sitzungen sofort. Die
        // Sitzung lebt im Leerlauf 60 Minuten, die Prüfung ist nach 60 Minuten
        // fällig – deshalb dazwischen ab und zu benutzen.
        mock.refresh_fehler("invalid_grant");
        for minuten in [120, 150] {
            p.uhr
                .setze(OffsetDateTime::now_utc() + time::Duration::minutes(minuten));
            let weiter = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
            assert_eq!(
                weiter.status,
                StatusCode::OK,
                "nach {minuten} Minuten ohne fällige Prüfung: {}",
                weiter.text
            );
        }
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::minutes(165));
        let abgewiesen = p.admin_mit(&cookie, "GET", "/admin/ueberblick", None).await;
        assert_eq!(
            abgewiesen.status,
            StatusCode::UNAUTHORIZED,
            "{}",
            abgewiesen.text
        );
        let conn = rusqlite::Connection::open(p.daten.join("quiver.db")).expect("DB");
        let anzahl: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sitzungen WHERE oid = ?1",
                rusqlite::params![OID],
                |zeile| zeile.get(0),
            )
            .expect("Zahl");
        assert_eq!(anzahl, 0, "die Sitzung ist gelöscht, nicht nur abgelehnt");
    })
    .await;
}

#[tokio::test]
async fn discovery_wird_nach_24_stunden_erneuert_und_haelt_den_alten_stand() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_anlegen().await;
        p.geraet_anmelden("Rechner", 51234).await;
        let mock = p.mock.clone().expect("Nachbau");
        let erster_stand = mock.discovery_zaehler();
        assert!(erster_stand >= 1, "die Discovery wurde geholt");

        // Keine 24 Stunden: kein neues Holen.
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::hours(23));
        p.geraet_anmelden("Zweiter", 51234).await;
        assert_eq!(
            mock.discovery_zaehler(),
            erster_stand,
            "innerhalb von 24 Stunden gilt die Discovery weiter"
        );

        // Nach 24 Stunden wird neu geholt – der Nachbau antwortet nicht, der
        // alte Stand gilt weiter (mit Warnung), die Anmeldung klappt trotzdem.
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::hours(25));
        mock.discovery_aus(true);
        let vor_dem_versuch = mock.discovery_zaehler();
        p.geraet_anmelden("Dritter", 51234).await;
        assert!(
            mock.discovery_zaehler() > vor_dem_versuch,
            "nach 24 Stunden wurde es erneut versucht"
        );

        // Ist der Nachbau wieder da, wird der neue Stand genommen.
        mock.discovery_aus(false);
        p.uhr
            .setze(OffsetDateTime::now_utc() + time::Duration::hours(50));
        let vor_dem_neuen = mock.discovery_zaehler();
        p.geraet_anmelden("Vierter", 51234).await;
        assert!(mock.discovery_zaehler() > vor_dem_neuen);
    })
    .await;
}

// ------------------------------------------- Aufgabe 23: Zertifikat statt Geheimnis

/// Eine Assertion mit fremdem Schlüssel (für die Gegenprobe des Nachbaus).
fn fremder_token(basis: &str, x5t: &str, alg: Algorithm) -> String {
    let mut rng = getrandom::SysRng;
    let key = PrivateKey::new(&mut rng, 2048).expect("RSA");
    let mut kopf = Header::new(alg);
    kopf.x5t_s256 = Some(x5t.to_string());
    let jetzt = OffsetDateTime::now_utc().unix_timestamp();
    let claims = json!({
        "aud": format!("{basis}/token"),
        "iss": CLIENT,
        "sub": CLIENT,
        "jti": format!("fremd-{jetzt}"),
        "nbf": jetzt - 10,
        "iat": jetzt - 10,
        "exp": jetzt + 200,
    });
    let pem = key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).expect("PEM");
    encode(
        &kopf,
        &claims,
        &EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap(),
    )
    .expect("JWT")
}

async fn formular_an_token(p: &Pruefung, mock: &Mock, assertion: &str) -> reqwest::Response {
    p.http
        .post(format!("{}/token", mock.basis))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT),
            ("code", "egal"),
            (
                "client_assertion_type",
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            ),
            ("client_assertion", assertion),
        ])
        .send()
        .await
        .expect("Mock erreichbar")
}

#[tokio::test]
async fn client_assertion_wird_streng_geprueft() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra_schluessel(365).await;
        let mock = p.mock.clone().expect("Mock");
        let key_pem = std::fs::read_to_string(p.key_pfad()).expect("Schlüssel");

        // Code-Einlösung und Refresh: je eine eigene, geprüfte Assertion.
        let _login = p.geraet_anmelden("Rechner", 51234).await;
        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::OK, "{}", geprueft.text);
        assert_eq!(mock.assertion_fehler(), None);
        assert!(
            mock.assertionen() >= 2,
            "nur {} Assertions",
            mock.assertionen()
        );

        // Gegenprobe des Nachbaus: fremder Schlüssel und falscher Algorithmus
        // werden abgelehnt (der Nachbau prüft also wirklich).
        let schluessel = quiver_server::EntraSchluessel::laden(&p.key_pfad()).expect("Schlüssel");
        let fremd = fremder_token(&mock.basis, schluessel.x5t_s256(), Algorithm::PS256);
        let antwort = formular_an_token(&p, &mock, &fremd).await;
        assert_eq!(antwort.status(), StatusCode::BAD_REQUEST);
        assert!(
            mock.assertion_fehler()
                .unwrap_or_default()
                .contains("abgelehnt"),
            "{:?}",
            mock.assertion_fehler()
        );

        let falscher_alg = fremder_token(&mock.basis, schluessel.x5t_s256(), Algorithm::RS256);
        let antwort = formular_an_token(&p, &mock, &falscher_alg).await;
        assert_eq!(antwort.status(), StatusCode::BAD_REQUEST);
        assert!(
            mock.assertion_fehler()
                .unwrap_or_default()
                .contains("Algorithmus"),
            "{:?}",
            mock.assertion_fehler()
        );

        // Der private Schlüssel steht in keiner Antwort und keinem Debug.
        let mut ausgaben = Vec::new();
        for pfad in [
            "/admin/benutzer",
            "/admin/entra-gruppen",
            "/admin/pruefspur",
            "/admin/ueberblick",
            "/admin/entra-schluessel",
            "/admin/gruppen",
        ] {
            let antwort = p.admin("GET", pfad, None).await;
            assert_eq!(antwort.status, StatusCode::OK, "{pfad}: {}", antwort.text);
            ausgaben.push(antwort.text);
        }
        for text in &ausgaben {
            assert!(!text.contains("PRIVATE KEY"), "Schlüssel in einer Antwort");
            assert!(
                !text.contains(key_pem.trim()),
                "Schlüsselmaterial in einer Antwort"
            );
        }
        assert!(!format!("{:?}", p.ausweis).contains("PRIVATE"));
        assert!(!format!("{schluessel:?}").contains("PRIVATE KEY"));
    })
    .await;
}

#[tokio::test]
async fn zertifikatswarnung_ab_dreissig_tagen_hoechstens_einmal_je_tag() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra_schluessel(10).await;
        for _ in 0..2 {
            let antwort = p.admin("GET", "/admin/entra-schluessel", None).await;
            assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
            assert_eq!(antwort.json["art"], "schluessel");
            assert_eq!(antwort.json["warnung"], true);
            assert!(
                antwort.json["tage_uebrig"].as_i64().unwrap_or(999) <= 10,
                "{}",
                antwort.json
            );
            assert!(antwort.json["fingerabdruck"]
                .as_str()
                .unwrap_or_default()
                .contains(':'));
            assert!(antwort.json["hinweis"]
                .as_str()
                .unwrap_or_default()
                .contains("gilt noch"));
        }
        let spur = p.admin("GET", "/admin/pruefspur", None).await;
        let hinweise = spur
            .json
            .as_array()
            .expect("Liste")
            .iter()
            .filter(|eintrag| {
                eintrag["art"] == "hinweis"
                    && eintrag["text"]
                        .as_str()
                        .unwrap_or_default()
                        .starts_with("zertifikat")
            })
            .count();
        assert_eq!(
            hinweise, 1,
            "Warnung steht mehr als einmal je Tag in der Prüfspur"
        );
    })
    .await;
}

#[tokio::test]
async fn ohne_warnung_kein_hinweis() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra_schluessel(365).await;
        let antwort = p.admin("GET", "/admin/entra-schluessel", None).await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        assert_eq!(antwort.json["warnung"], false);
        assert!(antwort.json["hinweis"].is_null());
        let spur = p.admin("GET", "/admin/pruefspur", None).await;
        assert!(!spur.text.contains("zertifikat:"));
    })
    .await;
}

#[tokio::test]
async fn geheimnis_und_zertifikat_sind_gegenseitig_ausgeschlossen() {
    // Ohne Entra ist der Stand „keine“; das ist kein Fehler.
    let p = Pruefung::neu().await;
    let antwort = p.admin("GET", "/admin/entra-schluessel", None).await;
    assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
    assert_eq!(antwort.json["art"], "keine");

    let p = Pruefung::mit_entra().await;
    let antwort = p.admin("GET", "/admin/entra-schluessel", None).await;
    assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
    assert_eq!(antwort.json["art"], "geheimnis");
    assert_eq!(antwort.json["warnung"], false);
}

// ------------------------------------ Aufgabe 23: Gruppen aus Entra von selbst

#[tokio::test]
async fn entra_gruppe_wird_einmal_angelegt_und_behaelt_ihren_titel() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let erst = p.geraet_anmelden("A", 51234).await;
        assert_eq!(
            erst.geraet.gruppen,
            vec![quiver_domain::GruppeId::neu(GRUPPE_ENTRA)],
            "die gesehene Id wird zur quiver-Gruppe"
        );
        let gruppen = p.gruppen().await;
        assert_eq!(gruppen.len(), 1);
        let gruppe = &gruppen[0];
        assert_eq!(gruppe.id.als_str(), GRUPPE_ENTRA);
        assert_eq!(gruppe.titel, "Entra-Gruppe 12345678");
        assert!(
            gruppe.saetze.is_empty(),
            "eine neue Gruppe gibt keine Tools"
        );
        assert_eq!(gruppe.entra, vec![String::from(GRUPPE_ENTRA)]);

        // Zweite Anmeldung: keine zweite Gruppe, kein anderes Aussehen.
        let _zweit = p.geraet_anmelden("B", 51235).await;
        assert_eq!(p.gruppen().await.len(), 1);

        // Der Admin benennt um; kein Abgleich überschreibt den Titel.
        let mut umbenannt = gruppe.clone();
        umbenannt.titel = String::from("Team");
        let antwort = p
            .admin(
                "PUT",
                &format!("/admin/gruppen/{GRUPPE_ENTRA}"),
                Some(json!(umbenannt)),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        let _dritt = p.geraet_anmelden("C", 51236).await;
        let nach = p.gruppen().await;
        assert_eq!(nach.len(), 1);
        assert_eq!(nach[0].titel, "Team");

        // Prüfspur: „angelegt durch Anmeldung“; die Verwaltung sieht die Id.
        let spur = p.admin("GET", "/admin/pruefspur", None).await;
        assert!(
            spur.text.contains("angelegt durch Anmeldung"),
            "{}",
            spur.text
        );
        let gesehen = p.entra_gruppen().await;
        assert_eq!(gesehen.len(), 1);
        assert_eq!(gesehen[0].id, GRUPPE_ENTRA);
        assert_eq!(gesehen[0].gruppe.as_deref(), Some(GRUPPE_ENTRA));
        assert!(!gesehen[0].geloescht);
        assert_eq!(gesehen[0].anzeigename.as_deref(), Some("Team"));
    })
    .await;
}

#[tokio::test]
async fn entzug_nimmt_tools_am_verbundenen_geraet_weg() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        p.eintrag_und_satz_anlegen().await;
        let login = p.geraet_anmelden("Rechner", 51234).await;
        // Die automatisch angelegte Gruppe bekommt einen Satz.
        let mut gruppe = p.gruppen().await.pop().expect("Gruppe");
        gruppe.saetze = vec![SatzId::neu("basis")];
        let antwort = p
            .admin(
                "PUT",
                &format!("/admin/gruppen/{}", gruppe.id),
                Some(json!(gruppe)),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        let soll = p
            .anfrage("GET", "/geraet/soll", Some(&login.token), None, &[], None)
            .await;
        assert_eq!(soll.status, StatusCode::OK, "{}", soll.text);
        let eintraege = soll.json["eintraege"].as_array().expect("Liste");
        assert_eq!(eintraege.len(), 1, "{}", soll.text);
        assert_eq!(eintraege[0]["eintrag"]["id"], "wetter");

        // Entra nimmt die Gruppe weg: das Gerät ist noch verbunden, verliert
        // aber sofort seine Tools. Die neue Gruppe hat keinen Satz.
        p.mock
            .as_ref()
            .expect("Mock")
            .setze_refresh_claims(|claims| {
                claims["groups"] = json!([GRUPPE_NEU]);
            });
        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::OK, "{}", geprueft.text);
        let soll = p
            .anfrage("GET", "/geraet/soll", Some(&login.token), None, &[], None)
            .await;
        assert_eq!(soll.status, StatusCode::OK, "{}", soll.text);
        assert!(
            soll.json["eintraege"].as_array().expect("Liste").is_empty(),
            "{}",
            soll.text
        );
        let gruppen = p.gruppen().await;
        assert!(gruppen
            .iter()
            .any(|g| g.id.als_str() == GRUPPE_NEU && g.saetze.is_empty()));
    })
    .await;
}

#[tokio::test]
async fn geloeschte_gruppe_bleibt_geloescht_und_wird_wieder_aufgenommen() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let _login = p.geraet_anmelden("A", 51234).await;
        let weg = p
            .admin("DELETE", &format!("/admin/gruppen/{GRUPPE_ENTRA}"), None)
            .await;
        assert_eq!(weg.status, StatusCode::NO_CONTENT, "{}", weg.text);

        // Eine neue Anmeldung legt sie nicht wieder an.
        let (code, state, oidc) = p.oidc(false, "B", 51235).await;
        let seite = p.callback(&code, &state, &oidc).await;
        assert_eq!(seite.status, StatusCode::OK, "{}", seite.text);
        assert!(p.gruppen().await.is_empty(), "gelöscht bleibt gelöscht");
        let gesehen = p.entra_gruppen().await;
        assert_eq!(gesehen.len(), 1);
        assert!(gesehen[0].geloescht);
        assert!(gesehen[0].gruppe.is_none());

        // Die Verwaltung nimmt sie wieder auf: eine Gruppe, ohne Sätze.
        let aufgenommen = p
            .admin(
                "PUT",
                &format!("/admin/entra-gruppen/{GRUPPE_ENTRA}"),
                Some(json!({"aufnehmen": true})),
            )
            .await;
        assert_eq!(aufgenommen.status, StatusCode::OK, "{}", aufgenommen.text);
        assert_eq!(aufgenommen.json["geloescht"], false);
        assert_eq!(aufgenommen.json["gruppe"], GRUPPE_ENTRA);
        let gruppen = p.gruppen().await;
        assert_eq!(gruppen.len(), 1);
        assert!(gruppen[0].saetze.is_empty());

        // Danach bleibt es bei genau einer Gruppe.
        let _dritt = p.geraet_anmelden("C", 51236).await;
        assert_eq!(p.gruppen().await.len(), 1);
    })
    .await;
}

#[tokio::test]
async fn neue_gruppe_ohne_satz_gibt_keine_tools() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let login = p.geraet_anmelden("Rechner", 51234).await;
        assert_eq!(login.geraet.gruppen.len(), 1);
        let soll = p
            .anfrage("GET", "/geraet/soll", Some(&login.token), None, &[], None)
            .await;
        assert_eq!(soll.status, StatusCode::OK, "{}", soll.text);
        assert!(
            soll.json["eintraege"].as_array().expect("Liste").is_empty(),
            "eine Gruppe ohne Satz gibt keine Tools: {}",
            soll.text
        );
    })
    .await;
}

#[tokio::test]
async fn gleichzeitige_anmeldungen_legen_genau_eine_gruppe_an() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let (a, b) = tokio::join!(p.geraet_anmelden("A", 51234), p.geraet_anmelden("B", 51235));
        assert_eq!(a.geraet.gruppen, b.geraet.gruppen);
        assert_eq!(p.gruppen().await.len(), 1);
        assert_eq!(p.entra_gruppen().await.len(), 1);
    })
    .await;
}

// ------------------------------- Aufgabe 30: Namen der Entra-Gruppen

/// Wartet, bis der Name aus Entra ankommt (Hintergrund, deshalb mit Frist).
async fn name_warten(p: &Pruefung, id: &str, erwartet: &str) {
    for _ in 0..200 {
        let liste = p.entra_gruppen().await;
        if liste
            .iter()
            .any(|zeile| zeile.id == id && zeile.name_entra.as_deref() == Some(erwartet))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let liste = p.entra_gruppen().await;
    panic!("Name {erwartet} kam nicht an: {liste:?}");
}

/// Wartet, bis mindestens eine Graph-Abfrage lief, und gibt ihr Zeit, zu wirken.
async fn graph_abwarten(p: &Pruefung) {
    for _ in 0..200 {
        if !p.mock.as_ref().expect("Mock").graph_ids().is_empty() {
            tokio::time::sleep(Duration::from_millis(400)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("keine Graph-Abfrage");
}

fn titel_der_gruppe(gruppen: &[quiver_domain::Gruppe], id: &str) -> String {
    gruppen
        .iter()
        .find(|gruppe| gruppe.id.als_str() == id)
        .map(|gruppe| gruppe.titel.clone())
        .unwrap_or_default()
}

async fn toter_port() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Port frei");
    let adresse = listener.local_addr().expect("Adresse");
    drop(listener);
    format!("http://{adresse}")
}

#[tokio::test]
async fn ohne_schalter_kein_scope_und_keine_anfrage() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let _login = p.geraet_anmelden("A", 51234).await;
        // Auch das Nachprüfen (Refresh) darf keinen Graph-Bereich anfordern.
        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::OK, "{}", geprueft.text);
        tokio::time::sleep(Duration::from_millis(300)).await;

        let mock = p.mock.as_ref().expect("Mock");
        assert!(
            mock.graph_ids().is_empty(),
            "ohne Schalter keine Graph-Anfrage: {:?}",
            mock.graph_ids()
        );
        for scope in mock.scopes().iter().chain(mock.refresh_scopes().iter()) {
            assert!(!scope.contains("GroupMember"), "scope: {scope}");
            assert_eq!(scope, "openid profile email offline_access");
        }
        assert!(!mock.refresh_scopes().is_empty(), "Refresh lief");
    })
    .await;
}

#[tokio::test]
async fn mit_schalter_kommt_der_name_und_folgt_der_umbenennung() {
    mit_zeitlimit(async {
        let mut p = Pruefung::mit_entra_namen().await;
        let _login = p.geraet_anmelden("A", 51234).await;
        name_warten(&p, GRUPPE_ENTRA, "Team Alpha").await;

        let gruppen = p.gruppen().await;
        assert_eq!(
            titel_der_gruppe(&gruppen, GRUPPE_ENTRA),
            "Team Alpha",
            "der Name ersetzt den automatischen Titel"
        );
        let gesehen = p.entra_gruppen().await;
        assert_eq!(gesehen[0].name_entra.as_deref(), Some("Team Alpha"));
        assert!(gesehen[0].name_entra_am.is_some(), "mit Zeitpunkt");

        let mock = p.mock.as_ref().expect("Mock");
        assert!(
            mock.scopes()
                .iter()
                .any(|scope| scope.contains(quiver_server::GRAPH_SCOPE)),
            "Anmeldung fordert den Lesebereich an: {:?}",
            mock.scopes()
        );

        // Umbenennen in Entra: nach 24 Stunden (und einer Prüfung) folgt der Titel.
        mock.setze_graph(GraphVerhalten::Name(String::from("Team Beta")));
        p.uhr.setze(p.uhr.jetzt() + time::Duration::hours(25));
        p.cookie = p.root_anmelden().await;
        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::OK, "{}", geprueft.text);
        name_warten(&p, GRUPPE_ENTRA, "Team Beta").await;
        assert_eq!(
            titel_der_gruppe(&p.gruppen().await, GRUPPE_ENTRA),
            "Team Beta"
        );
        assert!(
            mock.refresh_scopes()
                .iter()
                .any(|scope| scope.contains(quiver_server::GRAPH_SCOPE)),
            "Refresh fordert den Lesebereich an: {:?}",
            mock.refresh_scopes()
        );
        let spur = p.admin("GET", "/admin/pruefspur", None).await;
        assert!(
            spur.text.contains("Titel aus Entra übernommen"),
            "{}",
            spur.text
        );
    })
    .await;
}

#[tokio::test]
async fn admin_titel_bleibt_stehen() {
    mit_zeitlimit(async {
        let mut p = Pruefung::mit_entra_namen().await;
        let _login = p.geraet_anmelden("A", 51234).await;
        name_warten(&p, GRUPPE_ENTRA, "Team Alpha").await;

        // Der Admin benennt um; das gehört ihm.
        let mut gruppe = p.gruppen().await.pop().expect("Gruppe");
        gruppe.titel = String::from("Mein Team");
        let antwort = p
            .admin(
                "PUT",
                &format!("/admin/gruppen/{GRUPPE_ENTRA}"),
                Some(json!(gruppe)),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);

        p.mock
            .as_ref()
            .expect("Mock")
            .setze_graph(GraphVerhalten::Name(String::from("Team Beta")));
        p.uhr.setze(p.uhr.jetzt() + time::Duration::hours(25));
        p.cookie = p.root_anmelden().await;
        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::OK, "{}", geprueft.text);
        name_warten(&p, GRUPPE_ENTRA, "Team Beta").await;

        let gruppen = p.gruppen().await;
        assert_eq!(
            titel_der_gruppe(&gruppen, GRUPPE_ENTRA),
            "Mein Team",
            "der Server überschreibt keinen Admin-Titel"
        );
        // Der neue Name steht trotzdem als Anzeige bereit.
        let gesehen = p.entra_gruppen().await;
        assert_eq!(gesehen[0].name_entra.as_deref(), Some("Team Beta"));
        assert_eq!(gesehen[0].anzeigename.as_deref(), Some("Mein Team"));
    })
    .await;
}

#[tokio::test]
async fn verboten_pausiert_einen_tag_und_aendert_nichts() {
    mit_zeitlimit(async {
        let mut p = Pruefung::mit_entra_namen().await;
        p.mock
            .as_ref()
            .expect("Mock")
            .setze_graph(GraphVerhalten::Status(403));
        let _login = p.geraet_anmelden("A", 51234).await;
        graph_abwarten(&p).await;

        let mock = p.mock.as_ref().expect("Mock");
        assert_eq!(mock.graph_ids(), vec![String::from(GRUPPE_ENTRA)]);
        let gesehen = p.entra_gruppen().await;
        assert!(gesehen[0].name_entra.is_none(), "403 ändert nichts");
        assert_eq!(
            gesehen[0].anzeigename.as_deref(),
            Some("Entra-Gruppe 12345678")
        );

        // Zweite Anmeldung: ein Tag Pause, keine neue Anfrage.
        let _zweit = p.geraet_anmelden("B", 51235).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            p.mock.as_ref().expect("Mock").graph_ids().len(),
            1,
            "nach 403 wird einen Tag nicht gefragt"
        );

        // Nach dem Tag wieder – dann klappt es auch.
        p.mock
            .as_ref()
            .expect("Mock")
            .setze_graph(GraphVerhalten::Name(String::from("Team Alpha")));
        p.uhr.setze(p.uhr.jetzt() + time::Duration::hours(25));
        p.cookie = p.root_anmelden().await;
        let _dritt = p.geraet_anmelden("C", 51236).await;
        name_warten(&p, GRUPPE_ENTRA, "Team Alpha").await;
        assert!(
            p.mock.as_ref().expect("Mock").graph_ids().len() >= 2,
            "nach der Pause wird wieder gefragt"
        );
    })
    .await;
}

#[tokio::test]
async fn fehler_und_besondere_antworten_aendern_nichts() {
    mit_zeitlimit(async {
        for (verhalten, was) in [
            (GraphVerhalten::Status(404), "404"),
            (GraphVerhalten::Status(500), "500"),
            (GraphVerhalten::Weiterleitung, "Weiterleitung"),
            (GraphVerhalten::FremdeId, "fremde Id"),
            (GraphVerhalten::Riesig, "riesige Antwort"),
        ] {
            let p = Pruefung::mit_entra_namen().await;
            p.mock
                .as_ref()
                .expect("Mock")
                .setze_graph(verhalten.clone());
            let _login = p.geraet_anmelden("A", 51234).await;
            graph_abwarten(&p).await;
            let gesehen = p.entra_gruppen().await;
            assert!(
                gesehen[0].name_entra.is_none(),
                "{was} darf nichts ändern: {gesehen:?}"
            );
            assert_eq!(
                gesehen[0].anzeigename.as_deref(),
                Some("Entra-Gruppe 12345678"),
                "{was}"
            );
        }

        // Ein böser Name wird gesäubert (Steuer- und Richtungszeichen weg).
        let p = Pruefung::mit_entra_namen().await;
        p.mock
            .as_ref()
            .expect("Mock")
            .setze_graph(GraphVerhalten::BoeserName);
        let _login = p.geraet_anmelden("A", 51234).await;
        name_warten(&p, GRUPPE_ENTRA, "Team Alpha").await;
        assert_eq!(
            titel_der_gruppe(&p.gruppen().await, GRUPPE_ENTRA),
            "Team Alpha"
        );

        // Ein Zeitlimit lässt die Anmeldung in Ruhe.
        let p = Pruefung::mit_entra_namen().await;
        p.mock
            .as_ref()
            .expect("Mock")
            .setze_graph(GraphVerhalten::Langsam);
        let _login = p.geraet_anmelden("A", 51234).await;
        graph_abwarten(&p).await;
        // Zeitlimit (5 s) plus etwas Luft.
        tokio::time::sleep(Duration::from_secs(6)).await;
        let gesehen = p.entra_gruppen().await;
        assert!(gesehen[0].name_entra.is_none(), "Zeitlimit ändert nichts");
    })
    .await;
}

#[tokio::test]
async fn anmeldung_wartet_nicht_auf_graph() {
    mit_zeitlimit(async {
        // Graph antwortet erst nach sechs Sekunden; die Anmeldung darf nicht
        // darauf warten.
        let p = Pruefung::mit_entra_namen().await;
        p.mock
            .as_ref()
            .expect("Mock")
            .setze_graph(GraphVerhalten::Langsam);
        let start = std::time::Instant::now();
        let login = p.geraet_anmelden("A", 51234).await;
        let gedauert = start.elapsed();
        assert_eq!(login.geraet.gruppen.len(), 1);
        assert!(
            gedauert < Duration::from_secs(2),
            "Anmeldung dauerte {gedauert:?}"
        );

        // Und mit einem toten Graph erst recht nicht.
        let tot = toter_port().await;
        let p = Pruefung::mit_entra_totem_graph(&tot).await;
        let start = std::time::Instant::now();
        let login = p.geraet_anmelden("B", 51235).await;
        assert_eq!(login.geraet.gruppen.len(), 1);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "toter Graph hält die Anmeldung auf"
        );
    })
    .await;
}

#[tokio::test]
async fn zugriffstoken_steht_in_keiner_ausgabe() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra_namen().await;
        let _login = p.geraet_anmelden("A", 51234).await;
        name_warten(&p, GRUPPE_ENTRA, "Team Alpha").await;
        let mock = p.mock.as_ref().expect("Mock");
        assert_eq!(
            mock.graph_tokens(),
            vec![String::from(GRAPH_TOKEN)],
            "das Token kam bei Graph an"
        );

        // Datenbank: keine Spur des Tokens, auch nicht im WAL.
        let mut inhalt = Vec::new();
        for eintrag in std::fs::read_dir(&p.daten).expect("Datenordner").flatten() {
            if let Ok(bytes) = std::fs::read(eintrag.path()) {
                inhalt.extend_from_slice(&bytes);
            }
        }
        let text = String::from_utf8_lossy(&inhalt);
        assert!(!text.contains(GRAPH_TOKEN), "Token steht in der Datenbank");

        // Prüfspur und Verwaltungsantworten.
        let spur = p.admin("GET", "/admin/pruefspur", None).await;
        assert!(!spur.text.contains(GRAPH_TOKEN), "{}", spur.text);
        let gruppen = p.admin("GET", "/admin/entra-gruppen", None).await;
        assert!(!gruppen.text.contains(GRAPH_TOKEN));
        let benutzer = p.admin("GET", "/admin/benutzer", None).await;
        assert!(!benutzer.text.contains(GRAPH_TOKEN));
    })
    .await;
}

/// Die beiden ändernden Verwaltungsrouten, die es nur mit Entra gibt, stehen
/// auch in der Prüfspur – wer, was, Ziel.
///
/// Ohne Entra antworten sie 404; deshalb steht der erfolgreiche Weg hier und
/// nicht in `tests/server.rs` (dort läuft die Tabelle
/// `quiver_server::spur::AENDERNDE_ROUTEN` durch).
#[tokio::test]
async fn nachpruefen_und_aufnehmen_schreiben_in_die_pruefspur() {
    mit_zeitlimit(async {
        let p = Pruefung::mit_entra().await;
        let _login = p.geraet_anmelden("A", 51234).await;

        let geprueft = p
            .admin("POST", &format!("/admin/benutzer/{OID}/pruefen"), None)
            .await;
        assert_eq!(geprueft.status, StatusCode::OK, "{}", geprueft.text);

        let weg = p
            .admin("DELETE", &format!("/admin/gruppen/{GRUPPE_ENTRA}"), None)
            .await;
        assert_eq!(weg.status, StatusCode::NO_CONTENT, "{}", weg.text);
        let aufgenommen = p
            .admin(
                "PUT",
                &format!("/admin/entra-gruppen/{GRUPPE_ENTRA}"),
                Some(json!({"aufnehmen": true})),
            )
            .await;
        assert_eq!(aufgenommen.status, StatusCode::OK, "{}", aufgenommen.text);

        let spur = p.admin("GET", "/admin/pruefspur?limit=200", None).await;
        assert_eq!(spur.status, StatusCode::OK);
        let eintraege: Vec<(String, String)> = spur
            .json
            .as_array()
            .expect("Liste")
            .iter()
            .filter(|eintrag| eintrag["art"] == "verwaltung")
            .map(|eintrag| {
                (
                    eintrag["text"].as_str().unwrap_or_default().to_string(),
                    eintrag["akteur"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        for (methode, pfad) in [
            ("POST", "/api/v1/admin/benutzer/{oid}/pruefen"),
            ("PUT", "/api/v1/admin/entra-gruppen/{id}"),
        ] {
            assert!(
                eintraege
                    .iter()
                    .any(|(text, _)| text.contains(methode) && text.contains(pfad)),
                "{methode} {pfad} fehlt: {eintraege:?}"
            );
        }
        assert!(
            eintraege.iter().all(|(_, akteur)| akteur == ROOT_EMAIL),
            "der Akteur steht dabei: {eintraege:?}"
        );
        // Die Verwaltungs-Einträge tragen keine Werte: nur Methode, Pfad, Aktion.
        for (text, _) in &eintraege {
            assert!(!text.contains(GRUPPE_ENTRA), "keine Id im Klartext: {text}");
            assert!(!text.contains("Test Person"), "kein Name: {text}");
        }
    })
    .await;
}
