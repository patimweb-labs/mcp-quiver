// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Grenzen und Härtung des Clients (Paket 3).
//!
//! Geprüft wird das **Verhalten**, nicht die Angriffsart:
//!
//! 1. Eine Frist je Aufruf beendet einen hängenden Aufruf; Fortschritts-
//!    meldungen verlängern sie; ein Abbruch des Agenten beendet den Aufruf.
//! 2. Zu große Antworten und zu viele gleichzeitige Aufrufe haben eine klare
//!    Meldung.
//! 3. Eine Meldung des Tools über 16 MB startet den Eintrag neu.
//! 4. Ein kaputter Tresor verhindert den Start nicht; erst „Neu beginnen“
//!    schreibt wieder, die alten Dateien bleiben.
//! 5. Ein kaputter gespeicherter Stand wird beiseitegelegt, ein zweiter Kern
//!    auf demselben Profil startet nicht.
//! 6. Ein Eintrag mit unbrauchbarer Kennung startet nicht und ist benannt.
//! 7. Ein Agent sieht nur eigene Aufrufe und nur Logs von Einträgen, die er
//!    selbst benutzt hat.
//! 9. `einbinden` prüft die Server-Adresse.
//! 10. und 15. Antwortgrößen und Herkunft (`Origin`/`Host`) am Endpunkt.
//! 13. Eine laufende Einrichtung lässt sich abbrechen.
//! 18. Ein Werkzeug erbt nicht die ganze Umgebung.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use quiver_client::protokoll::{Filter, LogFilter};
use quiver_client::{DateiTresor, Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{
    Einrichtung, Eintrag, EintragId, Gruppe, GruppeId, Herkunft, Laufzeit, Satz, SatzEintrag,
    SatzId, Werte,
};
use quiver_protocol::EintragZustand;
use quiver_server::{app, Konfig};
use rmcp::handler::client::ClientHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, ClientRequest,
    Implementation,
};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{Peer, RoleClient, ServiceExt};
use serde_json::{json, Value};

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// Wie lange auf einen Zustand gewartet wird.
const FRIST: Duration = Duration::from_secs(15);

// ------------------------------------------------------------------ Gerüst

struct Pruefung {
    ordner: PathBuf,
    /// `http://127.0.0.1:<port>`
    basis: String,
    cookie: String,
    kern: Kern,
    http: reqwest::Client,
    /// Wohin der Demo-Server jede Zeile schreibt.
    protokoll: PathBuf,
}

impl Pruefung {
    async fn neu() -> Self {
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "quiver-client-haertung-{}-{nummer}-{stempel}",
            std::process::id()
        ));
        let daten = ordner.join("server");
        std::fs::create_dir_all(&daten).expect("Ordner");

        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let adresse = listener.local_addr().expect("Adresse");
        let basis = format!("http://{adresse}");
        let http = reqwest::Client::new();
        let mut konfig = Konfig::neu(&daten);
        konfig.oeffentlich = basis.clone();
        let router = app(konfig).await.expect("App baut");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let cookie = gemeinsam::sitzung(&http, &basis, &daten).await;
        let kern = Kern::starten(konfig_fuer(&ordner))
            .await
            .expect("Kern startet");

        Self {
            protokoll: ordner.join("protokoll.log"),
            ordner,
            basis,
            cookie,
            kern,
            http,
        }
    }

    fn endpunkt(&self) -> String {
        self.kern.modell().borrow().endpunkt.clone()
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

    async fn put(&self, pfad: &str, wert: &impl serde::Serialize) {
        self.admin("PUT", pfad, Some(json!(wert))).await;
    }

    /// Einträge und einen Satz anlegen, eine Gruppe zuordnen, einbinden.
    async fn zuweisen(&self, eintraege: Vec<Eintrag>, werte: Werte) {
        for eintrag in &eintraege {
            self.put(
                &format!("/admin/eintraege/{}", eintrag.id.als_str()),
                eintrag,
            )
            .await;
        }
        self.put(
            "/admin/saetze/basis",
            &Satz {
                id: SatzId::neu("basis"),
                titel: "Basis".into(),
                prioritaet: 0,
                eintraege: eintraege
                    .iter()
                    .map(|eintrag| SatzEintrag {
                        eintrag: eintrag.id.clone(),
                        werte: werte.clone(),
                        fest: Default::default(),
                    })
                    .collect(),
            },
        )
        .await;
        self.put(
            "/admin/gruppen/team",
            &Gruppe {
                id: GruppeId::neu("team"),
                titel: "Team".into(),
                saetze: vec![SatzId::neu("basis")],
                entra: Vec::new(),
            },
        )
        .await;
        let code = self
            .admin(
                "POST",
                "/admin/codes",
                Some(json!({ "gruppen": ["team"], "minuten": 60 })),
            )
            .await;
        let code = code["code"].as_str().expect("Code").to_string();
        self.kern
            .einbinden(&self.basis, &code, "Testrechner")
            .await
            .expect("Einbinden klappt");
    }

    /// Warten, bis ein Eintrag läuft und seine Werkzeuge kennt.
    async fn warte_auf_laeuft(&self, id: &EintragId) -> quiver_client::EintragSicht {
        let frist = Instant::now() + FRIST;
        loop {
            let stand = self.kern.modell().borrow().clone();
            if let Some(sicht) = stand.eintraege.iter().find(|eintrag| &eintrag.id == id) {
                if matches!(sicht.zustand, EintragZustand::Laeuft { .. }) && !sicht.tools.is_empty()
                {
                    return sicht.clone();
                }
            }
            assert!(
                Instant::now() < frist,
                "der Eintrag startet nicht: {:?}",
                stand.eintraege
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Ein Agent am Endpunkt dieses Geräts.
    async fn agent(&self, name: &str) -> rmcp::service::RunningService<RoleClient, Zeuge> {
        let token = self
            .kern
            .agent_koppeln(name)
            .await
            .expect("Agent gekoppelt");
        let transporte = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(self.endpunkt()).auth_header(&token),
        );
        Zeuge::neu()
            .serve(transporte)
            .await
            .expect("Agent verbindet sich")
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

fn konfig_fuer(ordner: &std::path::Path) -> KernKonfig {
    KernKonfig {
        profil: ordner.join("profil"),
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: std::sync::Arc::new(SpeicherTresor::neu()),
        browser: std::sync::Arc::new(quiver_client::SystemBrowser),
    }
}

/// Der Test-Agent: braucht keine Meldungen, nur einen Namen.
#[derive(Clone)]
struct Zeuge;

impl Zeuge {
    fn neu() -> Self {
        Self
    }
}

impl ClientHandler for Zeuge {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("quiver-haertung-zeuge", "0.1.0"),
        )
    }
}

// --------------------------------------------------------------- Testdaten

fn demo_bin() -> String {
    env!("CARGO_BIN_EXE_quiver-demo-mcp").to_string()
}

fn attrappe_bin() -> String {
    env!("CARGO_BIN_EXE_quiver-anmelde-attrappe").to_string()
}

/// Ein Eintrag, der den Demo-MCP startet; `umgebung` kommt oben drauf.
fn demo_eintrag(id: &str, protokoll: &std::path::Path, umgebung: Werte) -> Eintrag {
    let mut alle = Werte::from([
        (
            "QUIVER_DEMO_TOOLS".to_string(),
            "echo=Gibt den Text zurück".to_string(),
        ),
        ("QUIVER_DEMO_NAME".to_string(), id.to_string()),
        (
            "QUIVER_DEMO_LOG".to_string(),
            protokoll.to_string_lossy().to_string(),
        ),
    ]);
    alle.extend(umgebung);
    Eintrag {
        id: EintragId::neu(id),
        version: "0.1.0".into(),
        titel: format!("Titel {id}"),
        beschreibung: String::new(),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: demo_bin(),
            argumente: Vec::new(),
            umgebung: alle,
        },
        felder: Vec::new(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
    }
}

fn eingaben(wert: Value) -> rmcp::model::JsonObject {
    serde_json::from_value(wert).expect("JSON-Objekt")
}

fn text_aus(ergebnis: &CallToolResult) -> String {
    ergebnis
        .content
        .iter()
        .filter_map(|inhalt| inhalt.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn aufrufen(
    peer: &Peer<RoleClient>,
    name: &str,
    argumente: Value,
) -> Result<CallToolResult, rmcp::service::ServiceError> {
    peer.call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(eingaben(argumente)))
        .await
}

// ------------------------------------------------------------------ 1. Frist

/// Ein Aufruf ohne Lebenszeichen endet nach der Frist mit einer klaren
/// Meldung – der Client hängt nicht am Tool.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn die_frist_beendet_einen_haengenden_aufruf() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    // Eine Sekunde Frist statt der zwei Minuten.
    p.kern.aufruf_frist_setzen(&id, 1).expect("Frist gesetzt");
    p.zuweisen(
        vec![demo_eintrag(
            "echo",
            &p.protokoll,
            Werte::from([("QUIVER_DEMO_LANGSAM_MS".to_string(), "8000".to_string())]),
        )],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let agent = p.agent("pi").await;

    let beginn = Instant::now();
    let fehler = aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" }))
        .await
        .expect_err("der Aufruf läuft in die Frist");
    assert!(
        beginn.elapsed() < Duration::from_secs(6),
        "die Frist greift nach {:?}",
        beginn.elapsed()
    );
    let text = format!("{fehler:?}");
    assert!(
        text.contains("timeout") || text.contains("Frist"),
        "die Meldung nennt die Frist: {text}"
    );
    p.kern.beenden().await;
}

/// Fortschrittsmeldungen des Tools verlängern die Frist: ein Aufruf, der
/// länger als die Frist arbeitet, aber regelmäßig meldet, kommt durch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fortschritt_verlaengert_die_frist() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.kern.aufruf_frist_setzen(&id, 1).expect("Frist gesetzt");
    p.zuweisen(
        vec![demo_eintrag(
            "echo",
            &p.protokoll,
            Werte::from([
                ("QUIVER_DEMO_FORTSCHRITT".to_string(), "6".to_string()),
                ("QUIVER_DEMO_FORTSCHRITT_MS".to_string(), "400".to_string()),
                ("QUIVER_DEMO_LANGSAM_MS".to_string(), "200".to_string()),
            ]),
        )],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let agent = p.agent("pi").await;

    let antwort = aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" }))
        .await
        .expect("der Fortschritt verlängert die Frist");
    assert!(text_aus(&antwort).contains("hallo"));
    p.kern.beenden().await;
}

/// Ein Abbruch des Agenten beendet auch den Aufruf beim Tool.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_abbruch_beendet_den_aufruf() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.zuweisen(
        vec![demo_eintrag(
            "echo",
            &p.protokoll,
            Werte::from([("QUIVER_DEMO_LANGSAM_MS".to_string(), "10000".to_string())]),
        )],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let agent = p.agent("pi").await;

    let anfrage = ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
        CallToolRequestParams::new("echo__echo".to_string())
            .with_arguments(eingaben(json!({ "text": "hallo" }))),
    ));
    let griff = agent
        .peer()
        .send_cancellable_request(anfrage, Default::default())
        .await
        .expect("Anfrage abgeschickt");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let beginn = Instant::now();
    griff
        .cancel(Some("abgebrochen".to_string()))
        .await
        .expect("Abbruch abgeschickt");
    // Der Client beendet den Aufruf beim Tool: er muss nicht die zehn
    // Sekunden des Tools abwarten.
    let frist = Instant::now() + Duration::from_secs(5);
    loop {
        let aufrufe = p.kern.protokoll(&Filter::default());
        if aufrufe
            .iter()
            .any(|aufruf| aufruf.tool == "echo__echo" && aufruf.status.ist_fehler())
        {
            break;
        }
        assert!(
            Instant::now() < frist,
            "der Abbruch kommt beim Tool an: {aufrufe:#?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        beginn.elapsed() < Duration::from_secs(5),
        "schnell beendet: {:?}",
        beginn.elapsed()
    );
    p.kern.beenden().await;
}

// ------------------------------------------- 2. Größe und Auslastung

/// Eine Antwort über 8 MB ist ein klarer Fehler – und der Client bleibt
/// bedienbar.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eine_zu_grosse_antwort_ist_ein_klarer_fehler() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.zuweisen(
        vec![demo_eintrag(
            "echo",
            &p.protokoll,
            Werte::from([("QUIVER_DEMO_GROSS".to_string(), "9000000".to_string())]),
        )],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let agent = p.agent("pi").await;

    let fehler = aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" }))
        .await
        .expect_err("die Antwort ist zu groß");
    let text = format!("{fehler:?}");
    assert!(text.contains("zu groß"), "{text}");
    assert!(text.contains("Byte"), "{text}");

    // Der Client antwortet weiter: ein zweiter Aufruf (mit kleiner Antwort)
    // läuft.
    let antwort = aufrufen(agent.peer(), "quiver__protokoll", json!({}))
        .await
        .expect("der Endpunkt lebt");
    assert!(!text_aus(&antwort).is_empty());
    p.kern.beenden().await;
}

/// Bei mehr als acht gleichzeitigen Aufrufen je Eintrag antwortet der Client
/// sofort mit „ausgelastet“, statt alle zu verlangsamen.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zu_viele_gleichzeitige_aufrufe_werden_abgewiesen() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.zuweisen(
        vec![demo_eintrag(
            "echo",
            &p.protokoll,
            Werte::from([("QUIVER_DEMO_LANGSAM_MS".to_string(), "1500".to_string())]),
        )],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let agent = p.agent("pi").await;

    let mut aufgaben = Vec::new();
    for nummer in 0..12 {
        let peer = agent.peer().clone();
        aufgaben.push(tokio::spawn(async move {
            let ergebnis = aufrufen(&peer, "echo__echo", json!({ "text": nummer })).await;
            (nummer, ergebnis.map(|ergebnis| text_aus(&ergebnis)))
        }));
    }
    let mut ausgelastet = 0;
    let mut durch = 0;
    for aufgabe in aufgaben {
        let (_, ergebnis) = aufgabe.await.expect("Aufgabe läuft");
        match ergebnis {
            Ok(_) => durch += 1,
            Err(fehler) => {
                let text = format!("{fehler:?}");
                assert!(text.contains("Ausgelastet"), "{text}");
                ausgelastet += 1;
            }
        }
    }
    assert!(durch >= 1, "mindestens ein Aufruf kommt durch");
    assert!(
        ausgelastet >= 1,
        "die Grenze greift (durch: {durch}, ausgelastet: {ausgelastet})"
    );
    p.kern.beenden().await;
}

// ------------------------------------- 3. Zu große Meldung über stdio

/// Eine Meldung des Tools über 16 MB ist kein Zustand, dem man trauen kann:
/// der Eintrag wird neu gestartet (mit Pause), und das steht im Protokoll.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eine_zu_grosse_meldung_startet_den_eintrag_neu() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.zuweisen(
        vec![demo_eintrag(
            "echo",
            &p.protokoll,
            Werte::from([("QUIVER_DEMO_GROSS".to_string(), "17000000".to_string())]),
        )],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let agent = p.agent("pi").await;

    // Der Aufruf scheitert an der Grenze.
    let _ = aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" })).await;

    // Der Eintrag wird neu gestartet und sagt es im Protokoll.
    let frist = Instant::now() + FRIST;
    loop {
        let zeilen = p.kern.prozess_log(&id, &LogFilter::default());
        if zeilen.iter().any(|zeile| zeile.text.contains("größer als")) {
            break;
        }
        assert!(
            Instant::now() < frist,
            "der Neustart steht im Protokoll: {zeilen:#?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    p.kern.beenden().await;
}

// ------------------------------------------------------- 4. Kaputter Tresor

/// Ein kaputter Tresor verhindert den Start nicht: das Fenster sieht den
/// Fehler, nichts wird überschrieben, und erst „Neu beginnen“ schreibt wieder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_kaputter_tresor_verhindert_den_start_nicht() {
    let ordner = tempfile::tempdir().expect("Ordner");
    let profil = ordner.path().join("profil");
    let tresor = DateiTresor::fuer_profil(&profil);
    tresor
        .schreiben("geraet-token", "token-von-gestern")
        .expect("schreibbar");
    // Kaputt machen, wie es ein abgebrochener Schreibvorgang täte.
    std::fs::write(ordner.path().join("profil/tresor/tresor.json"), "kein json")
        .expect("schreibbar");

    let kern = Kern::starten(KernKonfig {
        profil: profil.clone(),
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: std::sync::Arc::new(DateiTresor::fuer_profil(&profil)),
        browser: std::sync::Arc::new(quiver_client::SystemBrowser),
    })
    .await
    .expect("der Start gelingt trotz kaputtem Tresor");

    let modell = kern.modell().borrow().clone();
    let fehler = modell
        .tresor_fehler
        .clone()
        .expect("der Fehler steht im Modell");
    assert!(fehler.contains("beschädigt"), "{fehler}");
    kern.tresor_neu_beginnen().expect("Neu beginnen klappt");
    // Die alten Dateien liegen als `.kaputt-<zeit>` daneben.
    let dateien: Vec<String> = std::fs::read_dir(profil.join("tresor"))
        .expect("Ordner")
        .flatten()
        .map(|eintrag| eintrag.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        dateien
            .iter()
            .any(|name| name.starts_with("tresor.json.kaputt-")),
        "{dateien:?}"
    );

    assert_eq!(
        kern.modell().borrow().tresor_fehler,
        None,
        "der Fehler ist weg"
    );
    kern.beenden().await;
}

// ---------------------------------- 5. Kaputtes Profil und zweiter Kern

/// Ein kaputter gespeicherter Stand wird beiseitegelegt und im Fenster
/// gemeldet; ein zweiter Kern auf demselben Profil startet nicht.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kaputtes_profil_und_zweiter_kern() {
    let ordner = tempfile::tempdir().expect("Ordner");
    let profil = ordner.path().join("profil");
    std::fs::create_dir_all(&profil).expect("Ordner");
    std::fs::write(profil.join("zustand.json"), "kein json").expect("schreibbar");

    let kern = Kern::starten(KernKonfig {
        profil: profil.clone(),
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: std::sync::Arc::new(SpeicherTresor::neu()),
        browser: std::sync::Arc::new(quiver_client::SystemBrowser),
    })
    .await
    .expect("der Start gelingt");
    let modell = kern.modell().borrow().clone();
    let hinweis = modell
        .profil_hinweis
        .clone()
        .expect("der Hinweis steht im Modell");
    assert!(hinweis.contains("nicht lesbar"), "{hinweis}");
    let kaputt: Vec<String> = std::fs::read_dir(&profil)
        .expect("Ordner")
        .flatten()
        .map(|eintrag| eintrag.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        kaputt
            .iter()
            .any(|name| name.starts_with("zustand.json.kaputt-")),
        "{kaputt:?}"
    );

    // Ein zweiter Kern auf demselben Profil startet nicht.
    let zweiter = Kern::starten(KernKonfig {
        profil: profil.clone(),
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: std::sync::Arc::new(SpeicherTresor::neu()),
        browser: std::sync::Arc::new(quiver_client::SystemBrowser),
    })
    .await;
    let fehler = format!("{:#}", zweiter.expect_err("das Profil ist belegt"));
    assert!(fehler.contains("schon benutzt"), "{fehler}");

    kern.beenden().await;
    // Nach dem Beenden ist das Profil wieder frei (ein Neustart).
    let nach = Kern::starten(KernKonfig {
        profil,
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: std::sync::Arc::new(SpeicherTresor::neu()),
        browser: std::sync::Arc::new(quiver_client::SystemBrowser),
    })
    .await
    .expect("nach dem Beenden startet ein neuer Kern");
    nach.beenden().await;
}

// ------------------------------------- 6. Unbrauchbare Kennung im Profil

/// Eine Kennung, die nicht als Präfix taugt, wird nicht gestartet und steht
/// mit Grund im Modell – das Fenster kann sie benennen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eine_unbrauchbare_kennung_startet_nicht() {
    let ordner = tempfile::tempdir().expect("Ordner");
    let profil = ordner.path().join("profil");
    std::fs::create_dir_all(&profil).expect("Ordner");
    // Ein gespeicherter Stand mit zwei Einträgen: einer taugt, einer nicht.
    let protokoll = ordner.path().join("protokoll.log");
    let soll = quiver_domain::Soll::neu(vec![
        quiver_domain::SollEintrag {
            eintrag: demo_eintrag("echo", &protokoll, Werte::new()),
            werte: Werte::new(),
            fest: Default::default(),
        },
        quiver_domain::SollEintrag {
            eintrag: demo_eintrag("quiver", &protokoll, Werte::new()),
            werte: Werte::new(),
            fest: Default::default(),
        },
    ]);
    let datei = json!({ "soll": soll });
    std::fs::write(
        profil.join("zustand.json"),
        serde_json::to_vec(&datei).expect("JSON"),
    )
    .expect("schreibbar");

    let kern = Kern::starten(KernKonfig {
        profil,
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: std::sync::Arc::new(SpeicherTresor::neu()),
        browser: std::sync::Arc::new(quiver_client::SystemBrowser),
    })
    .await
    .expect("Kern startet");

    let modell = kern.modell().borrow().clone();
    let kaputt = modell
        .eintraege
        .iter()
        .find(|eintrag| eintrag.id.als_str() == "quiver")
        .expect("der Eintrag steht im Modell");
    let grund = kaputt.unzulaessig.clone().expect("der Grund steht dabei");
    assert!(grund.contains("reserviert"), "{grund}");
    // Der gespeicherte Stand läuft nicht an, also läuft auch echo nicht.
    assert!(modell
        .eintraege
        .iter()
        .all(|eintrag| !matches!(eintrag.zustand, EintragZustand::Laeuft { .. })));
    kern.beenden().await;
}

// ---------------------------------------- 7. Nur eigene Aufrufe

/// Ein Agent sieht nur eigene Aufrufe; `quiver__tool_log` gibt es nur für
/// Einträge, die er selbst benutzt hat.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_agent_sieht_nur_seine_eigenen_aufrufe() {
    let p = Pruefung::neu().await;
    p.zuweisen(
        vec![
            demo_eintrag("echo", &p.protokoll, Werte::new()),
            demo_eintrag("dateien", &p.protokoll, Werte::new()),
        ],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&EintragId::neu("echo")).await;
    p.warte_auf_laeuft(&EintragId::neu("dateien")).await;

    let pi = p.agent("pi").await;
    let hermes = p.agent("hermes").await;
    // pi benutzt „echo“, hermes nur „dateien“.
    aufrufen(pi.peer(), "echo__echo", json!({ "text": "von-pi" }))
        .await
        .expect("pi ruft echo");
    aufrufen(
        hermes.peer(),
        "dateien__echo",
        json!({ "text": "von-hermes" }),
    )
    .await
    .expect("hermes ruft dateien");

    // Jeder sieht nur seine eigenen Zeilen.
    let protokoll_pi = text_aus(
        &aufrufen(pi.peer(), "quiver__protokoll", json!({}))
            .await
            .expect("Protokoll"),
    );
    assert!(protokoll_pi.contains("echo__echo"), "{protokoll_pi}");
    assert!(!protokoll_pi.contains("dateien__echo"), "{protokoll_pi}");
    assert!(protokoll_pi.contains("\"agent\": \"pi\""), "{protokoll_pi}");

    let protokoll_hermes = text_aus(
        &aufrufen(hermes.peer(), "quiver__protokoll", json!({}))
            .await
            .expect("Protokoll"),
    );
    assert!(
        !protokoll_hermes.contains("echo__echo"),
        "{protokoll_hermes}"
    );
    assert!(
        protokoll_hermes.contains("dateien__echo"),
        "{protokoll_hermes}"
    );

    // Der Schalter für Inhalte ist aus: die Log-Antwort ist leer.
    let log = aufrufen(
        hermes.peer(),
        "quiver__tool_log",
        json!({ "eintrag": "dateien" }),
    )
    .await
    .expect("eigener Eintrag");
    assert!(text_aus(&log).contains("hinweis"), "{}", text_aus(&log));

    // Ein fremder Eintrag ist gesperrt: „echo“ hat hermes nie benutzt.
    let fremd = aufrufen(
        hermes.peer(),
        "quiver__tool_log",
        json!({ "eintrag": "echo" }),
    )
    .await
    .expect("Antwort ist ein Fehlerergebnis");
    assert_eq!(fremd.is_error, Some(true));
    assert!(
        text_aus(&fremd).contains("nicht benutzt"),
        "{}",
        text_aus(&fremd)
    );

    p.kern.beenden().await;
}

// ------------------------------------------------------ 9. Adresse

/// `einbinden` prüft die Server-Adresse: unverschlüsseltes http nur auf
/// diesem Rechner.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn einbinden_prueft_die_server_adresse() {
    let p = Pruefung::neu().await;
    let fehler = p
        .kern
        .einbinden("http://fremd.example", "code", "Testrechner")
        .await
        .expect_err("http auf einem fremden Rechner");
    let text = format!("{fehler:#}");
    assert!(text.contains("nur auf diesem Rechner"), "{text}");
    p.kern.beenden().await;
}

// ------------------------------------------ 10./15. Netz und Herkunft

/// Anfragen mit `Origin` kommen nur von Loopback; der `Host` muss der eigene
/// Port sein. Ohne `Origin` (CLI-Agent) ist beides erlaubt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn herkunft_und_host_werden_geprueft() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.zuweisen(
        vec![demo_eintrag("echo", &p.protokoll, Werte::new())],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let token = p.kern.agent_koppeln("pi").await.expect("Agent");
    let endpunkt = p.endpunkt();

    // Fremder Origin: 403, auch mit gültigem Token.
    let fremd = p
        .http
        .post(&endpunkt)
        .header("authorization", format!("Bearer {token}"))
        .header("origin", "https://boese.example")
        .header("content-type", "application/json")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
        .send()
        .await
        .expect("Antwort");
    assert_eq!(fremd.status(), reqwest::StatusCode::FORBIDDEN);

    // Fremder Host: 403.
    let falscher_host = p
        .http
        .post(&endpunkt)
        .header("authorization", format!("Bearer {token}"))
        .header("host", "boese.example")
        .header("content-type", "application/json")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
        .send()
        .await
        .expect("Antwort");
    assert_eq!(falscher_host.status(), reqwest::StatusCode::FORBIDDEN);

    // Loopback-Origin und eigener Host: erlaubt (200/4xx fachlich, nicht 403).
    let adresse = endpunkt
        .trim_start_matches("http://")
        .trim_end_matches("/mcp")
        .to_string();
    let eigen = p
        .http
        .post(&endpunkt)
        .header("authorization", format!("Bearer {token}"))
        .header("origin", format!("http://{adresse}"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#)
        .send()
        .await
        .expect("Antwort");
    assert_ne!(
        eigen.status(),
        reqwest::StatusCode::FORBIDDEN,
        "Loopback ist erlaubt"
    );
    p.kern.beenden().await;
}

// --------------------------------------------- 13. Einrichtung abbrechen

/// Eine laufende Einrichtung lässt sich abbrechen: der Prozess endet, der
/// Zustand geht zurück auf „Bereit“.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eine_einrichtung_laesst_sich_abbrechen() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("outline");

    let mut eintrag = demo_eintrag("outline", &p.protokoll, Werte::new());
    eintrag.id = id.clone();
    eintrag.laufzeit = Laufzeit::Stdio {
        befehl: attrappe_bin(),
        argumente: vec!["mcp".to_string()],
        umgebung: Werte::from([
            ("QUIVER_ATTRAPPE_MCP".to_string(), demo_bin()),
            (
                "QUIVER_ATTRAPPE_DATEI".to_string(),
                p.ordner
                    .join("anmeldung.stand")
                    .to_string_lossy()
                    .to_string(),
            ),
        ]),
    };
    // Ein Einrichtungsbefehl, der auf den User wartet: er endet erst nach
    // 30 Sekunden – oder durch den Abbruch.
    eintrag.einrichtung = Some(Einrichtung {
        einrichten: vec!["warten".into(), "30000".into()],
    });
    p.zuweisen(vec![eintrag], Werte::new()).await;
    p.warte_auf_laeuft(&id).await;

    // Die Einrichtung hängt (die Attrappe wartet auf eine Datei, die nie
    // kommt).
    let kern = p.kern.clone();
    let fuer_lauf = id.clone();
    let lauf = tokio::spawn(async move { kern.einrichten(&fuer_lauf).await });
    let frist = Instant::now() + FRIST;
    loop {
        let stand = p.kern.modell().borrow().clone();
        let laeuft = stand
            .eintraege
            .iter()
            .find(|eintrag| eintrag.id == id)
            .and_then(|eintrag| eintrag.einrichtung.clone())
            .is_some_and(|zustand| zustand.ist_laeuft());
        if laeuft {
            break;
        }
        assert!(
            Instant::now() < frist,
            "die Einrichtung startet nicht: {:?}",
            stand.eintraege
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let beginn = Instant::now();
    p.kern.einrichtung_abbrechen(&id);
    tokio::time::timeout(Duration::from_secs(5), lauf)
        .await
        .expect("der Abbruch greift")
        .expect("Aufgabe läuft")
        .expect("ein Abbruch ist kein Fehler");
    assert!(
        beginn.elapsed() < Duration::from_secs(5),
        "{:?}",
        beginn.elapsed()
    );
    let stand = p.kern.modell().borrow().clone();
    let zustand = stand
        .eintraege
        .iter()
        .find(|eintrag| eintrag.id == id)
        .and_then(|eintrag| eintrag.einrichtung.clone());
    assert_eq!(zustand, Some(quiver_client::EinrichtZustand::Bereit));
    p.kern.beenden().await;
}

// ------------------------------------------------ 18. Umgebung (Whitelist)

/// Ein Werkzeug erbt nicht die ganze Umgebung: nur die erlaubten Werte und
/// die des Eintrags.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_werkzeug_erbt_nicht_die_ganze_umgebung() {
    // Eine Variable, die nur der Test-Prozess hat.
    std::env::set_var("QUIVER_TEST_FREMDTOKEN", "streng-geheim");
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.zuweisen(
        vec![demo_eintrag(
            "echo",
            &p.protokoll,
            Werte::from([(
                "QUIVER_DEMO_UMGEBUNG".to_string(),
                "QUIVER_TEST_FREMDTOKEN".to_string(),
            )]),
        )],
        Werte::new(),
    )
    .await;
    p.warte_auf_laeuft(&id).await;
    let agent = p.agent("pi").await;
    let antwort = text_aus(
        &aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" }))
            .await
            .expect("Aufruf"),
    );
    assert!(
        antwort.contains("QUIVER_TEST_FREMDTOKEN=leer"),
        "die Umgebung des Clients wird nicht vererbt: {antwort}"
    );
    // Was der Eintrag selbst setzt, kommt an (hier über QUIVER_DEMO_NAME).
    assert!(antwort.contains("hallo"));
    p.kern.beenden().await;
}
