// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Abnahmetests für den Server: gegen [`quiver_server::app`] mit
//! `tower::ServiceExt::oneshot` und einem temporären Datenordner.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use quiver_domain::{
    Eintrag, EintragId, Feld, FeldArt, FeldOption, Geraet, GeraetId, Gruppe, GruppeId, Herkunft,
    Laufzeit, Quelle, Satz, SatzEintrag, SatzId, Soll, Werte,
};
use quiver_protocol::{
    Anmelden, AnmeldenAntwort, Code, CodeAnlegen, EintragZustand, GeraetAendern, GeraetStand,
    Meldung, Ueberblick,
};
use quiver_server::{app, benutzer, Konfig};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tower::ServiceExt;

const ROOT_EMAIL: &str = "root@example.test";
const ROOT_PASSWORT: &str = "ein-sehr-langes-testpasswort";
const OEFFENTLICH: &str = "http://127.0.0.1:8740";
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

    // Satz auf einen unbekannten Eintrag → 400.
    let (status, rumpf) = p
        .put_satz(&satz("kaputt", "gibtsnicht", json!({ "PFAD": "/tmp" })))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rumpf}");

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
