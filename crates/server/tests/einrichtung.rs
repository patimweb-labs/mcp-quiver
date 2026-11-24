// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahmetests für Aufgabe 25 (Ersteinrichtung im Browser) und die Punkte 1,
//! 4, 7 und 9 aus Aufgabe 26, die den Server betreffen.
//!
//! Gegen [`quiver_server::app`] mit `tower::ServiceExt::oneshot`, einem
//! temporären Datenordner und Port 0 – nie gegen einen laufenden Server.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use quiver_domain::{Eintrag, EintragId, Herkunft, Laufzeit, Werte};
use quiver_protocol::{AuthStand, Einrichtungslage, Ich, Rolle};
use quiver_server::{app, Konfig};
use serde_json::{json, Value};
use tower::ServiceExt;

const OEFFENTLICH: &str = "http://127.0.0.1:8740";
const EMAIL: &str = "root@example.test";
const PASSWORT: &str = "ein-sehr-langes-testpasswort";

/// Argon2 läuft höchstens zweimal gleichzeitig; die Tests melden sich
/// nacheinander an, damit sie nicht absichtlich 429 bekommen.
static ARGON_SPERRE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ---------------------------------------------------------------- Gerüst

struct Antwort {
    status: StatusCode,
    json: Value,
    text: String,
    headers: HeaderMap,
}

struct Pruefung {
    ordner: PathBuf,
    router: Router,
    konfig: Konfig,
}

impl Pruefung {
    /// Ein frischer Server ohne Root: die Lage „Ersteinrichtung offen“.
    async fn neu() -> Self {
        Self::bauen(OEFFENTLICH, None).await
    }

    async fn bauen(oeffentlich: &str, vorbereiten: Option<&dyn Fn(&PathBuf)>) -> Self {
        let ordner = ordner();
        if let Some(vorbereiten) = vorbereiten {
            vorbereiten(&ordner);
        }
        Self::mit_konfig(ordner, oeffentlich).await
    }

    async fn mit_konfig(ordner: PathBuf, oeffentlich: &str) -> Self {
        let mut konfig = Konfig::neu(&ordner);
        konfig.oeffentlich = oeffentlich.to_string();
        let router = app(konfig.clone()).await.expect("App baut");
        Self {
            ordner,
            router,
            konfig,
        }
    }

    /// Die Laufzeit vorspulen (das Fenster der Einrichtung hängt daran).
    fn vorspulen(&self, dauer: Duration) {
        self.konfig.laufzeit.vorspulen(dauer);
    }

    fn host(&self) -> String {
        self.konfig
            .oeffentlich
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .to_string()
    }

    async fn stand(&self) -> AuthStand {
        let antwort = self
            .anfrage(
                "GET",
                "/auth/stand",
                None,
                self.konfig.oeffentlich.as_str(),
                None,
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "stand: {}", antwort.text);
        serde_json::from_value(antwort.json).expect("AuthStand")
    }

    /// `POST /auth/einrichten` mit den Köpfen eines echten Browsers.
    async fn einrichten(&self, rumpf: Value) -> Antwort {
        let oeffentlich = self.konfig.oeffentlich.clone();
        self.anfrage(
            "POST",
            "/auth/einrichten",
            Some(serde_json::to_vec(&rumpf).expect("JSON")),
            &oeffentlich,
            Some(("x-quiver", "1")),
        )
        .await
    }

    fn rumpf(email: &str, passwort: &str, code: Option<&str>) -> Value {
        match code {
            Some(code) => json!({"email": email, "passwort": passwort, "code": code}),
            None => json!({"email": email, "passwort": passwort}),
        }
    }

    /// `POST /auth/einrichten` von einer bestimmten Adresse (für die
    /// Fehlversuche je Adresse, Härtung Punkt 3).
    async fn einrichten_von(&self, ip: std::net::IpAddr, rumpf: Value) -> Antwort {
        let oeffentlich = self.konfig.oeffentlich.clone();
        self.anfrage_von(
            ip,
            "POST",
            "/auth/einrichten",
            Some(serde_json::to_vec(&rumpf).expect("JSON")),
            &oeffentlich,
            Some(("x-quiver", "1")),
        )
        .await
    }

    async fn anfrage_von(
        &self,
        ip: std::net::IpAddr,
        methode: &str,
        pfad: &str,
        rumpf: Option<Vec<u8>>,
        origin: &str,
        kopf: Option<(&str, &str)>,
    ) -> Antwort {
        let mut bauer = Request::builder()
            .method(methode)
            .uri(pfad)
            .header(header::HOST, self.host())
            .header("content-type", "application/json");
        if !origin.is_empty() {
            bauer = bauer.header(header::ORIGIN, origin);
        }
        if let Some((name, wert)) = kopf {
            bauer = bauer.header(name, wert);
        }
        let mut anfrage = bauer
            .body(rumpf.map(Body::from).unwrap_or_else(Body::empty))
            .expect("Anfrage");
        anfrage
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                ip, 50000,
            ))));
        let antwort = self.router.clone().oneshot(anfrage).await.expect("Antwort");
        auspacken(antwort).await
    }

    async fn anfrage(
        &self,
        methode: &str,
        pfad: &str,
        rumpf: Option<Vec<u8>>,
        origin: &str,
        kopf: Option<(&str, &str)>,
    ) -> Antwort {
        let mut bauer = Request::builder()
            .method(methode)
            .uri(pfad)
            .header(header::HOST, self.host())
            .header("content-type", "application/json");
        if !origin.is_empty() {
            bauer = bauer.header(header::ORIGIN, origin);
        }
        if let Some((name, wert)) = kopf {
            bauer = bauer.header(name, wert);
        }
        let antwort = self
            .router
            .clone()
            .oneshot(
                bauer
                    .body(rumpf.map(Body::from).unwrap_or_else(Body::empty))
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }
}

/// Aufrufe mit Sitzungscookie (Verwaltung, Geräte).
impl Pruefung {
    async fn mit_cookie_anfrage(
        &self,
        cookie: &str,
        methode: &str,
        pfad: &str,
        rumpf: Option<Value>,
    ) -> Antwort {
        let bauer = Request::builder()
            .method(methode)
            .uri(pfad)
            .header(header::HOST, self.host())
            .header("content-type", "application/json")
            .header(header::ORIGIN, &self.konfig.oeffentlich)
            .header("x-quiver", "1")
            .header(header::COOKIE, cookie);
        let antwort = self
            .router
            .clone()
            .oneshot(
                bauer
                    .body(
                        rumpf
                            .map(|wert| Body::from(serde_json::to_vec(&wert).expect("JSON")))
                            .unwrap_or_else(Body::empty),
                    )
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

fn ordner() -> PathBuf {
    static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
    let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let stempel = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Zeit nach 1970")
        .as_nanos();
    let ordner = std::env::temp_dir().join(format!(
        "quiver-einrichtung-test-{}-{nummer}-{stempel}",
        std::process::id()
    ));
    std::fs::create_dir_all(&ordner).expect("Ordner");
    ordner
}

fn cookie_aus(antwort: &Antwort, name: &str) -> String {
    antwort
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|wert| wert.to_str().ok())
        .find_map(|wert| {
            wert.split(';')
                .next()
                .filter(|teil| teil.starts_with(&format!("{name}=")))
                .map(str::to_string)
        })
        .unwrap_or_default()
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
    Antwort {
        status,
        headers,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        text: String::from_utf8_lossy(&bytes).to_string(),
    }
}

/// Zeitlimit je Test: Hängen ist ein Fehlschlag (Aufgabe 26, Punkt 7).
async fn mit_zeitlimit<T>(was: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(90), was)
        .await
        .expect("Test hängt: Zeitlimit überschritten")
}

// -------------------------------------------------------------------- Tests

#[tokio::test]
async fn stand_zeigt_die_offene_einrichtung() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let stand = p.stand().await;
        assert!(!stand.eingerichtet);
        assert_eq!(stand.einrichtung, Some(Einrichtungslage::Offen));
        assert!(!stand.code_noetig, "Loopback braucht keinen Code");
        assert!(!stand.entra);
        // Genau der Vertrag, den `admin/src/api/typen.ts` liest.
        let roh = p.anfrage("GET", "/auth/stand", None, "", None).await;
        assert_eq!(
            roh.text.trim(),
            "{\"eingerichtet\":false,\"einrichtung\":\"offen\",\
             \"code_noetig\":false,\"entra\":false}"
        );

        // Vor der Einrichtung ist die Verwaltung gesperrt, Geräte nicht.
        let admin = p
            .mit_cookie_anfrage("", "GET", "/api/v1/admin/ueberblick", None)
            .await;
        assert_eq!(admin.status, StatusCode::FORBIDDEN, "{}", admin.text);
        let geraet = p
            .mit_cookie_anfrage("", "GET", "/api/v1/geraet/soll", None)
            .await;
        assert_eq!(
            geraet.status,
            StatusCode::UNAUTHORIZED,
            "das Gerät scheitert am fehlenden Token, nicht an der Einrichtung"
        );
    })
    .await;
}

#[tokio::test]
async fn einrichten_meldet_an_und_ist_danach_zu() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let _wache = ARGON_SPERRE.lock().await;
        let antwort = p.einrichten(Pruefung::rumpf(EMAIL, PASSWORT, None)).await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        let ich: Ich = serde_json::from_value(antwort.json.clone()).expect("Ich");
        assert_eq!(ich.benutzer.rolle, Rolle::Root);
        let cookie = cookie_aus(&antwort, "quiver_sitzung");
        assert!(cookie.starts_with("quiver_sitzung="), "{cookie:?}");

        // Die Sitzung gilt sofort: die Verwaltung antwortet.
        let ueberblick = p
            .mit_cookie_anfrage(&cookie, "GET", "/api/v1/admin/ueberblick", None)
            .await;
        assert_eq!(ueberblick.status, StatusCode::OK, "{}", ueberblick.text);

        // Die Prüfspur nennt die Einrichtung.
        let spur = p
            .mit_cookie_anfrage(&cookie, "GET", "/api/v1/admin/pruefspur", None)
            .await;
        assert_eq!(spur.status, StatusCode::OK);
        assert!(spur.text.contains("einrichtung"), "{}", spur.text);
        assert!(
            !spur.text.contains(PASSWORT),
            "kein Passwort in der Prüfspur"
        );

        // Danach: 404, und die Seite bietet keine Einrichtung mehr an.
        let nochmal = p
            .einrichten(Pruefung::rumpf("zwei@example.test", PASSWORT, None))
            .await;
        assert_eq!(nochmal.status, StatusCode::NOT_FOUND);
        let stand = p.stand().await;
        assert!(stand.eingerichtet);
        assert_eq!(stand.einrichtung, None);
        // Der Stand ist genau der Vertrag der Verwaltung (klein geschrieben).
        let roh = p.anfrage("GET", "/auth/stand", None, "", None).await;
        assert_eq!(
            roh.json,
            json!({
                "eingerichtet": true,
                "einrichtung": null,
                "code_noetig": false,
                "entra": false
            })
        );
    })
    .await;
}

#[tokio::test]
async fn zehn_gleichzeitige_versuche_ergeben_genau_einen_root() {
    mit_zeitlimit(async {
        let p = Arc::new(Pruefung::neu().await);
        let mut aufgaben = Vec::new();
        for nummer in 0..10 {
            let p = p.clone();
            aufgaben.push(tokio::spawn(async move {
                let email = format!("root-{nummer}@example.test");
                let antwort = p.einrichten(Pruefung::rumpf(&email, PASSWORT, None)).await;
                antwort.status
            }));
        }
        let mut erfolge = 0;
        let mut konflikte = 0;
        for aufgabe in aufgaben {
            match aufgabe.await.expect("Aufgabe") {
                StatusCode::OK => erfolge += 1,
                StatusCode::CONFLICT => konflikte += 1,
                anderes => panic!("unerwarteter Status {anderes}"),
            }
        }
        assert_eq!(erfolge, 1, "genau ein Aufruf legt den Root an");
        assert_eq!(konflikte, 9, "die anderen bekommen 409");

        // In der Datenbank steht genau ein Root.
        let p2 = Arc::try_unwrap(p)
            .map_err(|_| ())
            .expect("keine weiteren Halter");
        let liste = p2
            .mit_cookie_anfrage("", "GET", "/api/v1/admin/benutzer", None)
            .await;
        // Ohne Sitzung: 401 – die Zahl prüfen wir direkt in der Datenbank.
        assert_eq!(liste.status, StatusCode::UNAUTHORIZED);
        let conn = rusqlite::Connection::open(p2.ordner.join("quiver.db")).expect("DB");
        let anzahl: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM benutzer WHERE oid = 'root'",
                [],
                |z| z.get(0),
            )
            .expect("Zahl");
        assert_eq!(anzahl, 1);
    })
    .await;
}

#[tokio::test]
async fn fenster_schliesst_nach_15_minuten_laufzeit() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        // Wanduhr weit zurückdrehen: das Fenster hängt an der Laufzeit.
        p.konfig
            .uhr
            .setze(time::OffsetDateTime::from_unix_timestamp(1_000_000).expect("Zeit"));
        assert_eq!(p.stand().await.einrichtung, Some(Einrichtungslage::Offen));

        p.vorspulen(Duration::from_secs(15 * 60));
        assert_eq!(
            p.stand().await.einrichtung,
            Some(Einrichtungslage::Geschlossen)
        );
        let roh = p.anfrage("GET", "/auth/stand", None, "", None).await;
        assert_eq!(roh.json["einrichtung"], "geschlossen");
        let antwort = p.einrichten(Pruefung::rumpf(EMAIL, PASSWORT, None)).await;
        assert_eq!(antwort.status, StatusCode::FORBIDDEN, "{}", antwort.text);
        assert!(antwort.text.contains("neu starten"));
    })
    .await;
}

#[tokio::test]
async fn host_origin_und_x_quiver_sind_pflicht() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let rumpf = serde_json::to_vec(&Pruefung::rumpf(EMAIL, PASSWORT, None)).expect("JSON");

        // Fremder Host: 421 ohne Inhalt, noch vor jedem Handler – auch beim Stand.
        let fremd = Request::builder()
            .method("POST")
            .uri("/auth/einrichten")
            .header(header::HOST, "evil.example.test")
            .header(header::ORIGIN, OEFFENTLICH)
            .header("x-quiver", "1")
            .header("content-type", "application/json")
            .body(Body::from(rumpf.clone()))
            .expect("Anfrage");
        let antwort = p.router.clone().oneshot(fremd).await.expect("Antwort");
        assert_eq!(antwort.status(), StatusCode::MISDIRECTED_REQUEST);
        let inhalt = antwort
            .into_body()
            .collect()
            .await
            .expect("Rumpf")
            .to_bytes();
        assert!(inhalt.is_empty(), "421 antwortet ohne Inhalt");

        let stand = Request::builder()
            .uri("/auth/stand")
            .header(header::HOST, "evil.example.test")
            .body(Body::empty())
            .expect("Anfrage");
        let antwort = p.router.clone().oneshot(stand).await.expect("Antwort");
        assert_eq!(antwort.status(), StatusCode::MISDIRECTED_REQUEST);

        // Fremder Origin.
        let antwort = p
            .anfrage(
                "POST",
                "/auth/einrichten",
                Some(rumpf.clone()),
                "https://evil.example.test",
                Some(("x-quiver", "1")),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::FORBIDDEN);

        // Kein `X-Quiver: 1`.
        let antwort = p
            .anfrage("POST", "/auth/einrichten", Some(rumpf), OEFFENTLICH, None)
            .await;
        assert_eq!(antwort.status, StatusCode::FORBIDDEN);

        // Etwas ist trotzdem nicht entstanden.
        assert!(!p.stand().await.eingerichtet);
    })
    .await;
}

/// Härtung Punkt 2: der Code ist Pflicht, sobald der Server **nicht nur auf
/// Loopback** lauscht – auch wenn die Basis-URL lokal ist (so läuft der
/// Container). Die Meldung „nur lokal erreichbar" gilt dann nicht mehr.
#[tokio::test(flavor = "current_thread")]
async fn offene_lauschadresse_verlangt_den_code_auch_bei_lokaler_basis_url() {
    mit_zeitlimit(async {
        let ordner = ordner();
        let mut konfig = Konfig::neu(&ordner);
        // Basis-URL lokal, aber der Server lauscht im Netz.
        konfig.oeffentlich = String::from(OEFFENTLICH);
        konfig.adresse = "0.0.0.0:8740".parse().expect("Adresse");
        assert!(konfig.einrichtung_mit_code(), "die Adresse entscheidet");
        let router = app(konfig.clone()).await.expect("App baut");
        let p = Pruefung {
            ordner,
            router,
            konfig,
        };
        let stand = p.stand().await;
        assert!(stand.code_noetig, "der Code ist Pflicht");
        // Ohne Code gibt es keinen Root.
        let antwort = p.einrichten(Pruefung::rumpf(EMAIL, PASSWORT, None)).await;
        assert_eq!(antwort.status, StatusCode::BAD_REQUEST, "{}", antwort.text);
        assert!(!p.stand().await.eingerichtet);
        let _ = std::fs::remove_dir_all(&p.ordner);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn code_fehlt_falsch_und_nach_zehn_versuchen_zu() {
    let puffer = Mitlesen::default();
    let abonnent = tracing_subscriber::fmt()
        .with_writer(puffer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _wache = tracing::subscriber::set_default(abonnent);
    // Ohne das Neuberechnen hängt es vom Zufall ab, ob der Interesse-Cache den
    // Aufrufpunkt schon kennt (viele Tests schreiben gleichzeitig ins Log).
    tracing::callsite::rebuild_interest_cache();
    mit_zeitlimit(async {
        // Nicht auf Loopback: der Einmal-Code aus dem Log ist Pflicht.
        let p = Pruefung::bauen("https://quiver.example.test", None).await;
        let stand = p.stand().await;
        assert!(stand.code_noetig);
        assert_eq!(stand.einrichtung, Some(Einrichtungslage::Offen));

        // Fehlt der Code, gibt es 400.
        let antwort = p.einrichten(Pruefung::rumpf(EMAIL, PASSWORT, None)).await;
        assert_eq!(antwort.status, StatusCode::BAD_REQUEST, "{}", antwort.text);

        // Falsch: 403.
        let antwort = p
            .einrichten(Pruefung::rumpf(EMAIL, PASSWORT, Some("falsch")))
            .await;
        assert_eq!(antwort.status, StatusCode::FORBIDDEN);

        // Zehnmal falsch von **einer** Adresse: nur diese ist gesperrt, die
        // Einrichtung bleibt offen (global erst nach hundert Fehlversuchen).
        let eine: std::net::IpAddr = "203.0.113.9".parse().unwrap();
        for _ in 0..10 {
            let _ = p
                .einrichten_von(
                    eine,
                    Pruefung::rumpf(EMAIL, PASSWORT, Some("immer-noch-falsch")),
                )
                .await;
        }
        assert_eq!(
            p.stand().await.einrichtung,
            Some(Einrichtungslage::Offen),
            "eine Adresse sperrt die Einrichtung nicht für alle"
        );
        // Von einer anderen Adresse geht es weiter (und bleibt falsch).
        let andere: std::net::IpAddr = "203.0.113.10".parse().unwrap();
        let antwort = p
            .einrichten_von(
                andere,
                Pruefung::rumpf(EMAIL, PASSWORT, Some("immer-noch-falsch")),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::FORBIDDEN, "{}", antwort.text);
        // Die gesperrte Adresse bekommt keine weiteren Versuche.
        let antwort = p
            .einrichten_von(
                eine,
                Pruefung::rumpf(EMAIL, PASSWORT, Some("immer-noch-falsch")),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::TOO_MANY_REQUESTS);

        // Ein frischer Server: der richtige Code steht nur im Log und öffnet.
        let p2 = Pruefung::bauen("https://quiver.example.test", None).await;
        let code = code_aus_log(&puffer.text()).expect("Codezeile im Log");
        let antwort = p2
            .einrichten(Pruefung::rumpf(EMAIL, PASSWORT, Some(&code)))
            .await;
        assert_eq!(
            antwort.status,
            StatusCode::OK,
            "der Code aus dem Log öffnet: {}",
            antwort.text
        );
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn passwort_und_code_stehen_nur_in_der_einen_codezeile_im_log() {
    let puffer = Mitlesen::default();
    let abonnent = tracing_subscriber::fmt()
        .with_writer(puffer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _wache = tracing::subscriber::set_default(abonnent);
    tracing::callsite::rebuild_interest_cache();
    mit_zeitlimit(async {
        let p = Pruefung::bauen("https://quiver.example.test", None).await;
        let code = code_aus_log(&puffer.text()).expect("Codezeile");
        let antwort = p
            .einrichten(Pruefung::rumpf(EMAIL, PASSWORT, Some(&code)))
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);

        let text = puffer.text();
        assert!(
            !text.contains(PASSWORT),
            "das Passwort steht im Log: {text}"
        );
        assert_eq!(
            text.matches(&code).count(),
            1,
            "der Code steht genau einmal im Log (die eine Codezeile)"
        );
    })
    .await;
}

#[tokio::test]
async fn admin_token_wird_geloescht_und_daten_bleiben() {
    mit_zeitlimit(async {
        let token_datei = Arc::new(Mutex::new(PathBuf::new()));
        let datei = token_datei.clone();
        let p = Pruefung::bauen(
            OEFFENTLICH,
            Some(&move |ordner: &PathBuf| {
                let pfad = ordner.join("admin-token");
                std::fs::write(&pfad, "alter-schluessel").expect("admin-token");
                *datei.lock().expect("Sperre") = pfad;
            }),
        )
        .await;
        let pfad = token_datei.lock().expect("Sperre").clone();
        assert!(pfad.exists());

        let _wache = ARGON_SPERRE.lock().await;
        let antwort = p.einrichten(Pruefung::rumpf(EMAIL, PASSWORT, None)).await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        assert!(!pfad.exists(), "admin-token ist nach der Einrichtung weg");

        // Vorhandene Daten (Eintrag, Satz, Gruppe) überleben die Einrichtung.
        let cookie = cookie_aus(&antwort, "quiver_sitzung");
        let eintrag = eintrag();
        let antwort = p
            .mit_cookie_anfrage(
                &cookie,
                "PUT",
                "/api/v1/admin/eintraege/wetter",
                Some(json!(eintrag)),
            )
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        let liste = p
            .mit_cookie_anfrage(&cookie, "GET", "/api/v1/admin/eintraege", None)
            .await;
        assert_eq!(liste.status, StatusCode::OK);
        assert_eq!(liste.json.as_array().map(Vec::len), Some(1));
    })
    .await;
}

#[tokio::test]
async fn falsche_eingaben_werden_abgelehnt() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        for rumpf in [
            Pruefung::rumpf("keine-email", PASSWORT, None),
            Pruefung::rumpf(EMAIL, "kurz", None),
            Pruefung::rumpf("", PASSWORT, None),
        ] {
            let antwort = p.einrichten(rumpf.clone()).await;
            assert_eq!(
                antwort.status,
                StatusCode::BAD_REQUEST,
                "{rumpf}: {}",
                antwort.text
            );
        }
        assert!(!p.stand().await.eingerichtet);
    })
    .await;
}

#[tokio::test]
async fn einstellungen_der_seite_sind_offen_und_sicher() {
    mit_zeitlimit(async {
        let p = Pruefung::neu().await;
        let css = p.anfrage("GET", "/auth/stil.css", None, "", None).await;
        assert_eq!(css.status, StatusCode::OK);
        assert_eq!(
            css.headers.get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );
        assert!(css.text.contains("prefers-color-scheme: light"));
        assert_eq!(
            css.headers.get(header::REFERRER_POLICY).unwrap(),
            "same-origin"
        );

        let bild = p
            .anfrage("GET", "/auth/marke-hell.png", None, "", None)
            .await;
        assert_eq!(bild.status, StatusCode::OK);
        assert_eq!(bild.headers.get(header::CONTENT_TYPE).unwrap(), "image/png");
    })
    .await;
}

#[tokio::test]
async fn heutige_datenbank_ohne_root_zeigt_die_einrichtung() {
    mit_zeitlimit(async {
        // Eine Datenbank im heutigen Schema, mit Daten, aber ohne Root:
        // Eintrag anlegen, dann den Root-Benutzer entfernen.
        let p = Pruefung::neu().await;
        let _wache = ARGON_SPERRE.lock().await;
        let antwort = p.einrichten(Pruefung::rumpf(EMAIL, PASSWORT, None)).await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        let cookie = cookie_aus(&antwort, "quiver_sitzung");
        let eintrag = eintrag();
        let gespeichert = p
            .mit_cookie_anfrage(
                &cookie,
                "PUT",
                "/api/v1/admin/eintraege/wetter",
                Some(json!(eintrag)),
            )
            .await;
        assert_eq!(gespeichert.status, StatusCode::OK, "{}", gespeichert.text);

        // Der Ordner muss bleiben: `Pruefung` räumt beim Fallenlassen auf.
        let ordner = p.ordner.clone();
        std::mem::forget(p);
        let conn = rusqlite::Connection::open(ordner.join("quiver.db")).expect("DB");
        conn.execute("DELETE FROM benutzer WHERE oid = 'root'", [])
            .expect("Root entfernen");
        conn.execute("DELETE FROM sitzungen", [])
            .expect("Sitzungen");
        drop(conn);

        // Derselbe Ordner, neuer Server: die Einrichtung ist offen, die Daten
        // sind noch da.
        let neuer = Pruefung::mit_konfig(ordner, OEFFENTLICH).await;
        let stand = neuer.stand().await;
        assert!(!stand.eingerichtet);
        assert_eq!(stand.einrichtung, Some(Einrichtungslage::Offen));
        let antwort = neuer
            .einrichten(Pruefung::rumpf("neu@example.test", PASSWORT, None))
            .await;
        assert_eq!(antwort.status, StatusCode::OK, "{}", antwort.text);
        let cookie = cookie_aus(&antwort, "quiver_sitzung");
        let liste = neuer
            .mit_cookie_anfrage(&cookie, "GET", "/api/v1/admin/eintraege", None)
            .await;
        assert_eq!(liste.status, StatusCode::OK, "{}", liste.text);
        assert_eq!(
            liste.json.as_array().map(Vec::len),
            Some(1),
            "der vorhandene Eintrag bleibt: {}",
            liste.text
        );
        // Der Test räumt selbst auf: `Pruefung::mit_konfig` legt keinen neuen
        // Ordner an, deshalb hier löschen.
        let _ = std::fs::remove_dir_all(neuer.ordner.clone());
    })
    .await;
}

// -------------------------------------------------------------- Hilfsmittel

fn eintrag() -> Eintrag {
    Eintrag {
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
        felder: Vec::new(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
    }
}

/// Den Einmal-Code aus der **letzten** Codezeile im Log holen (jeder Start
/// erzeugt einen neuen).
fn code_aus_log(text: &str) -> Option<String> {
    let marker = "Code für diesen Start: ";
    let start = text.rfind(marker)? + marker.len();
    let rest = &text[start..];
    let ende = rest
        .find(|zeichen: char| zeichen.is_whitespace())
        .unwrap_or(rest.len());
    Some(rest[..ende].to_string())
}

#[derive(Clone, Default)]
struct Mitlesen(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Mitlesen {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("Sperre").extend_from_slice(bytes);
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
        String::from_utf8_lossy(&self.0.lock().expect("Sperre")).to_string()
    }
}
