// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Server holt das Update-Paket und liefert es dem Gerät (Aufgabe 39).
//!
//! **Kein echtes Netz:** die „Veröffentlichung“ ist eine Attrappe auf Loopback
//! (dieselbe Rolle, die GitHub im Betrieb hat), die Paket, `SHA256SUMS` und die
//! Antwort der Veröffentlichung ausliefert. Geprüft wird: der Server holt,
//! prüft, verweigert eine falsche Prüfsumme, legt das Paket unter
//! `<daten>/updates/<version>/` ab und liefert es nur an ein angemeldetes Gerät
//! und nur für dessen System.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::Path as Pfad;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::routing::get;
use axum::Router;
use http_body_util::BodyExt;
use quiver_protocol::{Anmelden, AnmeldenAntwort, Code, CodeAnlegen, UpdateAngebot};
use quiver_server::benutzer;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower::ServiceExt;

const ROOT_EMAIL: &str = "root@example.test";
const ROOT_PASSWORT: &str = "ein-sehr-langes-testpasswort";
const HOST: &str = "127.0.0.1:8740";

/// Das Linux-Paket der „Veröffentlichung“.
const LINUX_PAKET: &[u8] = b"#!bin/sh\necho quiver\n";
/// Das macOS-Paket.
const MACOS_PAKET: &[u8] = b"dmg-attrappe";

fn sha256(inhalt: &[u8]) -> String {
    hex::encode(<sha2::Sha256 as sha2::Digest>::digest(inhalt))
}

/// Eine Attrappe der Veröffentlichung: Antwort, `SHA256SUMS` und die Pakete.
///
/// `falsche_pruefsumme` liefert absichtlich eine, die nicht zum Paket passt.
struct Veroeffentlichung {
    adresse: String,
    /// Wird beim Beenden der Attrappe gesetzt: dann antwortet sie nicht mehr.
    aus: Arc<AtomicUsize>,
}

impl Veroeffentlichung {
    async fn starten(falsche_pruefsumme: bool) -> Self {
        let linux = String::from("quiver-0.5.0-linux-x86_64.tar.gz");
        let macos = String::from("quiver-0.5.0-macos-arm64.dmg");
        let sums = Arc::new(Mutex::new(String::new()));
        let pfade = Arc::new(Mutex::new(
            std::collections::BTreeMap::<String, Vec<u8>>::new(),
        ));
        let aus = Arc::new(AtomicUsize::new(0));

        // Erst der Port, dann die Antworten: die Attrappe nennt ihre eigene
        // Adresse in den Download-Verweisen (wie GitHub).
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let adresse = format!("http://{}", listener.local_addr().expect("Adresse"));

        let router = {
            let sums = sums.clone();
            let pfade = pfade.clone();
            let aus = aus.clone();
            let basis = adresse.clone();
            let paket_json = {
                let linux = linux.clone();
                let macos = macos.clone();
                move |basis: &str| {
                    json!([
                        {
                            "name": linux,
                            "browser_download_url": format!("{basis}/download/{linux}"),
                            "size": LINUX_PAKET.len(),
                        },
                        {
                            "name": macos,
                            "browser_download_url": format!("{basis}/download/{macos}"),
                            "size": MACOS_PAKET.len(),
                        },
                        {
                            "name": "SHA256SUMS",
                            "browser_download_url": format!("{basis}/download/SHA256SUMS"),
                            "size": 128,
                        }
                    ])
                }
            };
            Router::new()
                .route(
                    "/releases/latest",
                    get({
                        let aus = aus.clone();
                        let basis = basis.clone();
                        move || {
                            let aus = aus.clone();
                            let basis = basis.clone();
                            let paket_json = paket_json.clone();
                            async move {
                                if aus.load(Ordering::SeqCst) > 0 {
                                    return (StatusCode::SERVICE_UNAVAILABLE, String::new());
                                }
                                let assets = paket_json(&basis);
                                let json = json!({
                                    "tag_name": "v0.5.0",
                                    "html_url": "https://example.test/releases/tag/v0.5.0",
                                    "published_at": "2026-11-25T10:00:00Z",
                                    "draft": false,
                                    "prerelease": false,
                                    "assets": assets,
                                });
                                (StatusCode::OK, serde_json::to_string(&json).expect("JSON"))
                            }
                        }
                    }),
                )
                .route(
                    "/download/{name}",
                    get({
                        let aus = aus.clone();
                        move |Pfad(name): Pfad<String>| {
                            let aus = aus.clone();
                            let pfade = pfade.clone();
                            let sums = sums.clone();
                            async move {
                                if aus.load(Ordering::SeqCst) > 0 {
                                    return (StatusCode::SERVICE_UNAVAILABLE, Vec::new());
                                }
                                if name == "SHA256SUMS" {
                                    let text = sums.lock().await.clone();
                                    return (StatusCode::OK, text.into_bytes());
                                }
                                match pfade.lock().await.get(&name).cloned() {
                                    Some(bytes) => (StatusCode::OK, bytes),
                                    None => (StatusCode::NOT_FOUND, Vec::new()),
                                }
                            }
                        }
                    }),
                )
        };

        {
            let mut karte = pfade.lock().await;
            karte.insert(linux.clone(), LINUX_PAKET.to_vec());
            karte.insert(macos.clone(), MACOS_PAKET.to_vec());
        }
        let linux_summe = if falsche_pruefsumme {
            sha256(b"etwas anderes")
        } else {
            sha256(LINUX_PAKET)
        };
        *sums.lock().await = format!(
            "{}  {}\n{}  {}\n",
            linux_summe,
            linux,
            sha256(MACOS_PAKET),
            macos
        );
        let dienen = axum::serve(listener, router);
        tokio::spawn(async move {
            let _ = dienen.await;
        });
        Self { adresse, aus }
    }

    /// Die Attrappe verstummt (für den Nachweis, dass aus der Ablage geliefert
    /// wird).
    fn verstummen(&self) {
        self.aus.store(1, Ordering::SeqCst);
    }
}

/// Der Server unter Prüfung.
struct Pruefung {
    ordner: PathBuf,
    router: Router,
    cookie: String,
    _attrappe: Veroeffentlichung,
}

impl Pruefung {
    async fn neu(falsche_pruefsumme: bool) -> Self {
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::Relaxed);
        let ordner = std::env::temp_dir().join(format!(
            "quiver-update-test-{}-{nummer}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).expect("Ordner");
        benutzer::root_setzen_auf_platte(&ordner, ROOT_EMAIL, ROOT_PASSWORT).expect("Root setzen");
        let attrappe = Veroeffentlichung::starten(falsche_pruefsumme).await;
        let mut konfig = quiver_server::Konfig::neu(&ordner);
        konfig.updates.an = true;
        // `api_adresse` hängt `/releases/latest` an – die Attrappe bedient
        // genau diesen Weg.
        konfig.updates.quelle = attrappe.adresse.clone();
        let router = quiver_server::app(konfig).await.expect("App baut");
        let mut p = Self {
            ordner,
            router,
            cookie: String::new(),
            _attrappe: attrappe,
        };
        p.cookie = p.root_anmelden().await;
        p
    }

    async fn root_anmelden(&self) -> String {
        let antwort = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/root")
                    .header("host", HOST)
                    .header("content-type", "application/json")
                    .header("origin", "http://127.0.0.1:8740")
                    .header("x-quiver", "1")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "email": ROOT_EMAIL,
                            "passwort": ROOT_PASSWORT
                        }))
                        .expect("JSON"),
                    ))
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        assert_eq!(antwort.status(), StatusCode::OK);
        antwort
            .headers()
            .get(header::SET_COOKIE)
            .expect("Cookie")
            .to_str()
            .expect("Text")
            .split(';')
            .next()
            .expect("Name=Wert")
            .to_string()
    }

    async fn admin(&self, methode: &str, pfad: &str, rumpf: Option<Value>) -> (StatusCode, Value) {
        let koerper = match rumpf {
            Some(wert) => Body::from(serde_json::to_vec(&wert).expect("JSON")),
            None => Body::empty(),
        };
        let antwort = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method(methode)
                    .uri(format!("/api/v1{pfad}"))
                    .header("host", HOST)
                    .header("content-type", "application/json")
                    .header("cookie", &self.cookie)
                    .header("origin", "http://127.0.0.1:8740")
                    .header("x-quiver", "1")
                    .body(koerper)
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }

    /// Ein Gerät einbinden.
    async fn geraet(&self, name: &str) -> AnmeldenAntwort {
        let (status, rumpf) = self
            .admin(
                "POST",
                "/admin/codes",
                Some(json!(CodeAnlegen {
                    gruppen: Vec::new(),
                    minuten: Some(60),
                    unbegrenzt: false,
                })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{rumpf}");
        let code: Code = serde_json::from_value(rumpf).expect("Code");
        let (status, rumpf) = self
            .roh(
                "POST",
                "/api/v1/anmelden",
                None,
                Some(json!(Anmelden {
                    code: code.code,
                    name: name.to_string(),
                })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{rumpf}");
        serde_json::from_value(rumpf).expect("AnmeldenAntwort")
    }

    async fn roh(
        &self,
        methode: &str,
        pfad: &str,
        token: Option<&str>,
        rumpf: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, _, bytes) = self.roh_bytes(methode, pfad, token, None, rumpf).await;
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    async fn roh_bytes(
        &self,
        methode: &str,
        pfad: &str,
        token: Option<&str>,
        extra: Option<(&str, &str)>,
        rumpf: Option<Value>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut bauer = Request::builder()
            .method(methode)
            .uri(pfad)
            .header("host", HOST);
        if let Some(token) = token {
            bauer = bauer.header("authorization", format!("Bearer {token}"));
        }
        if let Some((name, wert)) = extra {
            bauer = bauer.header(name, wert);
        }
        let koerper = match rumpf {
            Some(wert) => {
                bauer = bauer.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&wert).expect("JSON"))
            }
            None => Body::empty(),
        };
        let antwort = self
            .router
            .clone()
            .oneshot(bauer.body(koerper).expect("Anfrage"))
            .await
            .expect("Antwort");
        let status = antwort.status();
        let kopf = antwort.headers().clone();
        let bytes = antwort
            .into_body()
            .collect()
            .await
            .expect("Rumpf")
            .to_bytes()
            .to_vec();
        (status, kopf, bytes)
    }

    /// Das Linux-Paket, so wie die Attrappe es nennt.
    fn linux_paket(&self) -> &'static str {
        "quiver-0.5.0-linux-x86_64.tar.gz"
    }
}

async fn auspacken(antwort: axum::response::Response) -> (StatusCode, Value) {
    let status = antwort.status();
    let bytes = antwort
        .into_body()
        .collect()
        .await
        .expect("Rumpf")
        .to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn das_geraet_erfaehrt_die_pakete_seines_systems() {
    let p = Pruefung::neu(false).await;
    let geraet = p.geraet("Testrechner").await;
    let (status, rumpf) = p
        .roh(
            "GET",
            "/api/v1/geraet/updates?version=0.4.1&system=linux&arch=x64",
            Some(&geraet.token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let angebot: UpdateAngebot = serde_json::from_value(rumpf).expect("Angebot");
    assert_eq!(angebot.aktuelle_version, "0.4.1");
    assert_eq!(angebot.neueste_version.as_deref(), Some("0.5.0"));
    // Nur das Paket für Linux/x64 – nicht das für macOS.
    assert_eq!(angebot.pakete.len(), 1, "{:?}", angebot.pakete);
    let paket = &angebot.pakete[0];
    assert_eq!(paket.name, p.linux_paket());
    assert_eq!(paket.art, "tar.gz");
    assert_eq!(paket.groesse, LINUX_PAKET.len() as u64);
    assert_eq!(paket.sha256, sha256(LINUX_PAKET));

    // Ohne System: kein Paket, nur die Anzeige.
    let (status, rumpf) = p
        .roh(
            "GET",
            "/api/v1/geraet/updates?version=0.4.1",
            Some(&geraet.token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let ohne: UpdateAngebot = serde_json::from_value(rumpf).expect("Angebot");
    assert!(ohne.pakete.is_empty());

    // Ein unbekanntes System ist ein Fehler, kein Rateversuch.
    let (status, _) = p
        .roh(
            "GET",
            "/api/v1/geraet/updates?system=plan9&arch=x64",
            Some(&geraet.token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn das_paket_kommt_geprueft_und_aus_der_ablage() {
    let p = Pruefung::neu(false).await;
    let geraet = p.geraet("Testrechner").await;
    let pfad = format!(
        "/api/v1/geraet/updates/paket/{}?system=linux&arch=x64",
        p.linux_paket()
    );

    // Ohne Token: nichts.
    let (status, _, _) = p.roh_bytes("GET", &pfad, None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Mit Token: die Bytes, die Größe, ein ETag.
    let (status, kopf, bytes) = p
        .roh_bytes("GET", &pfad, Some(&geraet.token), None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, LINUX_PAKET);
    assert_eq!(
        kopf.get(header::CONTENT_LENGTH)
            .and_then(|wert| wert.to_str().ok()),
        Some(LINUX_PAKET.len().to_string().as_str())
    );
    let etag = kopf
        .get(header::ETAG)
        .and_then(|wert| wert.to_str().ok())
        .expect("ETag")
        .to_string();
    assert!(etag.contains(&sha256(LINUX_PAKET)), "{etag}");

    // Das Paket liegt jetzt in `<daten>/updates/<version>/`.
    let abgelegt = p.ordner.join("updates").join("0.5.0").join(p.linux_paket());
    assert!(abgelegt.is_file(), "{} fehlt", abgelegt.display());
    assert_eq!(std::fs::read(&abgelegt).expect("Datei"), LINUX_PAKET);
    assert!(p
        .ordner
        .join("updates")
        .join("0.5.0")
        .join("SHA256SUMS")
        .is_file());

    // Dasselbe noch einmal: 304, kein Rumpf.
    let (status, _, bytes) = p
        .roh_bytes(
            "GET",
            &pfad,
            Some(&geraet.token),
            Some(("if-none-match", &etag)),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(bytes.is_empty());

    // Ein Paket für ein anderes System gibt es hier nicht – auch nicht mit
    // einem erfundenen Namen.
    for name in [
        "quiver-0.5.0-macos-arm64.dmg",
        "../../etc/passwd",
        "quiver-0.5.0-linux-x86_64.zip",
    ] {
        let (status, _, _) = p
            .roh_bytes(
                "GET",
                &format!("/api/v1/geraet/updates/paket/{name}?system=linux&arch=x64"),
                Some(&geraet.token),
                None,
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name}");
    }
}

#[tokio::test]
async fn aus_der_ablage_wird_auch_ohne_veroeffentlichung_geliefert() {
    let p = Pruefung::neu(false).await;
    let geraet = p.geraet("Testrechner").await;
    let pfad = format!(
        "/api/v1/geraet/updates/paket/{}?system=linux&arch=x64",
        p.linux_paket()
    );
    // Einmal holen: jetzt liegt es in der Ablage.
    let (status, _, _) = p
        .roh_bytes("GET", &pfad, Some(&geraet.token), None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    // Die Veröffentlichung verstummt (kein Netz mehr).
    p._attrappe.verstummen();
    // Der Server hat den Stand noch im Speicher – und das Paket in der Ablage.
    let (status, _, bytes) = p
        .roh_bytes("GET", &pfad, Some(&geraet.token), None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, LINUX_PAKET);
}

#[tokio::test]
async fn eine_falsche_pruefsumme_wird_nicht_ausgeliefert() {
    let p = Pruefung::neu(true).await;
    let geraet = p.geraet("Testrechner").await;
    let (status, _, _) = p
        .roh_bytes(
            "GET",
            &format!(
                "/api/v1/geraet/updates/paket/{}?system=linux&arch=x64",
                p.linux_paket()
            ),
            Some(&geraet.token),
            None,
            None,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "was nicht passt, wird nicht ausgeliefert"
    );
    // Und es liegt nichts unter dem richtigen Namen.
    let ziel = p.ordner.join("updates").join("0.5.0").join(p.linux_paket());
    assert!(!ziel.exists(), "{} liegt trotzdem da", ziel.display());
}

#[tokio::test]
async fn ein_geloeschtes_geraet_bekommt_nichts_mehr() {
    let p = Pruefung::neu(false).await;
    let geraet = p.geraet("Testrechner").await;
    let pfad = format!(
        "/api/v1/geraet/updates/paket/{}?system=linux&arch=x64",
        p.linux_paket()
    );
    // Das Gerät meldet sich ab (der Server löscht es).
    let (status, _) = p
        .roh("POST", "/api/v1/geraet/abmelden", Some(&geraet.token), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = p
        .roh_bytes("GET", &pfad, Some(&geraet.token), None, None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
