// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahmetests für den Server: gegen [`quiver_server::app`] mit
//! `tower::ServiceExt::oneshot` und einem temporären Datenordner.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use quiver_domain::{
    Eintrag, EintragId, Feld, FeldArt, FeldOption, Geraet, GeraetId, Gruppe, GruppeId, Herkunft,
    Laufzeit, Quelle, Satz, SatzEintrag, SatzId, Soll, Werte,
};
use quiver_protocol::{
    Anmelden, AnmeldenAntwort, Code, CodeAnlegen, EintragZustand, GeraetAendern, GeraetStand,
    Meldung, PruefspurEintrag, Ueberblick,
};
use quiver_server::{app, benutzer, Konfig};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tower::ServiceExt;

const ROOT_EMAIL: &str = "root@example.test";
const ROOT_PASSWORT: &str = "ein-sehr-langes-testpasswort";
const OEFFENTLICH: &str = "http://127.0.0.1:8740";
/// Der `Host`-Kopf, der zu [`OEFFENTLICH`] passt (Port gehört dazu).
const HOST: &str = "127.0.0.1:8740";
/// Ein Token, das zu keinem Gerät gehört.
const FALSCH: &str = "kein-gueltiges-token";

/// Argon2 läuft höchstens zweimal gleichzeitig; Tests melden sich nacheinander an.
static ROOT_SPERRE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ---------------------------------------------------------------- Gerüst

struct Pruefung {
    ordner: PathBuf,
    router: Router,
    /// Sitzungscookie der Verwaltung (nach der Root-Anmeldung).
    cookie: String,
}

impl Pruefung {
    async fn neu() -> Self {
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::Relaxed);
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "quiver-server-test-{}-{nummer}-{stempel}",
            std::process::id()
        ));
        benutzer::root_setzen_auf_platte(&ordner, ROOT_EMAIL, ROOT_PASSWORT).expect("Root setzen");
        let router = app(Konfig::neu(&ordner)).await.expect("App baut");
        let mut p = Self {
            ordner,
            router,
            cookie: String::new(),
        };
        p.cookie = p.root_anmelden().await;
        p
    }

    /// Root-Anmeldung über die API; liefert das Sitzungscookie.
    async fn root_anmelden(&self) -> String {
        let _wache = ROOT_SPERRE.lock().await;
        let anfrage = Request::builder()
            .method("POST")
            .uri("/api/v1/auth/root")
            .header("host", HOST)
            .header("content-type", "application/json")
            .header("origin", OEFFENTLICH)
            .header("x-quiver", "1")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "email": ROOT_EMAIL,
                    "passwort": ROOT_PASSWORT
                }))
                .expect("JSON"),
            ))
            .expect("Anfrage");
        let antwort = self.router.clone().oneshot(anfrage).await.expect("Antwort");
        if antwort.status() != StatusCode::OK {
            let status = antwort.status();
            let text = antwort
                .into_body()
                .collect()
                .await
                .expect("Rumpf")
                .to_bytes();
            panic!(
                "Root-Anmeldung: {status} {}",
                String::from_utf8_lossy(&text)
            );
        }
        let set = antwort
            .headers()
            .get(header::SET_COOKIE)
            .expect("Sitzungscookie")
            .to_str()
            .expect("Cookie ist Text");
        set.split(';').next().expect("Name=Wert").to_string()
    }

    async fn anfrage(
        &self,
        methode: &str,
        pfad: &str,
        token: Option<&str>,
        rumpf: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut bauer = Request::builder()
            .method(methode)
            .uri(format!("/api/v1{pfad}"))
            .header("host", HOST)
            .header("content-type", "application/json");
        if let Some(token) = token {
            bauer = bauer.header("authorization", format!("Bearer {token}"));
        }
        let koerper = match rumpf {
            Some(wert) => Body::from(serde_json::to_vec(&wert).expect("JSON")),
            None => Body::empty(),
        };
        let antwort = self
            .router
            .clone()
            .oneshot(bauer.body(koerper).expect("Anfrage"))
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }

    /// Verwaltungs-Aufruf mit Sitzung, Origin und `X-Quiver: 1`.
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
                    .header("origin", OEFFENTLICH)
                    .header("x-quiver", "1")
                    .body(koerper)
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        auspacken(antwort).await
    }

    /// Verwaltungs-Aufruf mit einem zusätzlichen Kopf; liefert Status, JSON und
    /// die Köpfe (für `ETag`/`If-Match`).
    async fn admin_mit_kopf(
        &self,
        methode: &str,
        pfad: &str,
        extra: (&str, &str),
        rumpf: Option<Value>,
    ) -> (StatusCode, Value, HeaderMap) {
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
                    .header("origin", OEFFENTLICH)
                    .header("x-quiver", "1")
                    .header(extra.0, extra.1)
                    .body(koerper)
                    .expect("Anfrage"),
            )
            .await
            .expect("Antwort");
        let status = antwort.status();
        let headers = antwort.headers().clone();
        let bytes = antwort
            .into_body()
            .collect()
            .await
            .expect("Rumpf")
            .to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json, headers)
    }

    async fn put_eintrag(&self, eintrag: &Eintrag) -> (StatusCode, Value) {
        self.admin(
            "PUT",
            &format!("/admin/eintraege/{}", eintrag.id),
            Some(json!(eintrag)),
        )
        .await
    }

    async fn put_satz(&self, satz: &Satz) -> (StatusCode, Value) {
        self.admin(
            "PUT",
            &format!("/admin/saetze/{}", satz.id),
            Some(json!(satz)),
        )
        .await
    }

    async fn put_gruppe(&self, gruppe: &Gruppe) -> (StatusCode, Value) {
        self.admin(
            "PUT",
            &format!("/admin/gruppen/{}", gruppe.id),
            Some(json!(gruppe)),
        )
        .await
    }

    async fn code_anlegen(&self, gruppen: &[&str], minuten: Option<u32>) -> Code {
        self.code_anlegen_mit(gruppen, minuten, false).await
    }

    async fn code_anlegen_mit(
        &self,
        gruppen: &[&str],
        minuten: Option<u32>,
        unbegrenzt: bool,
    ) -> Code {
        let anfrage = CodeAnlegen {
            gruppen: gruppen.iter().map(|g| GruppeId::neu(*g)).collect(),
            minuten,
            unbegrenzt,
        };
        let (status, rumpf) = self
            .admin("POST", "/admin/codes", Some(json!(anfrage)))
            .await;
        assert_eq!(status, StatusCode::OK, "Code anlegen: {rumpf}");
        als(&rumpf)
    }

    async fn anmelden(&self, code: &str, name: &str) -> AnmeldenAntwort {
        let (status, rumpf) = self
            .anfrage(
                "POST",
                "/anmelden",
                None,
                Some(json!(Anmelden {
                    code: code.to_string(),
                    name: name.to_string(),
                })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "Anmelden: {rumpf}");
        als(&rumpf)
    }

    /// Eintrag, Satz, Gruppe, Code, Anmelden – der übliche Weg bis zum Gerät.
    async fn aufbau(&self) -> AnmeldenAntwort {
        assert_eq!(self.put_eintrag(&eintrag("wetter")).await.0, StatusCode::OK);
        assert_eq!(
            self.put_satz(&satz("basis", "wetter", json!({ "PFAD": "/tmp/wetter" })))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            self.put_gruppe(&gruppe("team", &["basis"])).await.0,
            StatusCode::OK
        );
        let code = self.code_anlegen(&["team"], Some(60)).await;
        self.anmelden(&code.code, "Testrechner").await
    }

    async fn soll(&self, token: &str) -> Soll {
        let (status, rumpf) = self.anfrage("GET", "/geraet/soll", Some(token), None).await;
        assert_eq!(status, StatusCode::OK, "Soll: {rumpf}");
        als(&rumpf)
    }
}

impl Pruefung {
    /// Wie [`Pruefung::neu`], aber mit einer gebauten Admin-Oberfläche.
    async fn mit_oberflaeche() -> Self {
        let mut p = Self::neu().await;
        let dist = p.ordner.join("dist");
        std::fs::create_dir_all(&dist).expect("Ordner");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>quiver</title>",
        )
        .expect("index.html");
        p.router = app(Konfig::neu(&p.ordner).mit_admin_dist(Some(dist)))
            .await
            .expect("App baut");
        p
    }

    /// Anfrage ohne das Präfix `/api/v1` (für die ausgelieferte Oberfläche).
    async fn roh(&self, pfad: &str) -> (StatusCode, String) {
        let anfrage = Request::builder()
            .uri(pfad)
            .header("host", HOST)
            .body(Body::empty())
            .expect("Anfrage");
        let antwort = self.router.clone().oneshot(anfrage).await.expect("Antwort");
        let status = antwort.status();
        let bytes = antwort
            .into_body()
            .collect()
            .await
            .expect("Rumpf lesbar")
            .to_bytes();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

fn als<T: DeserializeOwned>(wert: &Value) -> T {
    serde_json::from_value(wert.clone()).expect("Antwort passt zum Vertrag")
}

async fn auspacken(antwort: Response) -> (StatusCode, Value) {
    let status = antwort.status();
    let bytes = antwort
        .into_body()
        .collect()
        .await
        .expect("Rumpf lesbar")
        .to_bytes();
    let wert = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, wert)
}

// ------------------------------------------------------------ Testdaten

/// Ein Eintrag, der ein Programm startet – ohne Felder, damit nichts fehlt.
fn eintrag_mit(id: &str, befehl: &str) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "1.0.0".into(),
        titel: format!("Titel {id}"),
        beschreibung: "Ein Testeintrag".into(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: befehl.into(),
            argumente: Vec::new(),
            umgebung: Werte::new(),
        },
        felder: Vec::new(),
    }
}

fn eintrag(id: &str) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "1.0.0".into(),
        titel: format!("Titel {id}"),
        beschreibung: "Ein Testeintrag".into(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: "npx".into(),
            argumente: vec!["-y".into(), format!("{id}@1.0.0")],
            umgebung: Werte::new(),
        },
        felder: vec![Feld {
            name: "PFAD".into(),
            beschreibung: "Ordner".into(),
            quelle: Quelle::Server,
            pflicht: true,
            standard: None,
            link: None,
            ..Default::default()
        }],
    }
}

fn satz(id: &str, eintrag_id: &str, werte: Value) -> Satz {
    Satz {
        id: SatzId::neu(id),
        titel: format!("Satz {id}"),
        prioritaet: 0,
        eintraege: vec![SatzEintrag {
            eintrag: EintragId::neu(eintrag_id),
            werte: serde_json::from_value::<Werte>(werte).expect("Werte"),
        }],
    }
}

fn gruppe(id: &str, saetze: &[&str]) -> Gruppe {
    Gruppe {
        id: GruppeId::neu(id),
        titel: format!("Gruppe {id}"),
        saetze: saetze.iter().map(|s| SatzId::neu(*s)).collect(),
        entra: Vec::new(),
    }
}

// ------------------------------------------------------------- Die Tests

#[tokio::test]
async fn ohne_token_kein_zugriff() {
    let p = Pruefung::neu().await;

    let (status, rumpf) = p.anfrage("GET", "/geraet/soll", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(rumpf["fehler"].is_string(), "Fehler als Fehler-Objekt");

    let (status, _) = p
        .anfrage("GET", "/admin/eintraege", Some("falsches-token"), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Geräte-Token öffnet keinen Admin-Pfad …
    let antwort = p.aufbau().await;
    let (status, _) = p
        .anfrage("GET", "/admin/eintraege", Some(&antwort.token), None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // … und ein fremdes Bearer-Token öffnet keinen Geräte-Pfad.
    let (status, _) = p.anfrage("GET", "/geraet/soll", Some(FALSCH), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Die Sitzung kommt durch.
    let (status, _) = p.admin("GET", "/admin/ueberblick", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn ablauf_vom_eintrag_bis_zum_soll() {
    let p = Pruefung::neu().await;
    let antwort = p.aufbau().await;

    assert_eq!(antwort.geraet.name, "Testrechner");
    assert_eq!(antwort.geraet.gruppen, vec![GruppeId::neu("team")]);
    assert_eq!(antwort.geraet.id.als_str().len(), 12, "zwölf Zeichen hex");
    assert_eq!(antwort.token.len(), 64, "32 Byte als hex");

    let soll = p.soll(&antwort.token).await;
    assert_eq!(soll.eintraege.len(), 1);
    let eintrag = &soll.eintraege[0];
    assert_eq!(eintrag.eintrag.id, EintragId::neu("wetter"));
    assert_eq!(
        eintrag.werte.get("PFAD").map(String::as_str),
        Some("/tmp/wetter"),
        "Server-Wert aus dem Satz"
    );

    // Das Gerät meldet sich; die Admin-Sicht zeigt die Meldung.
    let meldung = Meldung {
        revision: soll.revision.clone(),
        client_version: "0.1.0".into(),
        eintraege: vec![(
            EintragId::neu("wetter"),
            EintragZustand::Laeuft { tools: 3 },
        )],
        programme: Vec::new(),
    };
    let (status, _) = p
        .anfrage(
            "POST",
            "/geraet/meldung",
            Some(&antwort.token),
            Some(json!(meldung)),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, rumpf) = p.admin("GET", "/admin/geraete", None).await;
    assert_eq!(status, StatusCode::OK);
    let staende: Vec<GeraetStand> = als(&rumpf);
    assert_eq!(staende.len(), 1);
    assert_eq!(staende[0].soll_revision, soll.revision);
    assert_eq!(staende[0].meldung.as_ref(), Some(&meldung));
    assert!(staende[0].zuletzt_gesehen.is_some());

    let (_, rumpf) = p.admin("GET", "/admin/ueberblick", None).await;
    let ueberblick: Ueberblick = als(&rumpf);
    assert_eq!(ueberblick.eintraege, 1);
    assert_eq!(ueberblick.saetze, 1);
    assert_eq!(ueberblick.gruppen, 1);
    assert_eq!(ueberblick.geraete, 1);
    assert!(ueberblick.geraete_verbunden <= 1);
    assert_eq!(ueberblick.server_version, env!("CARGO_PKG_VERSION"));

    // Ein zweiter Satz mit höherer Priorität gewinnt (Regel aus `domain`).
    assert_eq!(
        p.put_satz(&Satz {
            id: SatzId::neu("streng"),
            titel: "Streng".into(),
            prioritaet: 5,
            eintraege: vec![SatzEintrag {
                eintrag: EintragId::neu("wetter"),
                werte: serde_json::from_value::<Werte>(json!({ "PFAD": "/tmp/streng" }))
                    .expect("Werte"),
            }],
        })
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        p.put_gruppe(&gruppe("team", &["basis", "streng"])).await.0,
        StatusCode::OK
    );
    let soll = p.soll(&antwort.token).await;
    assert_eq!(
        soll.eintraege[0].werte.get("PFAD").map(String::as_str),
        Some("/tmp/streng")
    );
}

#[tokio::test]
async fn codes_sind_einmalig_und_laufen_ab() {
    let p = Pruefung::neu().await;
    assert_eq!(p.put_gruppe(&gruppe("team", &[])).await.0, StatusCode::OK);

    let code = p.code_anlegen(&["team"], Some(60)).await;
    assert_eq!(code.code.len(), 9, "XXXX-XXXX");
    assert_eq!(code.code.as_bytes()[4], b'-');
    let bis = code.gueltig_bis.clone().expect("mit Ablauf");
    assert!(bis.contains('T'), "RFC 3339: {bis}");

    // Anzeigeform, ohne Bindestrich und klein geschrieben sind alle gültig.
    let ohne_strich = code.code.replace('-', "");
    let antwort = p.anmelden(&ohne_strich.to_lowercase(), "Rechner A").await;
    assert_eq!(antwort.geraet.gruppen, vec![GruppeId::neu("team")]);

    // Zweite Benutzung: Fehler (der Code ist verbraucht).
    let (status, rumpf) = p
        .anfrage(
            "POST",
            "/anmelden",
            None,
            Some(json!(Anmelden {
                code: code.code.clone(),
                name: "Rechner B".into()
            })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(rumpf["fehler"].is_string());

    // Abgelaufener Code → 410.
    let kurz = p.code_anlegen(&[], Some(0)).await;
    let (status, rumpf) = p
        .anfrage(
            "POST",
            "/anmelden",
            None,
            Some(json!(Anmelden {
                code: kurz.code,
                name: "Rechner C".into()
            })),
        )
        .await;
    assert_eq!(status, StatusCode::GONE, "abgelaufen: {rumpf}");
}

#[tokio::test]
async fn ein_code_ohne_ablauf_gilt_bis_er_eingeloest_ist() {
    let p = Pruefung::neu().await;
    assert_eq!(p.put_gruppe(&gruppe("team", &[])).await.0, StatusCode::OK);

    // `minuten` wird bei `unbegrenzt` nicht beachtet – auch 0 läuft nicht ab.
    let code = p.code_anlegen_mit(&["team"], Some(0), true).await;
    assert_eq!(code.gueltig_bis, None, "ohne Ablauf");
    let antwort = p.anmelden(&code.code, "Rechner A").await;
    assert_eq!(antwort.geraet.gruppen, vec![GruppeId::neu("team")]);

    // Einmalig bleibt er trotzdem.
    let (status, _) = p
        .anfrage(
            "POST",
            "/anmelden",
            None,
            Some(json!(Anmelden {
                code: code.code.clone(),
                name: "Rechner B".into()
            })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "verbraucht");
}

#[tokio::test]
async fn konflikte_und_falsche_kennungen() {
    let p = Pruefung::neu().await;
    let antwort = p.aufbau().await;

    // Benutzter Eintrag → 409, benutzter Satz → 409.
    let (status, _) = p.admin("DELETE", "/admin/eintraege/wetter", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = p.admin("DELETE", "/admin/saetze/basis", None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Gruppe ist niemandem im Weg.
    let (status, _) = p.admin("DELETE", "/admin/gruppen/team", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // Jetzt geht der Satz, danach der Eintrag.
    let (status, _) = p.admin("DELETE", "/admin/saetze/basis", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = p.admin("DELETE", "/admin/eintraege/wetter", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = p.admin("DELETE", "/admin/eintraege/wetter", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Kennung im Pfad ≠ Rumpf → 400.
    let (status, _) = p
        .admin(
            "PUT",
            "/admin/eintraege/anders",
            Some(json!(eintrag("wetter"))),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = p
        .admin(
            "PUT",
            "/admin/saetze/anders",
            Some(json!(satz("basis", "wetter", json!({})))),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = p
        .admin(
            "PUT",
            "/admin/gruppen/anders",
            Some(json!(gruppe("team", &[]))),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Satz auf einen unbekannten Eintrag → 422 mit dem Namen.
    let (status, rumpf) = p
        .put_satz(&satz("kaputt", "gibtsnicht", json!({ "PFAD": "/tmp" })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{rumpf}");
    assert!(
        rumpf["fehler"]
            .as_str()
            .unwrap_or_default()
            .contains("gibtsnicht"),
        "der unbekannte Eintrag wird benannt: {rumpf}"
    );

    // Nach dem Löschen der Gruppe bekommt das Gerät nichts mehr.
    let soll = p.soll(&antwort.token).await;
    assert!(soll.eintraege.is_empty());

    // Falsche Methode: 405, ebenfalls als Fehler-Objekt.
    let (status, rumpf) = p.admin("GET", "/anmelden", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(rumpf["fehler"].is_string(), "405 im Vertrag: {rumpf}");

    // Ohne Token kommt erst die Anmeldung (401), dann die Methode.
    let (status, rumpf) = p.anfrage("POST", "/geraet/soll", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(rumpf["fehler"].is_string());

    // Unbekannter API-Pfad: 404 mit Fehler-Objekt, keine Admin-Oberfläche.
    let (status, rumpf) = p.admin("GET", "/gibtsnicht", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(rumpf["fehler"].is_string());
}

#[tokio::test]
async fn gesperrtes_geraet_bekommt_403() {
    let p = Pruefung::neu().await;
    let antwort = p.aufbau().await;

    let (status, rumpf) = p
        .admin(
            "PUT",
            &format!("/admin/geraete/{}", antwort.geraet.id),
            Some(json!(GeraetAendern {
                name: "Gesperrt".into(),
                gruppen: vec![GruppeId::neu("team")],
                gesperrt: true,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let stand: GeraetStand = als(&rumpf);
    assert!(stand.geraet.gesperrt);
    assert_eq!(stand.geraet.name, "Gesperrt");

    for pfad in ["/geraet/soll", "/geraet/ereignisse"] {
        let (status, _) = p.anfrage("GET", pfad, Some(&antwort.token), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{pfad}");
    }

    // Wieder freigeben: das Gerät bekommt wieder sein Soll.
    assert_eq!(
        p.admin(
            "PUT",
            &format!("/admin/geraete/{}", antwort.geraet.id),
            Some(json!(GeraetAendern {
                name: "Frei".into(),
                gruppen: vec![GruppeId::neu("team")],
                gesperrt: false,
            })),
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(p.soll(&antwort.token).await.eintraege.len(), 1);

    // Unbekanntes Gerät → 404.
    let (status, _) = p
        .admin(
            "PUT",
            "/admin/geraete/000000000000",
            Some(json!(GeraetAendern {
                name: "Nirgends".into(),
                gruppen: Vec::new(),
                gesperrt: false,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn eintrag_behaelt_art_und_titel() {
    let p = Pruefung::neu().await;
    let mut eintrag = eintrag("abap");
    eintrag.felder = vec![
        Feld {
            name: "zertifikat_pruefen".into(),
            beschreibung: "Prüft das Zertifikat".into(),
            quelle: Quelle::Client,
            pflicht: false,
            art: FeldArt::Schalter,
            titel: Some("Zertifikat prüfen".into()),
            ..Default::default()
        },
        Feld {
            name: "mandant".into(),
            beschreibung: "SAP-Mandant".into(),
            quelle: Quelle::Server,
            art: FeldArt::Auswahl {
                optionen: vec![
                    FeldOption {
                        wert: "100".into(),
                        anzeige: "Entwicklung".into(),
                    },
                    FeldOption {
                        wert: "200".into(),
                        anzeige: "Test".into(),
                    },
                ],
            },
            titel: Some("Mandant".into()),
            ..Default::default()
        },
    ];

    let (status, rumpf) = p.put_eintrag(&eintrag).await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let (status, rumpf) = p.admin("GET", "/admin/eintraege/abap", None).await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let gelesen: Eintrag = als(&rumpf);
    assert_eq!(
        gelesen.felder, eintrag.felder,
        "art und titel gehen unverändert durch den Server"
    );
}

#[tokio::test]
async fn import_aus_server_json() {
    let p = Pruefung::neu().await;
    let server_json = json!({
        "name": "io.github.jemand/wetterdienst",
        "title": "Wetterdienst",
        "description": "Wetterdaten über MCP",
        "version": "1.2.3",
        "packages": [{
            "registryType": "npm",
            "identifier": "@jemand/wetterdienst",
            "version": "1.2.3",
            "transport": { "type": "stdio" },
            "environmentVariables": [{
                "name": "WETTER_KEY",
                "description": "Schlüssel",
                "isRequired": true,
                "isSecret": true
            }]
        }]
    });

    let (status, rumpf) = p
        .admin("POST", "/admin/eintraege/import", Some(server_json))
        .await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let eintrag: Eintrag = als(&rumpf);
    assert_eq!(eintrag.id, EintragId::neu("wetterdienst"));
    match &eintrag.laufzeit {
        Laufzeit::Stdio {
            befehl, argumente, ..
        } => {
            assert_eq!(befehl, "npx");
            assert!(argumente.iter().any(|a| a == "@jemand/wetterdienst@1.2.3"));
        }
        andere => panic!("erwartet stdio, war {andere:?}"),
    }
    assert_eq!(eintrag.herkunft, Herkunft::Fremd);

    // Derselbe Eintrag noch einmal → 409.
    let (status, _) = p
        .admin(
            "POST",
            "/admin/eintraege/import",
            Some(json!({
                "name": "io.github.jemand/wetterdienst",
                "version": "2.0.0",
                "packages": [{
                    "registryType": "npm",
                    "identifier": "@jemand/wetterdienst",
                    "transport": { "type": "stdio" }
                }]
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Kaputtes server.json → 400 mit Fehler-Objekt.
    let (status, rumpf) = p
        .admin(
            "POST",
            "/admin/eintraege/import",
            Some(json!({ "version": "1" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(rumpf["fehler"].is_string());
}

#[tokio::test]
async fn admin_oberflaeche_liegt_unter_der_wurzel() {
    let p = Pruefung::mit_oberflaeche().await;
    for pfad in ["/", "/geraete", "/saetze/basis"] {
        let (status, rumpf) = p.roh(pfad).await;
        assert_eq!(status, StatusCode::OK, "{pfad}");
        assert!(rumpf.contains("<title>quiver</title>"), "{pfad}: {rumpf}");
    }
    // Die API bleibt die API: unbekannte Pfade dort sind 404, kein index.html.
    let (status, rumpf) = p.roh("/api/v1/gibtsnicht").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(rumpf.contains("fehler"));
}

// ------------------------------------------------------------------ SSE

#[tokio::test]
async fn ereignisse_melden_nur_was_dieses_geraet_betrifft() {
    let p = Pruefung::neu().await;
    let antwort = p.aufbau().await;

    let anfrage = Request::builder()
        .uri("/api/v1/geraet/ereignisse")
        .header("host", HOST)
        .header("authorization", format!("Bearer {}", antwort.token))
        .body(Body::empty())
        .expect("Anfrage");
    let antwort_strom = p.router.clone().oneshot(anfrage).await.expect("Antwort");
    assert_eq!(antwort_strom.status(), StatusCode::OK);
    assert_eq!(
        antwort_strom
            .headers()
            .get("content-type")
            .and_then(|w| w.to_str().ok()),
        Some("text/event-stream")
    );
    let mut strom = EreignisStrom::neu(antwort_strom.into_body());

    let (name, revision_a) = strom
        .naechstes(Duration::from_secs(5))
        .await
        .expect("sofort ein Ereignis mit der aktuellen Revision");
    assert_eq!(name, "soll");
    assert_eq!(revision_a, p.soll(&antwort.token).await.revision);

    // Satz ändern → neue Revision.
    assert_eq!(
        p.put_satz(&satz("basis", "wetter", json!({ "PFAD": "/tmp/anders" })))
            .await
            .0,
        StatusCode::OK
    );
    let (name, revision_b) = strom
        .naechstes(Duration::from_secs(5))
        .await
        .expect("Ereignis nach der Änderung");
    assert_eq!(name, "soll");
    assert_ne!(revision_a, revision_b);

    // Ein Satz, der keiner Gruppe dieses Geräts gehört, ändert nichts.
    assert_eq!(
        p.put_satz(&satz("anderer", "wetter", json!({ "PFAD": "/tmp/egal" })))
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        strom.naechstes(Duration::from_millis(800)).await.is_none(),
        "gleiche Revision, also kein Ereignis"
    );
}

#[tokio::test]
async fn abgeschalteter_eintrag_verlaesst_soll_und_meldet_sich() {
    let p = Pruefung::neu().await;
    let antwort = p.aufbau().await;
    assert_eq!(p.soll(&antwort.token).await.eintraege.len(), 1);

    // Ereignisstrom öffnen: zuerst kommt die aktuelle Revision.
    let anfrage = Request::builder()
        .uri("/api/v1/geraet/ereignisse")
        .header("host", HOST)
        .header("authorization", format!("Bearer {}", antwort.token))
        .body(Body::empty())
        .expect("Anfrage");
    let antwort_strom = p.router.clone().oneshot(anfrage).await.expect("Antwort");
    assert_eq!(antwort_strom.status(), StatusCode::OK);
    let mut strom = EreignisStrom::neu(antwort_strom.into_body());
    let (name, revision_an) = strom
        .naechstes(Duration::from_secs(5))
        .await
        .expect("sofort ein Ereignis mit der aktuellen Revision");
    assert_eq!(name, "soll");
    assert_eq!(revision_an, p.soll(&antwort.token).await.revision);

    // Abschalten: der Eintrag bleibt in Bibliothek und Satz.
    let mut aus = eintrag("wetter");
    aus.aktiv = false;
    assert_eq!(p.put_eintrag(&aus).await.0, StatusCode::OK);
    let (status, rumpf) = p.admin("GET", "/admin/eintraege/wetter", None).await;
    assert_eq!(status, StatusCode::OK);
    let gespeichert: Eintrag = als(&rumpf);
    assert!(!gespeichert.aktiv, "der Eintrag bleibt, nur abgeschaltet");
    let (_, satz_rumpf) = p.admin("GET", "/admin/saetze/basis", None).await;
    let satz: Satz = als(&satz_rumpf);
    assert_eq!(satz.eintraege.len(), 1, "der Satz verliert ihn nicht");

    // Das Soll des Geräts enthält ihn nicht mehr.
    let soll = p.soll(&antwort.token).await;
    assert!(
        soll.eintraege.is_empty(),
        "ein abgeschalteter Eintrag läuft auf keinem Gerät: {:?}",
        soll.eintraege
            .iter()
            .map(|e| &e.eintrag.id)
            .collect::<Vec<_>>()
    );

    // Und das Ereignis kommt.
    let (name, revision_aus) = strom
        .naechstes(Duration::from_secs(5))
        .await
        .expect("Ereignis nach dem Abschalten");
    assert_eq!(name, "soll");
    assert_ne!(revision_an, revision_aus, "neue Revision");
    assert_eq!(revision_aus, soll.revision);

    // Wieder einschalten: der Eintrag ist zurück.
    assert_eq!(p.put_eintrag(&eintrag("wetter")).await.0, StatusCode::OK);
    let soll = p.soll(&antwort.token).await;
    assert_eq!(soll.eintraege.len(), 1);
    assert_eq!(soll.eintraege[0].eintrag.id, EintragId::neu("wetter"));
    let (_, revision_wieder) = strom
        .naechstes(Duration::from_secs(5))
        .await
        .expect("Ereignis nach dem Einschalten");
    assert_eq!(revision_wieder, soll.revision);
}

/// Liest `ereignis`-Blöcke aus einem SSE-Rumpf.
struct EreignisStrom {
    koerper: Body,
    puffer: String,
}

impl EreignisStrom {
    fn neu(koerper: Body) -> Self {
        Self {
            koerper,
            puffer: String::new(),
        }
    }

    async fn naechstes(&mut self, dauer: Duration) -> Option<(String, String)> {
        let frist = tokio::time::Instant::now() + dauer;
        loop {
            if let Some(ereignis) = self.block_aus_puffer() {
                return Some(ereignis);
            }
            let rest = frist.saturating_duration_since(tokio::time::Instant::now());
            if rest.is_zero() {
                return None;
            }
            let rahmen = tokio::time::timeout(rest, self.koerper.frame())
                .await
                .ok()??;
            let rahmen = rahmen.ok()?;
            let daten = rahmen.into_data().ok()?;
            self.puffer.push_str(&String::from_utf8_lossy(&daten));
        }
    }

    fn block_aus_puffer(&mut self) -> Option<(String, String)> {
        loop {
            let ende = self.puffer.find("\n\n")?;
            let block = self.puffer[..ende].to_string();
            self.puffer = self.puffer[ende + 2..].to_string();
            let mut name = String::new();
            let mut daten = String::new();
            let mut kommentar = false;
            for zeile in block.lines() {
                if zeile.starts_with(':') {
                    kommentar = true;
                } else if let Some(wert) = zeile.strip_prefix("event:") {
                    name = wert.trim().to_string();
                } else if let Some(wert) = zeile.strip_prefix("data:") {
                    if !daten.is_empty() {
                        daten.push('\n');
                    }
                    daten.push_str(wert.trim_start());
                }
            }
            if kommentar || (name.is_empty() && daten.is_empty()) {
                continue;
            }
            return Some((name, daten));
        }
    }
}

// ------------------------------------------------------------- Kleinkram

#[test]
fn testdaten_sind_gueltig() {
    // Schützt die Helfer oben: was hier gebaut wird, muss der Vertrag schlucken.
    let e = eintrag("wetter");
    let wieder: Eintrag = serde_json::from_value(json!(e)).expect("Eintrag");
    assert_eq!(wieder.felder[0].quelle, Quelle::Server);

    let g = gruppe("team", &["basis"]);
    assert_eq!(g.saetze, vec![SatzId::neu("basis")]);

    let s = satz("basis", "wetter", json!({ "PFAD": "/tmp" }));
    assert_eq!(
        s.eintraege[0].werte.get("PFAD").map(String::as_str),
        Some("/tmp")
    );

    // Ein Gerät ohne Gruppen bekommt nichts (Regel aus `domain`).
    let leer = Geraet {
        id: GeraetId::neu("abc"),
        name: "leer".into(),
        gruppen: Vec::new(),
        gesperrt: false,
    };
    let soll = quiver_domain::soll_berechnen(&leer, &[], &[], &[]).expect("kein Fehler");
    assert!(soll.eintraege.is_empty());
    assert_eq!(soll.revision, Soll::neu(Vec::new()).revision);
}

/// **Vertrag (Aufgabe 19):** Ein Client, der einen Zustand meldet, den dieser
/// Server nicht kennt, wird nicht abgewiesen. Die Meldung wird gelesen und der
/// fremde Wert neutral abgelegt – die Verwaltung zeigt ihn als „unbekannt“.
#[tokio::test]
async fn ein_fremder_zustand_wird_nicht_abgelehnt() {
    let p = Pruefung::neu().await;
    let antwort = p.aufbau().await;
    let soll = p.soll(&antwort.token).await;

    // Von Hand, wie ein neuerer Client es schicken würde: `am-geraet-aus` kennt
    // der Vertrag, `von-morgen` nicht.
    let rumpf = json!({
        "revision": soll.revision,
        "client_version": "0.9.0",
        "eintraege": [
            ["wetter", { "zustand": "von-morgen", "was": 1 }],
            ["anderer", { "zustand": "am-geraet-aus" }],
        ],
    });
    let (status, _) = p
        .anfrage("POST", "/geraet/meldung", Some(&antwort.token), Some(rumpf))
        .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "ein unbekannter Zustand darf die Meldung nicht scheitern lassen"
    );

    let (_, geraete) = p.admin("GET", "/admin/geraete", None).await;
    let gemeldet = &geraete[0]["meldung"]["eintraege"];
    assert_eq!(
        gemeldet[0][1]["zustand"], "unbekannt",
        "der fremde Wert wird neutral abgelegt: {gemeldet}"
    );
    assert_eq!(
        gemeldet[1][1]["zustand"], "am-geraet-aus",
        "der neue Wert wird gelesen: {gemeldet}"
    );

    // Und andersherum: was der Server ausliefert, kann der Client lesen.
    let staende: Vec<GeraetStand> = als(&geraete);
    assert_eq!(
        staende[0].meldung.as_ref().expect("Meldung").eintraege[0].1,
        EintragZustand::Unbekannt
    );
    assert_eq!(
        staende[0].meldung.as_ref().expect("Meldung").eintraege[1].1,
        EintragZustand::AmGeraetAus
    );
}

// ------------------------------------------------- Grenzen und Prüfspur

/// Ein Eintrag, dessen Beschreibung `bytes` Zeichen lang ist.
fn grosser_eintrag(id: &str, bytes: usize) -> Eintrag {
    let mut eintrag = eintrag(id);
    eintrag.beschreibung = "x".repeat(bytes);
    eintrag
}

/// Ein Eintrag, dessen Rumpf groß ist und der **innerhalb** der Feldgrenzen
/// bleibt (Umgebung: 50 Einträge, je höchstens 4000 Zeichen).
fn grosser_eintrag_gueltig(id: &str, bytes: usize) -> Eintrag {
    let mut eintrag = eintrag(id);
    if let Laufzeit::Stdio { umgebung, .. } = &mut eintrag.laufzeit {
        let je = bytes.div_ceil(50).min(4000);
        for nummer in 0..50 {
            umgebung.insert(format!("GROSS_{nummer}"), "x".repeat(je));
        }
    }
    eintrag
}

/// Ein `server.json`, wie es die MCP-Registry liefert.
fn server_json(name: &str) -> Value {
    json!({
        "name": name,
        "title": "Wetterdienst",
        "description": "Wetterdaten über MCP",
        "version": "1.2.3",
        "packages": [{
            "registryType": "npm",
            "identifier": "@jemand/wetterdienst",
            "version": "1.2.3",
            "transport": { "type": "stdio" }
        }]
    })
}

/// Eine Zeile in `benutzer`, damit die Benutzer-Routen der Verwaltung etwas
/// zum Ändern haben. Ohne Entra entsteht kein Benutzer; der Test legt ihn
/// deshalb in der Datenbank an (nur Testdaten, keine echten Konten).
async fn benutzer_anlegen(p: &Pruefung, oid: &str) {
    let pfad = p.ordner.join("quiver.db");
    let conn = rusqlite::Connection::open(&pfad).expect("Datenbank");
    conn.execute(
        "INSERT OR REPLACE INTO benutzer
            (oid, tid, email, name, rolle, entra_json, token_stand, erstellt)
         VALUES (?1, 'tenant', 'test@example.test', 'Test Person', 'benutzer', '[]', 'gueltig', '2026-01-01T00:00:00Z')",
        rusqlite::params![oid],
    )
    .expect("Benutzer angelegt");
}

/// Ein Verwaltungsaufruf mit vollem Pfad (die Tabelle nennt ihn vollständig).
async fn admin_voll(
    p: &Pruefung,
    methode: &str,
    pfad: &str,
    rumpf: Option<Value>,
) -> (StatusCode, Value) {
    let koerper = match rumpf {
        Some(wert) => Body::from(serde_json::to_vec(&wert).expect("JSON")),
        None => Body::empty(),
    };
    let antwort = p
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(methode)
                .uri(pfad)
                .header("host", HOST)
                .header("content-type", "application/json")
                .header("cookie", &p.cookie)
                .header("origin", OEFFENTLICH)
                .header("x-quiver", "1")
                .body(koerper)
                .expect("Anfrage"),
        )
        .await
        .expect("Antwort");
    auspacken(antwort).await
}

#[tokio::test]
async fn rumpfgrenze_je_route() {
    let p = Pruefung::neu().await;

    // Standard: 64 KB. Ein größerer Rumpf auf einer gewöhnlichen Route → 413.
    let zu_gross =
        json!({ "gruppen": [], "minuten": 5, "unbegrenzt": false, "x": "y".repeat(70 * 1024) });
    let (status, rumpf) = p.admin("POST", "/admin/codes", Some(zu_gross)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{rumpf}");

    // Auch der Weg mit eigener Rumpfbehandlung (`anmelden`) antwortet so.
    let zu_gross = json!({ "code": "ABCD-EFGH", "name": "x".repeat(70 * 1024) });
    let (status, rumpf) = p.anfrage("POST", "/anmelden", None, Some(zu_gross)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{rumpf}");

    // Einträge dürfen 256 KB: 100 KB gehen durch (innerhalb der Feldgrenzen),
    // 300 KB nicht.
    let gross = grosser_eintrag_gueltig("gross", 100 * 1024);
    assert_eq!(
        p.put_eintrag(&gross).await.0,
        StatusCode::OK,
        "100 KB sind für einen Eintrag in Ordnung"
    );
    let zu_gross = grosser_eintrag("zu-gross", 300 * 1024);
    let (status, rumpf) = p.put_eintrag(&zu_gross).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{rumpf}");
}

#[tokio::test]
async fn ereignis_stroeme_sind_begrenzt() {
    let p = Pruefung::neu().await;
    let geraet = p.aufbau().await;

    let mut offen = Vec::new();
    for nummer in 0..quiver_server::STROEME_JE_GERAET {
        let anfrage = Request::builder()
            .uri("/api/v1/geraet/ereignisse")
            .header("host", HOST)
            .header("authorization", format!("Bearer {}", geraet.token))
            .body(Body::empty())
            .expect("Anfrage");
        let antwort = p.router.clone().oneshot(anfrage).await.expect("Antwort");
        assert_eq!(antwort.status(), StatusCode::OK, "Strom {nummer}");
        offen.push(antwort);
    }

    // Einer mehr: 429 statt eines weiteren offenen Stroms.
    let anfrage = Request::builder()
        .uri("/api/v1/geraet/ereignisse")
        .header("host", HOST)
        .header("authorization", format!("Bearer {}", geraet.token))
        .body(Body::empty())
        .expect("Anfrage");
    let antwort = p.router.clone().oneshot(anfrage).await.expect("Antwort");
    assert_eq!(antwort.status(), StatusCode::TOO_MANY_REQUESTS);

    // Wird einer geschlossen, ist wieder Platz.
    offen.pop();
    let anfrage = Request::builder()
        .uri("/api/v1/geraet/ereignisse")
        .header("host", HOST)
        .header("authorization", format!("Bearer {}", geraet.token))
        .body(Body::empty())
        .expect("Anfrage");
    let antwort = p.router.clone().oneshot(anfrage).await.expect("Antwort");
    assert_eq!(antwort.status(), StatusCode::OK);
}

#[tokio::test]
async fn jede_aendernde_verwaltungsroute_schreibt_in_die_pruefspur() {
    let p = Pruefung::neu().await;
    let geraet = p.aufbau().await;
    const BENUTZER: &str = "99999999-8888-7777-6666-555555555555";
    benutzer_anlegen(&p, BENUTZER).await;

    for (methode, pfad, aktion) in quiver_server::spur::AENDERNDE_ROUTEN {
        // Jede Route wird so aufgerufen, wie sie im Betrieb aufgerufen wird.
        let status = match (*methode, *pfad) {
            ("POST", "/api/v1/admin/eintraege/import") => {
                admin_voll(
                    &p,
                    methode,
                    pfad,
                    Some(server_json("io.github.jemand/wetterdienst")),
                )
                .await
                .0
            }
            ("PUT", "/api/v1/admin/eintraege/{id}") => {
                admin_voll(
                    &p,
                    methode,
                    "/api/v1/admin/eintraege/weg",
                    Some(json!(eintrag("weg"))),
                )
                .await
                .0
            }
            ("DELETE", "/api/v1/admin/eintraege/{id}") => {
                admin_voll(&p, methode, "/api/v1/admin/eintraege/weg", None)
                    .await
                    .0
            }
            ("PUT", "/api/v1/admin/saetze/{id}") => {
                admin_voll(
                    &p,
                    methode,
                    "/api/v1/admin/saetze/weg",
                    Some(json!(satz("weg", "wetter", json!({ "PFAD": "/tmp" })))),
                )
                .await
                .0
            }
            ("DELETE", "/api/v1/admin/saetze/{id}") => {
                admin_voll(&p, methode, "/api/v1/admin/saetze/weg", None)
                    .await
                    .0
            }
            ("PUT", "/api/v1/admin/gruppen/{id}") => {
                admin_voll(
                    &p,
                    methode,
                    "/api/v1/admin/gruppen/weg",
                    Some(json!(gruppe("weg", &["basis"]))),
                )
                .await
                .0
            }
            ("DELETE", "/api/v1/admin/gruppen/{id}") => {
                admin_voll(&p, methode, "/api/v1/admin/gruppen/weg", None)
                    .await
                    .0
            }
            ("PUT", "/api/v1/admin/geraete/{id}") => {
                admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/geraete/{}", geraet.geraet.id),
                    Some(json!(GeraetAendern {
                        name: "Anderer Name".into(),
                        gruppen: Vec::new(),
                        gesperrt: false,
                    })),
                )
                .await
                .0
            }
            ("DELETE", "/api/v1/admin/geraete/{id}") => {
                admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/geraete/{}", geraet.geraet.id),
                    None,
                )
                .await
                .0
            }
            ("POST", "/api/v1/admin/codes") => {
                admin_voll(
                    &p,
                    methode,
                    pfad,
                    Some(json!(CodeAnlegen {
                        gruppen: Vec::new(),
                        minuten: Some(5),
                        unbegrenzt: false,
                    })),
                )
                .await
                .0
            }
            ("PUT", "/api/v1/admin/benutzer/{oid}") => {
                admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/benutzer/{BENUTZER}"),
                    Some(json!({ "rolle": "admin", "gesperrt": false })),
                )
                .await
                .0
            }
            ("DELETE", "/api/v1/admin/benutzer/{oid}") => {
                admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/benutzer/{BENUTZER}"),
                    None,
                )
                .await
                .0
            }
            // Aufgabe 33: dieselbe Zugehörigkeit von beiden Seiten. Die Gruppe
            // wird vorher angelegt, damit es etwas zuzuordnen gibt.
            ("PUT", "/api/v1/admin/benutzer/{oid}/gruppen/{gruppe}") => {
                // Der Benutzer wurde oben schon gelöscht; die Route braucht
                // einen, den es gibt.
                benutzer_anlegen(&p, BENUTZER).await;
                admin_voll(
                    &p,
                    "PUT",
                    "/api/v1/admin/gruppen/zuordnung",
                    Some(json!(gruppe("zuordnung", &[]))),
                )
                .await;
                admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/benutzer/{BENUTZER}/gruppen/zuordnung"),
                    Some(json!({ "vorhanden": true })),
                )
                .await
                .0
            }
            ("PUT", "/api/v1/admin/gruppen/{id}/mitglieder/{oid}") => {
                admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/gruppen/zuordnung/mitglieder/{BENUTZER}"),
                    Some(json!({ "vorhanden": true })),
                )
                .await
                .0
            }
            // Ohne Entra gibt es nichts nachzuprüfen und keine gesehene
            // Entra-Gruppe; beide Routen antworten 404 und schreiben nichts.
            // Der erfolgreiche Weg steht in `tests/entra.rs`
            // (`nachpruefen_und_aufnehmen_schreiben_in_die_pruefspur`).
            ("POST", "/api/v1/admin/benutzer/{oid}/pruefen") => {
                let (status, _) = admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/benutzer/{BENUTZER}/pruefen"),
                    None,
                )
                .await;
                assert_eq!(status, StatusCode::NOT_FOUND, "{methode} {pfad}");
                continue;
            }
            ("PUT", "/api/v1/admin/entra-gruppen/{id}") => {
                const GRUPPE: &str = "12345678-1234-1234-1234-123456789012";
                let (status, _) = admin_voll(
                    &p,
                    methode,
                    &format!("/api/v1/admin/entra-gruppen/{GRUPPE}"),
                    Some(json!({ "aufnehmen": true })),
                )
                .await;
                assert_eq!(status, StatusCode::NOT_FOUND, "{methode} {pfad}");
                continue;
            }
            (andere, pfad) => panic!("kein Aufruf für {andere} {pfad} hinterlegt"),
        };
        assert!(
            status.is_success(),
            "{methode} {pfad} muss gelingen, war {status}"
        );

        // Und der Eintrag steht in der Prüfspur: wer, was, Ziel.
        let (status, rumpf) = p.admin("GET", "/admin/pruefspur?limit=200", None).await;
        assert_eq!(status, StatusCode::OK);
        let eintraege: Vec<PruefspurEintrag> = als(&rumpf);
        let treffer = eintraege.iter().any(|eintrag| {
            eintrag.art == "verwaltung"
                && eintrag.text.contains(methode)
                && eintrag.text.contains(pfad)
                && eintrag.text.contains(aktion)
        });
        assert!(
            treffer,
            "kein Eintrag für {methode} {pfad} ({aktion}) in der Prüfspur: {:?}",
            eintraege
                .iter()
                .filter(|e| e.art == "verwaltung")
                .map(|e| e.text.clone())
                .collect::<Vec<_>>()
        );
        let akteur = eintraege
            .iter()
            .find(|eintrag| eintrag.art == "verwaltung" && eintrag.text.contains(pfad))
            .map(|eintrag| eintrag.akteur.clone())
            .expect("Akteur");
        assert_eq!(akteur, ROOT_EMAIL, "wer es war, steht dabei");
    }

    // Und die Spur enthält keine Werte: kein Titel, kein Passwort, kein Token.
    let (_, rumpf) = p.admin("GET", "/admin/pruefspur?limit=200", None).await;
    let texte = als::<Vec<PruefspurEintrag>>(&rumpf)
        .into_iter()
        .filter(|eintrag| eintrag.art == "verwaltung")
        .map(|eintrag| eintrag.text)
        .collect::<Vec<_>>()
        .join("\n");
    for verboten in ["Titel weg", "Test Person", "ein-sehr-langes", "Bearer"] {
        assert!(!texte.contains(verboten), "{verboten} steht in der Spur");
    }
}

/// Aufgabe 33: Eine gelöschte Gruppe nimmt ihre Zuordnungen mit – nichts
/// bleibt zurück, was beim nächsten Anlegen derselben Kennung falsch wirkte.
#[tokio::test]
async fn geloeschte_gruppe_nimmt_ihre_zuordnungen_mit() {
    let p = Pruefung::neu().await;
    const BENUTZER: &str = "99999999-8888-7777-6666-555555555555";
    benutzer_anlegen(&p, BENUTZER).await;
    assert_eq!(p.put_gruppe(&gruppe("hand", &[])).await.0, StatusCode::OK);

    // Von Hand zuordnen und wiederfinden.
    let (status, rumpf) = admin_voll(
        &p,
        "PUT",
        &format!("/api/v1/admin/benutzer/{BENUTZER}/gruppen/hand"),
        Some(json!({ "vorhanden": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let (status, rumpf) = p
        .admin("GET", &format!("/admin/benutzer/{BENUTZER}/gruppen"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let gruppen: Vec<quiver_protocol::BenutzerGruppe> = als(&rumpf);
    assert_eq!(gruppen.len(), 1);
    assert!(gruppen[0].von_hand);

    // Gruppe löschen: die Zuordnung verschwindet.
    let (status, _) = p.admin("DELETE", "/admin/gruppen/hand", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, rumpf) = p
        .admin("GET", &format!("/admin/benutzer/{BENUTZER}/gruppen"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let gruppen: Vec<quiver_protocol::BenutzerGruppe> = als(&rumpf);
    assert!(
        gruppen.is_empty(),
        "die Zuordnung wirkt nicht mehr: {gruppen:?}"
    );

    // Die Kennung neu anlegen: die alte Zuordnung kommt nicht zurück.
    assert_eq!(p.put_gruppe(&gruppe("hand", &[])).await.0, StatusCode::OK);
    let (_, rumpf) = p
        .admin("GET", &format!("/admin/benutzer/{BENUTZER}/gruppen"), None)
        .await;
    let gruppen: Vec<quiver_protocol::BenutzerGruppe> = als(&rumpf);
    assert!(gruppen.is_empty(), "nichts wirkt wieder von selbst");
}

// ------------------------------------ Härtung: gleichzeitiges Bearbeiten (12)

#[tokio::test]
async fn veralteter_stand_gibt_409_mit_hinweis() {
    let p = Pruefung::neu().await;
    let eintrag = eintrag("wetter");
    assert_eq!(p.put_eintrag(&eintrag).await.0, StatusCode::OK);

    // Der Stand, den dieser Aufrufer geladen hat.
    let (status, _, koepfe) = p
        .admin_mit_kopf("GET", "/admin/eintraege/wetter", ("accept", "*/*"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let etag = koepfe
        .get(header::ETAG)
        .expect("GET liefert ein ETag")
        .to_str()
        .expect("ETag ist Text")
        .to_string();

    // Jemand anders ändert den Eintrag.
    let mut anders = eintrag.clone();
    anders.titel = String::from("Wetter (neu)");
    assert_eq!(p.put_eintrag(&anders).await.0, StatusCode::OK);

    // Mit dem alten Stand speichern: 409 und ein Hinweis zum Neuladen.
    let (status, rumpf, _) = p
        .admin_mit_kopf(
            "PUT",
            "/admin/eintraege/wetter",
            ("if-match", &etag),
            Some(json!(&eintrag)),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{rumpf}");
    assert!(
        rumpf["fehler"]
            .as_str()
            .unwrap_or_default()
            .contains("neu laden"),
        "{rumpf}"
    );

    // Mit dem aktuellen Stand geht es.
    let (status, _, koepfe) = p
        .admin_mit_kopf("GET", "/admin/eintraege/wetter", ("accept", "*/*"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let etag = koepfe
        .get(header::ETAG)
        .expect("ETag")
        .to_str()
        .expect("Text")
        .to_string();
    let (status, rumpf, _) = p
        .admin_mit_kopf(
            "PUT",
            "/admin/eintraege/wetter",
            ("if-match", &etag),
            Some(json!(&eintrag)),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
}

#[tokio::test]
async fn gerat_und_satz_und_gruppe_pruefen_if_match() {
    let p = Pruefung::neu().await;
    assert_eq!(p.put_eintrag(&eintrag("wetter")).await.0, StatusCode::OK);
    assert_eq!(
        p.put_satz(&satz("basis", "wetter", json!({}))).await.0,
        StatusCode::OK
    );
    assert_eq!(
        p.put_gruppe(&gruppe("team", &["basis"])).await.0,
        StatusCode::OK
    );
    // Ein falscher Stand trifft alle drei.
    for (pfad, rumpf) in [
        (
            "/admin/saetze/basis",
            json!(satz("basis", "wetter", json!({ "PFAD": "/tmp" }))),
        ),
        ("/admin/gruppen/team", json!(gruppe("team", &["basis"]))),
    ] {
        let (status, antwort, _) = p
            .admin_mit_kopf("PUT", pfad, ("if-match", "\"veraltet\""), Some(rumpf))
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{pfad}: {antwort}");
    }
    // Ein Gerät entsteht erst mit einer Anmeldung; dafür genügt der Satz.
    let geraet = p.aufbau().await;
    let (status, antwort, _) = p
        .admin_mit_kopf(
            "PUT",
            &format!("/admin/geraete/{}", geraet.geraet.id),
            ("if-match", "\"veraltet\""),
            Some(json!({
                "name": "Testrechner",
                "gruppen": geraet.geraet.gruppen.iter().map(|g| g.als_str()).collect::<Vec<_>>(),
                "gesperrt": false
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{antwort}");
}

// ----------------------------------------------- Härtung: Grenzen (13)

#[tokio::test]
async fn zu_grosse_texte_und_zu_viele_felder_sind_422() {
    let p = Pruefung::neu().await;

    let mut lang = eintrag("wetter");
    lang.titel = "T".repeat(121);
    let (status, rumpf) = p.put_eintrag(&lang).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{rumpf}");

    let mut lang = eintrag("wetter");
    lang.beschreibung = "B".repeat(1001);
    let (status, _) = p.put_eintrag(&lang).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let mut lang = eintrag("wetter");
    lang.id = EintragId::neu("x".repeat(81));
    let (status, _) = p.put_eintrag(&lang).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Gruppen-Name 80.
    let mut gruppe_lang = gruppe("team", &[]);
    gruppe_lang.titel = "G".repeat(121);
    let (status, _) = p.put_gruppe(&gruppe_lang).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn eine_meldung_wird_gekuerzt_und_nicht_abgelehnt() {
    let p = Pruefung::neu().await;
    let geraet = p.aufbau().await;
    let eintraege: Vec<Value> = (0..210)
        .map(|i| {
            let text = if i == 0 {
                "x".repeat(600)
            } else {
                String::from("kurz")
            };
            json!([
                format!("e{i}"),
                { "zustand": "fehler", "text": text }
            ])
        })
        .collect();
    let (status, rumpf) = p
        .anfrage(
            "POST",
            "/geraet/meldung",
            Some(&geraet.token),
            Some(json!({
                "revision": geraet.geraet.id.als_str(),
                "client_version": "v".repeat(200),
                "eintraege": eintraege
            })),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{rumpf}");

    // Gespeichert sind höchstens 200 Einträge, der Text ist gekürzt, die
    // Client-Fassung auf 64 Zeichen.
    let (status, liste) = p.admin("GET", "/admin/geraete", None).await;
    assert_eq!(status, StatusCode::OK);
    let meldung = &liste[0]["meldung"];
    assert_eq!(meldung["eintraege"].as_array().expect("Liste").len(), 200);
    assert_eq!(
        meldung["client_version"]
            .as_str()
            .expect("Text")
            .chars()
            .count(),
        64
    );
    let text = meldung["eintraege"][0][1]["text"].as_str().expect("Text");
    assert_eq!(text.chars().count(), 501, "gekürzt mit …");
}

// ------------------------------------ Härtung: unbekannter Satz (15)

#[tokio::test]
async fn gruppe_mit_unbekanntem_satz_ist_422_mit_namen() {
    let p = Pruefung::neu().await;
    let (status, rumpf) = p.put_gruppe(&gruppe("team", &["gibtsnicht"])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{rumpf}");
    assert!(
        rumpf["fehler"]
            .as_str()
            .unwrap_or_default()
            .contains("gibtsnicht"),
        "die Namen stehen in der Antwort: {rumpf}"
    );
}

// --------------------------------------------- Programme (Aufgabe 37)

impl Pruefung {
    /// Ein Programm in die Ablage legen: `<daten>/tools/<system>/<arch>/`.
    fn programm_ablegen(&self, name: &str, system: &str, arch: &str, inhalt: &[u8]) -> String {
        let ordner = self.ordner.join("tools").join(system).join(arch);
        std::fs::create_dir_all(&ordner).expect("Ordner");
        std::fs::write(ordner.join(name), inhalt).expect("Programm");
        hex::encode(<sha2::Sha256 as sha2::Digest>::digest(inhalt))
    }

    /// Ein Gerät, dessen Eintrag ein verteiltes Programm startet.
    async fn aufbau_verteilt(&self, befehl: &str) -> AnmeldenAntwort {
        let eintrag = eintrag_mit("devops", befehl);
        assert_eq!(self.put_eintrag(&eintrag).await.0, StatusCode::OK);
        assert_eq!(
            self.put_satz(&satz("basis", "devops", json!({}))).await.0,
            StatusCode::OK
        );
        assert_eq!(
            self.put_gruppe(&gruppe("team", &["basis"])).await.0,
            StatusCode::OK
        );
        let code = self.code_anlegen(&["team"], Some(60)).await;
        self.anmelden(&code.code, "Testrechner").await
    }

    /// Ein Abruf, der den Rumpf als Bytes zurückgibt (Programme).
    async fn roh_bytes(
        &self,
        pfad: &str,
        token: Option<&str>,
        extra: Option<(&str, &str)>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut bauer = Request::builder()
            .method("GET")
            .uri(format!("/api/v1{pfad}"))
            .header("host", HOST);
        if let Some(token) = token {
            bauer = bauer.header("authorization", format!("Bearer {token}"));
        }
        if let Some((name, wert)) = extra {
            bauer = bauer.header(name, wert);
        }
        let antwort = self
            .router
            .clone()
            .oneshot(bauer.body(Body::empty()).expect("Anfrage"))
            .await
            .expect("Antwort");
        let status = antwort.status();
        let headers = antwort.headers().clone();
        let bytes = antwort
            .into_body()
            .collect()
            .await
            .expect("Rumpf")
            .to_bytes()
            .to_vec();
        (status, headers, bytes)
    }

    async fn soll_mit_system(&self, token: &str, system: &str, arch: &str) -> (StatusCode, Value) {
        self.anfrage(
            "GET",
            &format!("/geraet/soll?system={system}&arch={arch}"),
            Some(token),
            None,
        )
        .await
    }
}

/// Das Soll nennt Programm, Größe und Prüfsumme – aber nur für das System, das
/// das Gerät meldet; ohne Angabe bleibt die Liste leer (ältere Clients).
#[tokio::test]
async fn das_soll_nennt_das_programm_fuer_das_system() {
    let p = Pruefung::neu().await;
    let inhalt = b"programm-v1";
    let sha = p.programm_ablegen("quiver-devops", "linux", "x64", inhalt);
    let geraet = p.aufbau_verteilt("quiver-devops").await;

    let (status, rumpf) = p.soll_mit_system(&geraet.token, "linux", "x64").await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let soll: Soll = als(&rumpf);
    let programm = soll.programm("quiver-devops").expect("Programm im Soll");
    assert_eq!(programm.sha256, sha);
    assert_eq!(programm.groesse, inhalt.len() as u64);
    assert_eq!(programm.system, "linux");
    assert_eq!(programm.arch, "x64");

    // Ohne System: keine Programme (ein älterer Client sucht selbst).
    let (status, rumpf) = p
        .anfrage("GET", "/geraet/soll", Some(&geraet.token), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let ohne: Soll = als(&rumpf);
    assert!(ohne.programme.is_empty());

    // Ein unbekanntes System ist ein Fehler, kein Rateversuch.
    let (status, _) = p.soll_mit_system(&geraet.token, "plan9", "x64").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Die Revision ändert sich mit dem Programm.
    let vorher = soll.revision;
    std::thread::sleep(Duration::from_millis(20));
    p.programm_ablegen("quiver-devops", "linux", "x64", b"programm-v2");
    let (_, rumpf) = p.soll_mit_system(&geraet.token, "linux", "x64").await;
    let nachher: Soll = als(&rumpf);
    assert_ne!(vorher, nachher.revision, "neue Fassung, neue Revision");
}

/// Das Programm kommt gestreamt, mit `ETag`, und nur für ein zugewiesenes
/// Gerät. Ein Entzug wirkt sofort.
#[tokio::test]
async fn das_programm_kommt_nur_fuer_dieses_geraet() {
    let p = Pruefung::neu().await;
    let inhalt = b"#!programm\n";
    let sha = p.programm_ablegen("quiver-devops", "linux", "x64", inhalt);
    let geraet = p.aufbau_verteilt("quiver-devops").await;
    let pfad = "/geraet/tools/quiver-devops?system=linux&arch=x64";

    // Ohne Token: nichts.
    let (status, _, _) = p.roh_bytes(pfad, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Mit Token: die Bytes, die Prüfsumme und ein ETag.
    let (status, kopf, bytes) = p.roh_bytes(pfad, Some(&geraet.token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, inhalt);
    let etag = kopf
        .get(header::ETAG)
        .and_then(|wert| wert.to_str().ok())
        .expect("ETag")
        .to_string();
    assert!(etag.contains(&sha), "{etag}");
    assert_eq!(
        kopf.get(header::CONTENT_LENGTH)
            .and_then(|wert| wert.to_str().ok()),
        Some(inhalt.len().to_string().as_str())
    );

    // Dasselbe noch einmal: 304, kein Rumpf.
    let (status, _, bytes) = p
        .roh_bytes(pfad, Some(&geraet.token), Some(("if-none-match", &etag)))
        .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(bytes.is_empty());

    // Ein Name, den der Server nicht verteilt.
    let (status, _, _) = p
        .roh_bytes(
            "/geraet/tools/npx?system=linux&arch=x64",
            Some(&geraet.token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Ein verteilter Name, der nicht im Soll steht: 403.
    let (status, _, _) = p
        .roh_bytes(
            "/geraet/tools/quiver-nix?system=linux&arch=x64",
            Some(&geraet.token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Für ein anderes System gibt es das Programm nicht: 404, kein Fehler.
    let (status, _, _) = p
        .roh_bytes(
            "/geraet/tools/quiver-devops?system=macos&arch=arm64",
            Some(&geraet.token),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Fehlt die Angabe, wird nichts geraten.
    let (status, _, _) = p
        .roh_bytes("/geraet/tools/quiver-devops", Some(&geraet.token), None)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Entzug: der Satz fällt weg, das Gerät bekommt sofort nichts mehr.
    assert_eq!(
        p.put_satz(&satz("basis", "devops", json!({}))).await.0,
        StatusCode::OK
    );
    assert_eq!(p.put_gruppe(&gruppe("team", &[])).await.0, StatusCode::OK);
    let (status, _, _) = p.roh_bytes(pfad, Some(&geraet.token), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Entzug wirkt sofort");
}

/// Ein fremdes Gerät bekommt kein Programm: es steht in keinem Soll.
#[tokio::test]
async fn ein_fremdes_geraet_bekommt_kein_programm() {
    let p = Pruefung::neu().await;
    p.programm_ablegen("quiver-devops", "linux", "x64", b"geheim");
    let eigenes = p.aufbau_verteilt("quiver-devops").await;
    // Ein zweites Gerät in einer Gruppe ohne den Satz.
    assert_eq!(p.put_gruppe(&gruppe("leer", &[])).await.0, StatusCode::OK);
    let code = p.code_anlegen(&["leer"], Some(60)).await;
    let fremd = p.anmelden(&code.code, "Anderer Rechner").await;

    let pfad = "/geraet/tools/quiver-devops?system=linux&arch=x64";
    let (status, _, _) = p.roh_bytes(pfad, Some(&eigenes.token), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = p.roh_bytes(pfad, Some(&fremd.token), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// Die Drossel zählt je Programm: erfundene Namen verbrauchen nichts, und
/// wiederholtes Laden ist möglich, aber nicht endlos.
#[tokio::test]
async fn die_drossel_zaehlt_je_zugewiesenem_programm() {
    let p = Pruefung::neu().await;
    p.programm_ablegen("quiver-devops", "linux", "x64", b"inhalt");
    let geraet = p.aufbau_verteilt("quiver-devops").await;
    let abruf = |name: &str| {
        let pfad = format!("/geraet/tools/{name}?system=linux&arch=x64");
        let token = geraet.token.clone();
        let p = &p;
        async move { p.roh_bytes(&pfad, Some(&token), None).await.0 }
    };
    for _ in 0..35 {
        assert_eq!(abruf("quiver-nix").await, StatusCode::FORBIDDEN);
    }
    for _ in 0..30 {
        assert_eq!(abruf("quiver-devops").await, StatusCode::OK);
    }
    assert_eq!(abruf("quiver-devops").await, StatusCode::TOO_MANY_REQUESTS);
}

/// Die Verwaltung sieht, welche Programme der Server bereithält (Aufgabe 37).
#[tokio::test]
async fn die_verwaltung_sieht_die_programme() {
    let p = Pruefung::neu().await;
    let sha = p.programm_ablegen("quiver-devops", "linux", "x64", b"hallo");
    let (status, rumpf) = p.admin("GET", "/admin/programme", None).await;
    assert_eq!(status, StatusCode::OK, "{rumpf}");
    let liste: Vec<Value> = als(&rumpf);
    assert_eq!(liste.len(), 1, "{liste:?}");
    assert_eq!(liste[0]["name"], "quiver-devops");
    assert_eq!(liste[0]["sha256"], sha.as_str());
    assert_eq!(liste[0]["system"], "linux");
    assert_eq!(liste[0]["arch"], "x64");
    // Ändern lässt sich hier nichts: kein PUT, kein DELETE.
    let (status, _) = p.admin("PUT", "/admin/programme", Some(json!([]))).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}
