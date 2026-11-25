// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Protokoll der Werkzeug-Aufrufe und der Werkzeug-Prozesse (Aufgabe 17,
//! ausgeführt für `docs/aufgaben/17-protokoll.md`).
//!
//! Der Ablauf ist der aus der Aufgabe:
//! 1. Jeder `tools/call` am Endpunkt wird mit Zeit, Agent, Eintrag, Werkzeug,
//!    Dauer und Ergebnis aufgezeichnet; Argumente gekürzt, Antwort als Vorschau.
//! 2. Fehler und Ablehnungen stehen mit Text im Protokoll.
//! 3. Ein Wert aus dem Tresor erscheint in **keiner** Datei und keiner Antwort,
//!    auch nicht als Teilstring.
//! 4. stderr eines Eintrags landet je Eintrag im Protokoll, ohne ANSI-Codes.
//! 5. `quiver__protokoll` und `quiver__tool_log` liefern ohne den Schalter nur
//!    Metadaten; mit Schalter Argumente und Zeilen. Die eigenen Aufrufe landen
//!    nicht im Protokoll.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use quiver_client::protokoll::{Filter, LogFilter, Status};
use quiver_client::{Aufruf, Kern, KernKonfig, SpeicherTresor, Tresor, Zeile};
use quiver_domain::{
    Eintrag, EintragId, Feld, Gruppe, GruppeId, Herkunft, Laufzeit, Quelle, Satz, SatzEintrag,
    SatzId, Werte,
};
use quiver_protocol::EintragZustand;
use quiver_server::{app, Konfig};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientConfig, Implementation, PaginatedRequestParams,
};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{ClientHandler, Peer, RoleClient, ServiceExt};
use serde_json::{json, Value};

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// Das Geheimnis, das nirgends auftauchen darf.
const GEHEIM: &str = "super-geheim-4711";
const FRIST: Duration = Duration::from_secs(10);

// ------------------------------------------------------------------ Gerüst

struct Pruefung {
    ordner: PathBuf,
    /// `http://127.0.0.1:<port>`
    basis: String,
    /// Sitzungscookie der Verwaltung.
    cookie: String,
    kern: Kern,
    http: reqwest::Client,
    tresor: Arc<SpeicherTresor>,
}

impl Pruefung {
    async fn neu() -> Self {
        // Zähler **und** Zeitstempel: zwei Tests im selben Prozess dürfen sich
        // niemals denselben Ordner teilen.
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "quiver-client-protokoll-{}-{nummer}-{stempel}",
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

        let tresor = Arc::new(SpeicherTresor::neu());
        let kern = Kern::starten(KernKonfig {
            profil: ordner.join("profil"),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: tresor.clone(),
            browser: std::sync::Arc::new(quiver_client::SystemBrowser),
        })
        .await
        .expect("Kern startet");

        Self {
            ordner,
            basis,
            cookie,
            kern,
            http,
            tresor,
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

    async fn put(&self, pfad: &str, wert: &impl serde::Serialize) {
        self.admin("PUT", pfad, Some(json!(wert))).await;
    }

    /// Den Endpunkt, wie ihn der Agent anspricht.
    fn endpunkt(&self) -> String {
        self.kern.modell().borrow().endpunkt.clone()
    }

    /// Einen Eintrag einbinden: Admin-Daten setzen, Code holen, Gerät anmelden.
    async fn einbinden(&self, eintrag: Eintrag) {
        self.put(&format!("/admin/eintraege/{}", eintrag.id), &eintrag)
            .await;
        self.put(
            "/admin/saetze/basis",
            &Satz {
                id: SatzId::neu("basis"),
                titel: "Basis".into(),
                prioritaet: 0,
                eintraege: vec![SatzEintrag {
                    eintrag: eintrag.id.clone(),
                    werte: Werte::new(),
                }],
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

    /// Warten, bis der Eintrag im Modell läuft und seine Werkzeuge kennt.
    async fn warte_auf_laeuft(&self, id: &EintragId) {
        let frist = Instant::now() + FRIST;
        loop {
            let stand = self.kern.modell().borrow().clone();
            if let Some(sicht) = stand.eintraege.iter().find(|eintrag| &eintrag.id == id) {
                if matches!(sicht.zustand, EintragZustand::Laeuft { .. }) && !sicht.tools.is_empty()
                {
                    return;
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

    /// Warten, bis ein Aufruf im Protokoll steht, auf den die Bedingung passt.
    async fn warte_auf_aufruf(&self, was: &str, bedingung: impl Fn(&Aufruf) -> bool) -> Aufruf {
        let frist = Instant::now() + FRIST;
        loop {
            let aufrufe = self.kern.protokoll(&Filter::default());
            if let Some(aufruf) = aufrufe.iter().find(|aufruf| bedingung(aufruf)) {
                return aufruf.clone();
            }
            assert!(
                Instant::now() < frist,
                "„{was}“ steht nicht im Protokoll: {aufrufe:#?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Warten, bis Zeilen eines Eintrags im Protokoll stehen.
    async fn warte_auf_zeilen(&self, eintrag: &EintragId) -> Vec<Zeile> {
        let frist = Instant::now() + FRIST;
        loop {
            let zeilen = self.kern.prozess_log(eintrag, &LogFilter::default());
            if !zeilen.is_empty() {
                return zeilen;
            }
            assert!(Instant::now() < frist, "keine Prozesszeilen für {eintrag}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Alles, was im Protokollordner auf der Platte liegt.
    fn dateien(&self) -> String {
        self.dateien_mit(|name| name.ends_with(".jsonl"))
    }

    /// Nur die Aufrufe (`aufrufe-JJJJ-MM-TT.jsonl`), ohne die Prozesszeilen.
    fn aufruf_dateien(&self) -> String {
        self.dateien_mit(|name| name.starts_with("aufrufe-"))
    }

    fn dateien_mit(&self, passt: impl Fn(&str) -> bool) -> String {
        let ordner = self.ordner.join("profil").join("protokoll");
        let Ok(eintraege) = std::fs::read_dir(&ordner) else {
            return String::new();
        };
        let mut text = String::new();
        for eintrag in eintraege.flatten() {
            let name = eintrag.file_name().to_string_lossy().to_string();
            if passt(&name) {
                text.push_str(&std::fs::read_to_string(eintrag.path()).unwrap_or_default());
            }
        }
        text
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
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
            rmcp::model::ClientCapabilities::default(),
            Implementation::new("quiver-protokoll-zeuge", "0.1.0"),
        )
    }
}

// --------------------------------------------------------------- Testdaten

fn demo_bin() -> String {
    env!("CARGO_BIN_EXE_quiver-demo-mcp").to_string()
}

fn feld(name: &str, quelle: Quelle, pflicht: bool) -> Feld {
    Feld {
        name: name.into(),
        beschreibung: format!("Beschreibung {name}"),
        quelle,
        pflicht,
        standard: None,
        link: None,
        ..Default::default()
    }
}

/// Ein Eintrag, der den Demo-MCP startet. `umgebung` kommt oben drauf.
fn eintrag(id: &str, umgebung: Werte, felder: Vec<Feld>) -> Eintrag {
    let mut alle = Werte::from([
        (
            "QUIVER_DEMO_TOOLS".to_string(),
            "echo=Gibt den Text zurück".to_string(),
        ),
        ("QUIVER_DEMO_NAME".to_string(), id.to_string()),
    ]);
    alle.extend(umgebung);
    Eintrag {
        id: EintragId::neu(id),
        version: "0.1.0".into(),
        titel: format!("Titel {id}"),
        beschreibung: format!("Beschreibung {id}"),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: demo_bin(),
            argumente: Vec::new(),
            umgebung: alle,
        },
        felder,
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
    }
}

/// Argumente als JSON-Objekt.
fn eingaben(wert: Value) -> rmcp::model::JsonObject {
    serde_json::from_value(wert).expect("JSON-Objekt")
}

/// Der Text einer Werkzeugantwort.
fn text_aus(ergebnis: &CallToolResult) -> String {
    ergebnis
        .content
        .first()
        .and_then(|inhalt| inhalt.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_default()
}

/// Ein Werkzeug aufrufen; das Ergebnis ist Absicht (Fehler sind erlaubt).
async fn aufrufen(
    peer: &Peer<RoleClient>,
    name: &str,
    argumente: Value,
) -> Result<CallToolResult, rmcp::service::ServiceError> {
    peer.call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(eingaben(argumente)))
        .await
}

// ------------------------------------------------------------------ Die Tests

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_aufruf_steht_mit_agent_dauer_und_status_im_protokoll() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.einbinden(eintrag("echo", Werte::new(), Vec::new())).await;
    let agent = p.agent("test").await;
    p.warte_auf_laeuft(&id).await;

    let antwort = aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" }))
        .await
        .expect("tools/call");
    assert!(text_aus(&antwort).contains("hallo"));

    let aufruf = p
        .warte_auf_aufruf("der Aufruf", |aufruf| aufruf.tool == "echo__echo")
        .await;
    assert_eq!(aufruf.agent, "test", "der Agent kommt aus seinem Token");
    assert_eq!(aufruf.eintrag, id);
    assert_eq!(aufruf.status, Status::Ok);
    assert!(aufruf.zeit.ends_with('Z'), "RFC 3339: {}", aufruf.zeit);
    assert!(
        aufruf.argumente.contains("hallo"),
        "die Argumente stehen im Protokoll: {}",
        aufruf.argumente
    );
    assert!(
        aufruf.vorschau.chars().count() <= 500,
        "die Vorschau ist gekürzt: {} Zeichen",
        aufruf.vorschau.chars().count()
    );
    assert!(aufruf.vorschau.contains("hallo"), "{}", aufruf.vorschau);
    assert!(aufruf.antwort_groesse > 0);

    // Ein Werkzeug, das der Eintrag nicht anbietet: der Client reicht es
    // **nicht** weiter (die Zuordnung ist eine Tabelle, Paket 3) und hält die
    // Ablehnung fest.
    let fehler = aufrufen(agent.peer(), "echo__gibtsnicht", json!({})).await;
    assert!(fehler.is_err(), "der Client kennt das Werkzeug nicht");
    let aufruf = p
        .warte_auf_aufruf("der Fehler", |aufruf| aufruf.tool == "echo__gibtsnicht")
        .await;
    assert_eq!(aufruf.status, Status::Abgelehnt);
    assert!(
        aufruf
            .fehler
            .as_deref()
            .unwrap_or_default()
            .contains("unbekannt"),
        "{:?}",
        aufruf.fehler
    );

    // Ein Werkzeug, das es gar nicht gibt (kein Präfix): abgelehnt.
    let fehler = aufrufen(agent.peer(), "gibtsnicht", json!({})).await;
    assert!(fehler.is_err());
    let aufruf = p
        .warte_auf_aufruf("die Ablehnung", |aufruf| aufruf.tool == "gibtsnicht")
        .await;
    assert_eq!(aufruf.status, Status::Abgelehnt);
    assert!(
        aufruf
            .fehler
            .as_deref()
            .unwrap_or_default()
            .contains("ist unbekannt"),
        "{:?}",
        aufruf.fehler
    );

    // Auch auf der Platte: eine Zeile je Aufruf (die Prozesszeilen zählen nicht
    // mit, sie tragen kein `tool`).
    let text = p.dateien();
    let aufrufe = text
        .lines()
        .filter(|zeile| zeile.contains("\"tool\""))
        .count();
    assert_eq!(aufrufe, 3, "{text}");
    let _ = agent.cancel().await;
    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn argumente_mit_passwort_oder_api_token_sind_maskiert() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.einbinden(eintrag("echo", Werte::new(), Vec::new())).await;
    let agent = p.agent("test").await;
    p.warte_auf_laeuft(&id).await;

    aufrufen(
        agent.peer(),
        "echo__echo",
        json!({
            "passwort": "streng-geheim",
            "api_token": "qvr-token-123",
            "text": "offen"
        }),
    )
    .await
    .expect("tools/call");

    let aufruf = p
        .warte_auf_aufruf("der maskierte Aufruf", |aufruf| {
            aufruf.argumente.contains("offen")
        })
        .await;
    assert!(
        aufruf.argumente.contains("<verborgen>"),
        "{}",
        aufruf.argumente
    );
    assert!(
        !aufruf.argumente.contains("streng-geheim"),
        "{}",
        aufruf.argumente
    );
    assert!(
        !aufruf.argumente.contains("qvr-token-123"),
        "{}",
        aufruf.argumente
    );
    assert!(aufruf.argumente.contains("offen"), "{}", aufruf.argumente);

    // Die Vorschau ist die Antwort des Tools — dort stehen die Argumente
    // ebenfalls; auch sie ist maskiert.
    assert!(
        !aufruf.vorschau.contains("streng-geheim"),
        "{}",
        aufruf.vorschau
    );
    assert!(
        !aufruf.vorschau.contains("qvr-token-123"),
        "{}",
        aufruf.vorschau
    );

    // In den Aufrufen (Argumente und Vorschau) steht beides nicht mehr. Die
    // Prozesszeilen sind freier Text des Tools — dort gilt die Maske der
    // Tresorwerte, nicht die der Schlüsselnamen.
    let text = p.aufruf_dateien();
    assert!(!text.contains("streng-geheim"), "{text}");
    assert!(!text.contains("qvr-token-123"), "{text}");
    assert!(text.contains("<verborgen>"), "{text}");
    let _ = agent.cancel().await;
    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_tresorwert_erscheint_in_keiner_datei_und_keiner_antwort() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    // Der Demo-Server gibt den Geheimwert aus der Umgebung zurück und schreibt
    // ihn auf stderr — beides muss der Client maskieren.
    p.einbinden(eintrag(
        "echo",
        Werte::from([
            ("QUIVER_DEMO_GEHEIM".to_string(), "${TOKEN}".to_string()),
            ("QUIVER_DEMO_GEHEIM_ECHO".to_string(), "1".to_string()),
            ("QUIVER_DEMO_FARBE".to_string(), "1".to_string()),
        ]),
        vec![feld("TOKEN", Quelle::Geheim, true)],
    ))
    .await;
    let agent = p.agent("test").await;
    assert!(
        p.tresor.lesen("wert/echo/TOKEN").expect("lesbar").is_none(),
        "vor dem Setzen liegt nichts im Tresor"
    );
    p.kern
        .werte_setzen(
            &id,
            Werte::new(),
            Werte::from([("TOKEN".to_string(), GEHEIM.to_string())]),
        )
        .await
        .expect("Werte gesetzt");
    p.warte_auf_laeuft(&id).await;

    let antwort = aufrufen(agent.peer(), "echo__echo", json!({ "text": "x" }))
        .await
        .expect("tools/call");
    let text = text_aus(&antwort);
    assert!(
        text.contains(GEHEIM),
        "das Tool gibt den Geheimwert aus (sonst prüft der Test nichts): {text}"
    );

    // 1. Keine Datei im Protokollordner.
    let dateien = p.dateien();
    assert!(!dateien.is_empty(), "es muss Protokolldateien geben");
    assert!(
        !dateien.contains(GEHEIM),
        "der Tresorwert steht in einer Datei"
    );

    // 2. Nicht im Speicher — weder Argumente noch Vorschau noch Fehlertext.
    let aufruf = p
        .warte_auf_aufruf("der Aufruf", |aufruf| !aufruf.vorschau.is_empty())
        .await;
    assert!(!aufruf.vorschau.contains(GEHEIM), "{:?}", aufruf.vorschau);
    assert!(!aufruf.argumente.contains(GEHEIM), "{:?}", aufruf.argumente);

    // 3. Nicht in den Prozesszeilen (der Demo-Server schreibt sie auf stderr).
    let zeilen = p.warte_auf_zeilen(&id).await;
    let alle: String = zeilen.iter().map(|zeile| zeile.text.clone()).collect();
    assert!(
        alle.contains("gestartet"),
        "die Zeilen kommen vom Demo-Server: {alle:?}"
    );
    assert!(
        !alle.contains(GEHEIM),
        "der Tresorwert steht in einer Zeile"
    );

    // 4. Nicht in der Antwort von `quiver__protokoll` — auch nicht mit
    //    eingeschaltetem Schalter.
    p.kern.protokoll_fuer_agenten_setzen(true).await;
    let antwort = aufrufen(agent.peer(), "quiver__protokoll", json!({}))
        .await
        .expect("quiver__protokoll");
    let text = text_aus(&antwort);
    assert!(text.contains("\"aufrufe\""), "{text}");
    assert!(
        !text.contains(GEHEIM),
        "der Tresorwert steht in der Antwort"
    );
    let protokoll = aufrufen(
        agent.peer(),
        "quiver__tool_log",
        json!({ "eintrag": "echo" }),
    )
    .await
    .expect("quiver__tool_log");
    assert!(!text_aus(&protokoll).contains(GEHEIM));

    // 5. Und nicht im Profil.
    let profil = std::fs::read_to_string(p.ordner.join("profil").join("zustand.json"))
        .expect("zustand.json");
    assert!(!profil.contains(GEHEIM), "der Tresorwert steht im Profil");

    let _ = agent.cancel().await;
    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stderr_zeilen_landen_je_eintrag_ohne_ansi_codes() {
    let p = Pruefung::neu().await;
    let echo = EintragId::neu("echo");
    p.einbinden(eintrag(
        "echo",
        Werte::from([("QUIVER_DEMO_FARBE".to_string(), "1".to_string())]),
        Vec::new(),
    ))
    .await;
    let _agent = p.agent("test").await;
    p.warte_auf_laeuft(&echo).await;

    let zeilen = p.warte_auf_zeilen(&echo).await;
    let texte: Vec<&str> = zeilen.iter().map(|zeile| zeile.text.as_str()).collect();
    assert!(
        texte.iter().any(|text| text.contains("gestartet")),
        "{texte:?}"
    );
    for text in &texte {
        assert!(
            !text.contains('\u{1b}'),
            "ein ANSI-Farbcode steht noch in der Zeile: {text:?}"
        );
    }
    // Die Zeilen hängen am richtigen Eintrag: ein anderer sieht nichts.
    assert!(
        p.kern
            .prozess_log(&EintragId::neu("anders"), &LogFilter::default())
            .is_empty(),
        "Zeilen gehören zu genau einem Eintrag"
    );
    // Und sie stehen auf der Platte.
    assert!(!p.dateien().contains('\u{1b}'));
    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ohne_schalter_nur_metadaten_und_die_eigenen_tools_stehen_nicht_im_protokoll() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.einbinden(eintrag("echo", Werte::new(), Vec::new())).await;
    let agent = p.agent("test").await;
    p.warte_auf_laeuft(&id).await;

    // Beide Werkzeuge stehen in `tools/list` und sind lesend.
    let liste = agent
        .peer()
        .list_tools(Some(PaginatedRequestParams::default()))
        .await
        .expect("tools/list");
    for name in ["quiver__protokoll", "quiver__tool_log"] {
        let werkzeug = liste
            .tools
            .iter()
            .find(|werkzeug| werkzeug.name == name)
            .unwrap_or_else(|| panic!("{name} fehlt in tools/list"));
        assert_eq!(
            werkzeug.annotations.as_ref().and_then(|a| a.read_only_hint),
            Some(true),
            "{name} ist nicht als lesend markiert"
        );
        assert_eq!(
            werkzeug.input_schema.get("type").and_then(Value::as_str),
            Some("object"),
            "{name} hat kein type: object"
        );
    }

    // Ein echter Aufruf kommt ins Protokoll.
    aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" }))
        .await
        .expect("tools/call");

    // Ohne Schalter: nur Metadaten und ein Hinweis.
    let antwort = aufrufen(agent.peer(), "quiver__protokoll", json!({}))
        .await
        .expect("quiver__protokoll");
    let text = text_aus(&antwort);
    assert!(text.contains("\"hinweis\""), "{text}");
    assert!(text.contains("echo__echo"), "{text}");
    for gesperrt in ["argumente", "vorschau", "hallo"] {
        assert!(!text.contains(gesperrt), "{gesperrt} ist zu sehen: {text}");
    }
    let antwort = aufrufen(
        agent.peer(),
        "quiver__tool_log",
        json!({ "eintrag": "echo" }),
    )
    .await
    .expect("quiver__tool_log");
    let text = text_aus(&antwort);
    assert!(text.contains("\"hinweis\""), "{text}");
    assert!(!text.contains("gestartet"), "{text}");
    assert!(
        !text.contains("\"zeilen\": [\n    {"),
        "keine Zeilen: {text}"
    );

    // Mit Schalter: Argumente, Vorschau und Zeilen.
    p.kern.protokoll_fuer_agenten_setzen(true).await;
    let antwort = aufrufen(agent.peer(), "quiver__protokoll", json!({}))
        .await
        .expect("quiver__protokoll");
    let text = text_aus(&antwort);
    assert!(!text.contains("\"hinweis\""), "kein Hinweis mehr: {text}");
    assert!(text.contains("\"argumente\""), "{text}");
    assert!(text.contains("hallo"), "{text}");
    let antwort = aufrufen(
        agent.peer(),
        "quiver__tool_log",
        json!({ "eintrag": "echo", "enthaelt": "gestartet" }),
    )
    .await
    .expect("quiver__tool_log");
    let text = text_aus(&antwort);
    assert!(text.contains("gestartet"), "{text}");
    assert!(!text.contains("\"hinweis\""), "{text}");

    // Die eigenen Aufrufe (`quiver__*`) landen nicht im Protokoll.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let aufrufe = p.kern.protokoll(&Filter::default());
    assert!(
        aufrufe
            .iter()
            .all(|aufruf| !aufruf.tool.starts_with("quiver__")),
        "Rückkopplung im Protokoll: {:?}",
        aufrufe.iter().map(|a| &a.tool).collect::<Vec<_>>()
    );
    assert_eq!(
        aufrufe.len(),
        1,
        "nur der eine echte Aufruf steht drin: {aufrufe:#?}"
    );

    let _ = agent.cancel().await;
    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn die_abfrage_filtert_und_das_leeren_raeumt_auf() {
    let p = Pruefung::neu().await;
    let echo = EintragId::neu("echo");
    p.einbinden(eintrag("echo", Werte::new(), Vec::new())).await;
    let agent = p.agent("pi").await;
    p.warte_auf_laeuft(&echo).await;
    aufrufen(agent.peer(), "echo__echo", json!({ "text": "hallo" }))
        .await
        .expect("tools/call");
    p.warte_auf_aufruf("der Aufruf", |aufruf| aufruf.tool == "echo__echo")
        .await;

    // Der Filter kennt Eintrag, Werkzeug, Agent, nur Fehler und eine Anzahl.
    for filter in [
        Filter {
            eintrag: Some(echo.clone()),
            ..Default::default()
        },
        Filter {
            tool: Some("echo__echo".into()),
            ..Default::default()
        },
        Filter {
            agent: Some("pi".into()),
            ..Default::default()
        },
        Filter {
            anzahl: Some(1),
            ..Default::default()
        },
    ] {
        assert_eq!(p.kern.protokoll(&filter).len(), 1, "{filter:?}");
    }
    assert!(
        p.kern
            .protokoll(&Filter {
                nur_fehler: true,
                ..Default::default()
            })
            .is_empty(),
        "ein geglückter Aufruf ist kein Fehler"
    );

    // Leeren räumt Speicher und Dateien.
    p.kern.protokoll_leeren().expect("leerbar");
    assert!(p.kern.protokoll(&Filter::default()).is_empty());
    assert_eq!(p.dateien(), "", "keine Datei bleibt stehen");

    let _ = agent.cancel().await;
    p.kern.beenden().await;
}
