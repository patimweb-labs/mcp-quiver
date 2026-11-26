// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: Anmeldung des Geräts am echten Server mit Entra-Nachbau
//! (Aufgabe 22).
//!
//! Kein echter Mandant, keine Firmendaten, keine Geheimnisse: der Nachbau
//! läuft auf Loopback mit eigenem RSA-Schlüssel, wie `crates/server/tests/
//! entra.rs`. Der Browser ist eine Attrappe, die den Weg selbst geht:
//! Anmeldeseite → Entra → Bestätigung → Rücksprung auf `127.0.0.1:<port>`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes};
use axum::extract::{Query, State as AxState};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use futures_util::StreamExt;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use quiver_client::{Anmeldewege, Browser, Kern, KernKonfig, SpeicherTresor, Tresor, Verbindung};
use quiver_domain::{
    Eintrag, EintragId, Feld, Gruppe, GruppeId, Herkunft, Laufzeit, Quelle, Satz, SatzEintrag,
    SatzId, Werte,
};
use quiver_protocol::{AnmeldeGrund, EintragZustand};
use quiver_server::{app, benutzer, EntraKonfig, Konfig, Uhr};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::PrivateKey;
use serde_json::{json, Value};
use sha2::Digest;

#[path = "gemeinsam.rs"]
mod gemeinsam;

const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const OID: &str = "99999999-8888-7777-6666-555555555555";
const GRUPPE_ENTRA: &str = "12345678-1234-1234-1234-123456789012";
const ENTRA_SECRET: &str = "kein-echtes-geheimnis-nur-test";
const FRIST: Duration = Duration::from_secs(20);

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
    claims: Value,
    refresh_fehler: Option<String>,
}

struct CodeInfo {
    challenge: String,
    nonce: String,
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

fn claims(basis: &str, jetzt: i64) -> Value {
    json!({
        "iss": basis,
        "aud": CLIENT,
        "tid": TENANT,
        "oid": OID,
        "exp": jetzt + 3600,
        "nbf": jetzt - 10,
        "iat": jetzt - 10,
        "preferred_username": "test@example.test",
        "name": "Test Person",
        "groups": [GRUPPE_ENTRA],
    })
}

impl Mock {
    async fn start(uhr: Uhr) -> Self {
        let mut rng = getrandom::SysRng;
        let key = Arc::new(PrivateKey::new(&mut rng, 2048).expect("RSA"));
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
        let mock = Mock {
            basis: basis.clone(),
            kid: "kid-1".into(),
            key,
            uhr: uhr.clone(),
            zustand: Arc::new(Mutex::new(MockZustand {
                codes: HashMap::new(),
                naechster_code: 0,
                claims: claims(&basis, uhr.jetzt().unix_timestamp()),
                refresh_fehler: None,
            })),
        };
        let router = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .route("/keys", get(keys))
            .with_state(mock.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        mock
    }

    fn refresh_abbrechen(&self) {
        self.zustand.lock().unwrap().refresh_fehler = Some("invalid_grant".into());
    }
}

async fn discovery(AxState(m): AxState<Mock>) -> Response {
    Json(json!({
        "issuer": m.basis,
        "authorization_endpoint": format!("{}/authorize", m.basis),
        "token_endpoint": format!("{}/token", m.basis),
        "jwks_uri": format!("{}/keys", m.basis),
    }))
    .into_response()
}

async fn keys(AxState(m): AxState<Mock>) -> Json<Value> {
    Json(jwks(&m.key, &m.kid))
}

async fn authorize(
    AxState(m): AxState<Mock>,
    Query(abfrage): Query<HashMap<String, String>>,
) -> Response {
    let mut z = m.zustand.lock().unwrap();
    z.naechster_code += 1;
    let code = format!("code-{}", z.naechster_code);
    z.codes.insert(
        code.clone(),
        CodeInfo {
            challenge: abfrage.get("code_challenge").cloned().unwrap_or_default(),
            nonce: abfrage.get("nonce").cloned().unwrap_or_default(),
        },
    );
    let state = abfrage.get("state").cloned().unwrap_or_default();
    let redirect = abfrage.get("redirect_uri").cloned().unwrap_or_default();
    Redirect::to(&format!("{redirect}?code={code}&state={state}")).into_response()
}

async fn token(AxState(m): AxState<Mock>, rumpf: Bytes) -> Response {
    let form: HashMap<String, String> = url::form_urlencoded::parse(&rumpf).into_owned().collect();
    let (refresh_fehler, mut info, jetzt, key) = {
        let z = m.zustand.lock().unwrap();
        (
            z.refresh_fehler.clone(),
            form.get("code")
                .and_then(|code| z.codes.get(code))
                .map(|i| (i.challenge.clone(), i.nonce.clone(), z.claims.clone())),
            m.uhr.jetzt().unix_timestamp(),
            m.key.clone(),
        )
    };
    if form.get("grant_type").map(String::as_str) == Some("refresh_token") {
        if let Some(fehler) = refresh_fehler {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": fehler }))).into_response();
        }
        let mut claims = m.zustand.lock().unwrap().claims.clone();
        claims["exp"] = json!(jetzt + 3600);
        return Json(json!({
            "id_token": sign(&key, &m.kid, &claims),
            "refresh_token": "refresh-2",
            "token_type": "Bearer",
            "expires_in": 3600,
        }))
        .into_response();
    }
    let Some((challenge, nonce, mut claims)) = info.take() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    };
    let verifier = form.get("code_verifier").cloned().unwrap_or_default();
    let erwartet = URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
    if erwartet != challenge {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        )
            .into_response();
    }
    claims["nonce"] = json!(nonce);
    claims["exp"] = json!(jetzt + 3600);
    Json(json!({
        "id_token": sign(&key, &m.kid, &claims),
        "refresh_token": "refresh-1",
        "token_type": "Bearer",
        "expires_in": 3600,
    }))
    .into_response()
}

// ------------------------------------------------------------ Browser-Attrappe

/// Ein Browser, der den ganzen Weg selbst geht (Anmelden → Bestätigen →
/// Rücksprung auf den Lauscher). Jede geöffnete Adresse wird gemerkt, damit
/// „Browser erneut öffnen“ prüfbar ist.
struct NachbauBrowser {
    http: reqwest::Client,
    server: String,
    gesehen: Arc<Mutex<Vec<String>>>,
    fehler: Arc<Mutex<Option<String>>>,
}

impl Browser for NachbauBrowser {
    fn oeffnen(&self, adresse: &str) -> anyhow::Result<()> {
        self.gesehen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(adresse.to_string());
        let http = self.http.clone();
        let server = self.server.clone();
        let adresse = adresse.to_string();
        let fehler = self.fehler.clone();
        std::thread::spawn(move || {
            let laufzeit = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Laufzeit");
            if let Err(problem) = laufzeit.block_on(weg(&http, &server, &adresse)) {
                *fehler.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("{problem:#}"));
            }
        });
        Ok(())
    }
}

async fn weg(http: &reqwest::Client, server: &str, adresse: &str) -> anyhow::Result<()> {
    let start = http.get(adresse).send().await?;
    anyhow::ensure!(
        start.status() == StatusCode::SEE_OTHER,
        "Start der Anmeldung: {}",
        start.status()
    );
    let oidc = cookie(&start, "quiver_oidc")?;
    let ziel = ort(&start)?;

    let entra = http.get(ziel).send().await?;
    anyhow::ensure!(
        entra.status() == StatusCode::SEE_OTHER,
        "Entra-Nachbau: {}",
        entra.status()
    );
    let rueck = ort(&entra)?;

    let seite = http.get(rueck).header(header::COOKIE, oidc).send().await?;
    anyhow::ensure!(
        seite.status() == StatusCode::OK,
        "Bestätigungsseite: {}",
        seite.status()
    );
    let bestaetigung = cookie(&seite, "quiver_bestaetigung")?;
    let html = seite.text().await?;
    let token = feld(&html, "token");
    let csrf = feld(&html, "csrf");

    let rumpf = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("token", &token)
        .append_pair("csrf", &csrf)
        .finish();
    let bestaetigt = http
        .post(format!("{server}/auth/geraet/bestaetigen"))
        .header(header::COOKIE, bestaetigung)
        .header(header::ORIGIN, server)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(rumpf)
        .send()
        .await?;
    anyhow::ensure!(
        bestaetigt.status() == StatusCode::SEE_OTHER,
        "Bestätigen: {}",
        bestaetigt.status()
    );
    let lauscher = ort(&bestaetigt)?;
    anyhow::ensure!(
        lauscher.starts_with("http://127.0.0.1:"),
        "Rücksprung muss auf Loopback gehen: {lauscher}"
    );
    let fertig = http.get(lauscher).send().await?;
    anyhow::ensure!(
        fertig.status() == StatusCode::OK,
        "Abschlussseite: {}",
        fertig.status()
    );
    Ok(())
}

fn cookie(antwort: &reqwest::Response, name: &str) -> anyhow::Result<String> {
    let wert = antwort
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|wert| wert.to_str().ok())
        .find_map(|wert| {
            let erstes = wert.split(';').next().unwrap_or_default();
            erstes
                .starts_with(&format!("{name}="))
                .then(|| erstes.to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("Cookie {name} fehlt"))?;
    Ok(wert)
}

fn ort(antwort: &reqwest::Response) -> anyhow::Result<String> {
    antwort
        .headers()
        .get(header::LOCATION)
        .and_then(|wert| wert.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("Location fehlt"))
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

// ------------------------------------------------------------------- Prüfung

struct Pruefung {
    ordner: PathBuf,
    basis: String,
    cookie: String,
    http: reqwest::Client,
    kern: Kern,
    tresor: Arc<SpeicherTresor>,
    browser: Arc<NachbauBrowser>,
    mock: Option<Mock>,
}

impl Pruefung {
    async fn mit_entra() -> Self {
        let uhr = Uhr::default();
        let mock = Mock::start(uhr.clone()).await;
        Self::bauen(Some((mock, uhr))).await
    }

    async fn ohne_entra() -> Self {
        Self::bauen(None).await
    }

    async fn bauen(mock: Option<(Mock, Uhr)>) -> Self {
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let stempel = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Zeit")
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "quiver-client-serveranmeldung-{}-{nummer}-{stempel}",
            std::process::id()
        ));
        let daten = ordner.join("server");
        std::fs::create_dir_all(&daten).expect("Ordner");
        benutzer::root_setzen_auf_platte(&daten, gemeinsam::ROOT_EMAIL, gemeinsam::ROOT_PASSWORT)
            .expect("Root");

        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
        let mut konfig = Konfig::neu(&daten);
        konfig.oeffentlich = basis.clone();
        let (mockteil, uhr) = match mock {
            Some((mock, uhr)) => {
                konfig.entra = Some(EntraKonfig {
                    tenant: TENANT.into(),
                    client_id: CLIENT.into(),
                    ausweis: quiver_server::Ausweis::Geheimnis(ENTRA_SECRET.into()),
                    authority: mock.basis.clone(),
                    issuer: mock.basis.clone(),
                    redirect_uri: format!("{basis}/auth/callback"),
                    gruppen_namen: false,
                    graph_basis: String::from(quiver_server::GRAPH_BASIS),
                });
                (Some(mock), uhr)
            }
            None => (None, Uhr::default()),
        };
        konfig.uhr = uhr;
        let router = app(konfig).await.expect("App baut");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("HTTP");
        let cookie = gemeinsam::sitzung(&http, &basis, &daten).await;

        let browser = Arc::new(NachbauBrowser {
            http: http.clone(),
            server: basis.clone(),
            gesehen: Arc::new(Mutex::new(Vec::new())),
            fehler: Arc::new(Mutex::new(None)),
        });
        let tresor = Arc::new(SpeicherTresor::neu());
        let kern = Kern::starten(KernKonfig {
            profil: ordner.join("profil"),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: tresor.clone(),
            browser: browser.clone(),
            vorgaben: quiver_client::Vorgaben::default(),
            aufstellung: quiver_client::update::Aufstellung::nicht_tauschen(),
        })
        .await
        .expect("Kern startet");

        Self {
            ordner,
            basis,
            cookie,
            http,
            kern,
            tresor,
            browser,
            mock: mockteil,
        }
    }

    async fn admin(&self, methode: &str, pfad: &str, rumpf: Option<Value>) -> Value {
        let mut anfrage = self
            .http
            .request(
                methode.parse().expect("Methode"),
                format!("{}/api/v1{pfad}", self.basis),
            )
            .header("cookie", &self.cookie)
            .header("origin", &self.basis)
            .header("x-quiver", "1")
            .header("content-type", "application/json");
        if let Some(rumpf) = rumpf {
            anfrage = anfrage.body(serde_json::to_vec(&rumpf).expect("JSON"));
        }
        let antwort = anfrage.send().await.expect("Server erreichbar");
        let status = antwort.status();
        let text = antwort.text().await.expect("Rumpf");
        assert!(status.is_success(), "Admin {pfad}: {status} {text}");
        if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).expect("Antwort ist JSON")
        }
    }

    async fn gruppe_anlegen(&self) {
        let eintrag = Eintrag {
            id: EintragId::neu("echo"),
            version: "1.0.0".into(),
            titel: "Echo".into(),
            beschreibung: "Werkzeug für den Test.".into(),
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: env!("CARGO_BIN_EXE_quiver-echo-mcp").to_string(),
                argumente: Vec::new(),
                umgebung: Werte::new(),
            },
            felder: vec![Feld {
                name: "PFAD".into(),
                beschreibung: String::new(),
                quelle: Quelle::Server,
                pflicht: false,
                standard: None,
                link: None,
                ..Default::default()
            }],
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
            mehrfach: None,
        };
        self.admin("PUT", "/admin/eintraege/echo", Some(json!(eintrag)))
            .await;
        self.admin(
            "PUT",
            "/admin/saetze/basis",
            Some(json!(Satz {
                id: SatzId::neu("basis"),
                titel: "Basis".into(),
                prioritaet: 0,
                eintraege: vec![SatzEintrag {
                    eintrag: EintragId::neu("echo"),
                    werte: Werte::new(),
                }],
            })),
        )
        .await;
        self.admin(
            "PUT",
            "/admin/gruppen/team",
            Some(json!(Gruppe {
                id: GruppeId::neu("team"),
                titel: "Team".into(),
                saetze: vec![SatzId::neu("basis")],
                entra: vec![GRUPPE_ENTRA.to_string()],
            })),
        )
        .await;
    }

    async fn anmeldewege(&self) -> Anmeldewege {
        self.kern
            .anmeldewege(&self.basis)
            .await
            .expect("Anmeldewege");
        self.kern
            .modell()
            .borrow()
            .anmeldewege
            .clone()
            .expect("Wege im Modell")
    }

    async fn anmelden(&self) {
        let wegen = self.anmeldewege().await;
        assert!(wegen.entra, "der Server meldet Entra: {wegen:?}");
        tokio::time::timeout(FRIST, self.kern.anmelden_server(&self.basis, "Testrechner"))
            .await
            .expect("Anmeldung endet (Browser-Attrappe)")
            .expect("Anmeldung klappt");
        if let Some(fehler) = self
            .browser
            .fehler
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            panic!("Browser-Attrappe: {fehler}");
        }
    }

    async fn warte_auf<F>(&self, was: &str, bedingung: F) -> quiver_client::Modell
    where
        F: Fn(&quiver_client::Modell) -> bool,
    {
        let frist = Instant::now() + FRIST;
        let mut modell = self.kern.modell();
        loop {
            {
                let stand = modell.borrow_and_update().clone();
                if bedingung(&stand) {
                    return stand;
                }
                let rest = frist.saturating_duration_since(Instant::now());
                assert!(
                    !rest.is_zero(),
                    "{was} bleibt aus: verbindung={:?}, benutzer={:?}, eintraege={:?}",
                    stand.verbindung,
                    stand.benutzer,
                    stand.eintraege
                );
                let _ = tokio::time::timeout(rest, modell.changed()).await;
            }
        }
    }

    async fn geraete(&self) -> Vec<Value> {
        self.admin("GET", "/admin/geraete", None)
            .await
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

// ------------------------------------------------------------------- Tests

/// Ohne Entra: die Seite bietet den Code-Weg, und der Code bindet ein.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ohne_entra_bietet_der_server_den_code_weg() {
    let p = Pruefung::ohne_entra().await;
    p.gruppe_anlegen().await;
    let wegen = p.anmeldewege().await;
    assert!(!wegen.entra, "kein Entra eingerichtet: {wegen:?}");
    assert!(wegen.code, "ohne Entra gibt es Einmal-Codes");

    let code = p
        .admin(
            "POST",
            "/admin/codes",
            Some(json!({ "gruppen": ["team"], "minuten": 60 })),
        )
        .await;
    p.kern
        .einbinden(
            &p.basis,
            code["code"].as_str().expect("Code"),
            "Testrechner",
        )
        .await
        .expect("Einbinden klappt");
    let stand = p
        .warte_auf("verbunden", |m| {
            matches!(m.verbindung, Verbindung::Verbunden { .. })
        })
        .await;
    assert!(
        stand.benutzer.is_none(),
        "der Code-Weg kennt keinen Benutzer"
    );
    p.kern.beenden().await;
}

/// Der ganze Entra-Weg über die Browser-Attrappe: anmelden, „Angemeldet als“,
/// und das Gerät steht am Server.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entra_anmeldung_ueber_browser_trait() {
    let p = Pruefung::mit_entra().await;
    p.gruppe_anlegen().await;
    p.anmelden().await;
    let stand = p
        .warte_auf("verbunden", |m| {
            matches!(m.verbindung, Verbindung::Verbunden { .. })
        })
        .await;
    match &stand.verbindung {
        Verbindung::Verbunden { server, geraet } => {
            assert_eq!(server, &p.basis);
            assert_eq!(geraet, "Testrechner");
        }
        andere => panic!("nicht verbunden: {andere:?}"),
    }
    assert_eq!(
        stand.benutzer.as_deref(),
        Some("test@example.test"),
        "„Angemeldet als …“ kommt aus der Anmeldeantwort"
    );
    assert!(
        p.tresor.lesen("geraet-token").expect("Tresor").is_some(),
        "das Token liegt im Tresor"
    );
    let geraete = p.geraete().await;
    assert_eq!(geraete.len(), 1, "das Gerät steht am Server: {geraete:?}");
    assert_eq!(geraete[0]["geraet"]["name"], "Testrechner");
    assert!(
        !p.browser
            .gesehen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty(),
        "der Browser wurde geöffnet"
    );
    p.kern.beenden().await;
}

/// Ein zweites Anmelden führt dasselbe Gerät weiter (`altes_token`), statt ein
/// zweites anzulegen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn altes_token_haelt_dasselbe_geraet() {
    let p = Pruefung::mit_entra().await;
    p.gruppe_anlegen().await;
    p.anmelden().await;
    p.warte_auf("verbunden", |m| {
        matches!(m.verbindung, Verbindung::Verbunden { .. })
    })
    .await;
    let vorher = p.geraete().await;
    assert_eq!(vorher.len(), 1);
    let id = vorher[0]["geraet"]["id"].clone();

    p.anmelden().await;
    p.warte_auf("wieder verbunden", |m| {
        matches!(m.verbindung, Verbindung::Verbunden { .. })
    })
    .await;
    let nachher = p.geraete().await;
    assert_eq!(nachher.len(), 1, "kein zweites Gerät: {nachher:?}");
    assert_eq!(nachher[0]["geraet"]["id"], id, "dasselbe Gerät");
    p.kern.beenden().await;
}

/// Abmelden löscht das Gerät am Server; die Adresse bleibt vorbelegt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abmelden_loescht_das_geraet_am_server_und_behaelt_die_adresse() {
    let p = Pruefung::mit_entra().await;
    p.gruppe_anlegen().await;
    p.anmelden().await;
    p.warte_auf("verbunden", |m| {
        matches!(m.verbindung, Verbindung::Verbunden { .. })
    })
    .await;
    assert_eq!(p.geraete().await.len(), 1);

    p.kern.abmelden().await.expect("Abmelden klappt");
    assert!(
        p.geraete().await.is_empty(),
        "das Gerät ist am Server gelöscht"
    );
    let stand = p.kern.modell().borrow().clone();
    assert_eq!(stand.verbindung, Verbindung::NichtEingebunden);
    assert_eq!(
        stand.server_url.as_deref(),
        Some(p.basis.as_str()),
        "die Adresse bleibt"
    );
    assert!(stand.benutzer.is_none());
    assert!(p.tresor.lesen("geraet-token").expect("Tresor").is_none());
    p.kern.beenden().await;
}

/// Ist der Server beim Abmelden nicht erreichbar, meldet der Client lokal
/// trotzdem ab und **sagt**, dass das Gerät am Server stehen blieb.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abmelden_ohne_server_sagt_es_und_meldet_lokal_ab() {
    let p = Pruefung::mit_entra().await;
    p.gruppe_anlegen().await;
    p.anmelden().await;
    p.warte_auf("verbunden", |m| {
        matches!(m.verbindung, Verbindung::Verbunden { .. })
    })
    .await;
    p.kern.beenden().await;

    let profil = p.ordner.join("profil");
    let zustand = profil.join("zustand.json");
    let text = std::fs::read_to_string(&zustand).expect("zustand.json");
    std::fs::write(&zustand, text.replace(&p.basis, "http://127.0.0.1:9")).expect("zustand.json");
    let zweiter = Kern::starten(KernKonfig {
        profil,
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: p.tresor.clone(),
        browser: p.browser.clone(),
        vorgaben: quiver_client::Vorgaben::default(),
        aufstellung: quiver_client::update::Aufstellung::nicht_tauschen(),
    })
    .await
    .expect("Kern startet");
    let fehler = zweiter
        .abmelden()
        .await
        .expect_err("der Server ist nicht erreichbar");
    assert!(
        fehler.to_string().contains("nicht löschen"),
        "der Client sagt, dass das Gerät am Server blieb: {fehler}"
    );
    assert!(
        p.tresor.lesen("geraet-token").expect("Tresor").is_none(),
        "lokal ist das Token weg"
    );
    assert!(matches!(
        zweiter.modell().borrow().verbindung,
        Verbindung::NichtEingebunden | Verbindung::Getrennt { .. }
    ));
    zweiter.beenden().await;
}

/// `gesperrt` und `kein-zugriff`: alle Tools aus, klare Meldung, Token weg.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gesperrt_und_kein_zugriff_stoppen_alle_tools() {
    let p = Pruefung::mit_entra().await;
    p.gruppe_anlegen().await;
    p.anmelden().await;
    p.warte_auf("verbunden", |m| {
        matches!(m.verbindung, Verbindung::Verbunden { .. })
    })
    .await;
    p.warte_auf("Werkzeug läuft", |m| {
        m.eintraege
            .iter()
            .any(|e| matches!(e.zustand, EintragZustand::Laeuft { .. }))
    })
    .await;

    // Gruppe wegnehmen → kein Zugriff.
    let ohne = quiver_domain::Gruppe {
        id: GruppeId::neu("team"),
        titel: "Team".into(),
        saetze: vec![SatzId::neu("basis")],
        entra: Vec::new(),
    };
    p.admin("PUT", "/admin/gruppen/team", Some(json!(ohne)))
        .await;
    let stand = p
        .warte_auf("kein Zugriff", |m| {
            m.abgelehnt && m.anmelde_grund == Some(AnmeldeGrund::KeinZugriff)
        })
        .await;
    assert!(stand.eintraege.is_empty(), "alle Tools sind aus");
    assert!(stand.abgelehnt);
    assert!(p.tresor.lesen("geraet-token").expect("Tresor").is_none());

    // Wieder anmelden und dann sperren.
    p.gruppe_anlegen().await;
    p.anmelden().await;
    p.warte_auf("wieder verbunden", |m| {
        matches!(m.verbindung, Verbindung::Verbunden { .. })
    })
    .await;
    // Erst wenn das Werkzeug läuft, steht auch der Ereignis-Strom; sonst
    // könnte die Sperre als Rundruf verpuffen, bevor jemand zuhört.
    p.warte_auf("Werkzeug läuft wieder", |m| {
        m.eintraege
            .iter()
            .any(|e| matches!(e.zustand, EintragZustand::Laeuft { .. }))
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let liste = p.admin("GET", "/admin/benutzer", None).await;
    let oid = liste
        .as_array()
        .and_then(|liste| liste.iter().find(|b| b["oid"] == OID))
        .map(|b| b["oid"].as_str().unwrap_or_default().to_string())
        .expect("Benutzer");
    p.admin(
        "PUT",
        &format!("/admin/benutzer/{oid}"),
        Some(json!({ "gesperrt": true })),
    )
    .await;
    let stand = p
        .warte_auf("gesperrt", |m| {
            m.abgelehnt && m.anmelde_grund == Some(AnmeldeGrund::Gesperrt)
        })
        .await;
    assert!(stand.eintraege.is_empty(), "alle Tools sind aus");
    p.kern.beenden().await;
}

/// Ein stummer Server für den einen Fall, der nicht vom Ereignis abhängen
/// darf: der Strom ist offen, das Ereignis des Entzugs fiel aber zwischen das
/// letzte Soll und das Abonnement — es hat niemand gehört, und die Revision
/// bleibt gleich, weil der Server sie aus dem gespeicherten Gerätestand rechnet.
///
/// Antworten: das erste `soll` liefert den erwarteten (leeren) Stand, jedes
/// weitere `403 kein-zugriff`; der Strom schickt genau ein Ereignis mit der
/// Revision dieses Standes und dann nichts mehr; `meldung` nimmt an.
async fn stummer_server(erste_revision: String) -> String {
    let leer = quiver_domain::Soll::neu(Vec::new());
    let abrufe = Arc::new(AtomicUsize::new(0));
    let router = Router::new()
        .route(
            "/api/v1/geraet/soll",
            get({
                let abrufe = abrufe.clone();
                move || {
                    let abrufe = abrufe.clone();
                    let leer = leer.clone();
                    async move {
                        // Nur der erste Abruf gelingt: danach ist der Zugang
                        // entzogen, ohne dass sich die Revision ändert.
                        if abrufe.fetch_add(1, Ordering::SeqCst) == 0 {
                            Json(leer).into_response()
                        } else {
                            (
                                StatusCode::FORBIDDEN,
                                Json(json!({
                                    "fehler": "dem Benutzer ist keine Gruppe zugeordnet",
                                    "grund": "kein-zugriff",
                                })),
                            )
                                .into_response()
                        }
                    }
                }
            }),
        )
        .route(
            "/api/v1/geraet/ereignisse",
            get({
                let revision = erste_revision.clone();
                move || {
                    let revision = revision.clone();
                    async move {
                        let anfang = futures_util::stream::once(async move {
                            Ok::<_, std::io::Error>(format!("event: soll\ndata: {revision}\n\n"))
                        });
                        // Danach offen und still: kein Ereignis, kein Byte.
                        let koerper = anfang.chain(futures_util::stream::pending()).boxed();
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "text/event-stream")
                            .body(Body::from_stream(koerper))
                            .expect("Antwort")
                    }
                }
            }),
        )
        .route(
            "/api/v1/geraet/meldung",
            post(|| async { StatusCode::NO_CONTENT }),
        );
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("Port frei");
    let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    basis
}

/// Der Entzug hängt nicht am Ereignis: der Client fragt direkt nach dem Öffnen
/// des Stroms nach.
///
/// Das ist der Fall, der in der CI einmal auffiel: der Admin nimmt die Gruppe
/// weg, während der Client gerade zwischen „Soll holen“ und „Strom öffnen“
/// steckt. Das Ereignis geht an ein Abonnement, das es noch nicht gibt, und die
/// Revision bleibt gleich – ohne Nachfrage bliebe der Client „verbunden“ und
/// die Werkzeuge liefen weiter, bis die nächste Nachfrage fällig ist (fünf
/// Minuten). Hier kommt nie ein Ereignis: der Entzug muss trotzdem sofort
/// wirken.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entzug_ohne_ereignis_wirkt_beim_oeffnen_des_stroms() {
    let p = Pruefung::mit_entra().await;
    p.gruppe_anlegen().await;
    p.anmelden().await;
    p.warte_auf("verbunden", |m| {
        matches!(m.verbindung, Verbindung::Verbunden { .. })
    })
    .await;
    p.kern.beenden().await;

    // Der stumme Server gibt den Stand bekannt, den der Client gleich hat.
    let leer = quiver_domain::Soll::neu(Vec::new());
    let stub = stummer_server(leer.revision.clone()).await;

    // Dasselbe Profil (Geräte-Token, Angaben), nur eine andere Adresse:
    // Loopback, wie im Betrieb erlaubt.
    let profil = p.ordner.join("profil");
    let zustand = profil.join("zustand.json");
    let text = std::fs::read_to_string(&zustand).expect("zustand.json");
    std::fs::write(&zustand, text.replace(&p.basis, &stub)).expect("zustand.json");
    let client = Kern::starten(KernKonfig {
        profil,
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: p.tresor.clone(),
        browser: p.browser.clone(),
        vorgaben: quiver_client::Vorgaben::default(),
        aufstellung: quiver_client::update::Aufstellung::nicht_tauschen(),
    })
    .await
    .expect("Kern startet");

    let mut modell = client.modell();
    let stand = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let stand = modell.borrow_and_update().clone();
            if stand.abgelehnt && stand.anmelde_grund == Some(AnmeldeGrund::KeinZugriff) {
                return stand;
            }
            modell.changed().await.expect("das Modell lebt");
        }
    })
    .await
    .expect("der Entzug wirkt ohne Ereignis (kein Warten auf die Nachfrage)");
    assert!(stand.eintraege.is_empty(), "alle Tools sind aus");
    assert!(
        !matches!(stand.verbindung, Verbindung::Verbunden { .. }),
        "nicht mehr „verbunden“: {stand:?}"
    );
    client.beenden().await;
}

/// `neu-anmelden`: alle Tools aus, aber Geräte-Token und Angaben bleiben.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn neu_anmelden_behaelt_token_und_angaben() {
    let p = Pruefung::mit_entra().await;
    p.gruppe_anlegen().await;
    p.anmelden().await;
    p.warte_auf("Werkzeug läuft", |m| {
        m.eintraege
            .iter()
            .any(|e| matches!(e.zustand, EintragZustand::Laeuft { .. }))
    })
    .await;
    let token_vorher = p.tresor.lesen("geraet-token").expect("Tresor");

    // Der Nachbau liefert beim Nachprüfen `invalid_grant`.
    p.mock.as_ref().expect("Mock").refresh_abbrechen();
    let liste = p.admin("GET", "/admin/benutzer", None).await;
    let oid = liste
        .as_array()
        .and_then(|liste| liste.iter().find(|b| b["oid"] == OID))
        .map(|b| b["oid"].as_str().unwrap_or_default().to_string())
        .expect("Benutzer");
    let geprueft = p
        .http
        .post(format!("{}/api/v1/admin/benutzer/{oid}/pruefen", p.basis))
        .header("cookie", &p.cookie)
        .header("origin", &p.basis)
        .header("x-quiver", "1")
        .send()
        .await
        .expect("erreichbar");
    assert!(!geprueft.status().is_success(), "Prüfen meldet den Grund");

    let stand = p
        .warte_auf("neu anmelden", |m| {
            matches!(m.verbindung, Verbindung::NeuAnmelden { .. })
        })
        .await;
    assert_eq!(
        p.tresor.lesen("geraet-token").expect("Tresor"),
        token_vorher,
        "das Geräte-Token bleibt"
    );
    assert!(stand.benutzer.is_some(), "die Angaben bleiben");
    assert!(
        stand
            .eintraege
            .iter()
            .all(|e| !matches!(e.zustand, EintragZustand::Laeuft { .. })),
        "alle Tools sind aus: {:?}",
        stand.eintraege
    );
    assert!(
        !stand.eintraege.is_empty(),
        "die Einträge bleiben sichtbar (kein Ausbinden)"
    );
    p.kern.beenden().await;
}
