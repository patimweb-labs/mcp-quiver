// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ende-zu-Ende: ein Admin weist ein MCP zu, und **höchstens 5 Sekunden
//! später** sieht ein Agent, der am Client hängt, dessen Werkzeuge und kann
//! sie aufrufen – ohne Neustart von irgendetwas.
//!
//! Der Ablauf ist der aus `docs/aufgaben/04-client.md`:
//! 1. Admin legt Einträge `echo` und `geheim`, einen Satz, eine Gruppe und
//!    einen Code an.
//! 2. Der Client bindet sich ein, ein Agent wird gekoppelt.
//! 3. Der Agent sieht noch nichts.
//! 4. Der Admin ordnet den Satz der Gruppe **jetzt erst** zu → der Agent
//!    bekommt `notifications/tools/list_changed` und sieht `echo__echo`.
//! 5. `geheim` wartet auf sein Pflichtfeld und läuft nach `werte_setzen`.
//! 6. Ohne Token gibt es 401.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use quiver_client::{Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{
    Eintrag, EintragId, Feld, Gruppe, GruppeId, Herkunft, Laufzeit, Quelle, Satz, SatzEintrag,
    SatzId, Werte,
};
use quiver_protocol::EintragZustand;
use quiver_server::{app, Konfig};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientConfig, Implementation, PaginatedRequestParams,
};
use rmcp::service::NotificationContext;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{ClientHandler, Peer, RoleClient, ServiceExt};
use serde_json::{json, Value};

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// Das Abnahmekriterium: höchstens fünf Sekunden.
const FRIST: Duration = Duration::from_secs(5);

// ------------------------------------------------------------------ Gerüst

struct Pruefung {
    ordner: PathBuf,
    /// `http://127.0.0.1:<port>`
    basis: String,
    /// Sitzungscookie der Verwaltung.
    cookie: String,
    kern: Kern,
    http: reqwest::Client,
    /// Derselbe Tresor für einen zweiten Start mit diesem Profil.
    tresor: std::sync::Arc<SpeicherTresor>,
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
            "quiver-client-ende-{}-{nummer}-{stempel}",
            std::process::id()
        ));
        let daten = ordner.join("server");
        std::fs::create_dir_all(&daten).expect("Ordner");

        // Server auf einem freien Port.
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

        let tresor = std::sync::Arc::new(SpeicherTresor::neu());
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

    /// Der Endpunkt, wie ihn der Agent anspricht.
    fn endpunkt(&self) -> String {
        self.kern.modell().borrow().endpunkt.clone()
    }

    /// Ohne Token bzw. mit falschem Token gibt es 401.
    async fn ohne_token_401(&self) {
        let endpunkt = self.endpunkt();
        for token in [None, Some("falsches-token")] {
            let mut anfrage = self.http.post(&endpunkt).json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list"
            }));
            if let Some(token) = token {
                anfrage = anfrage.bearer_auth(token);
            }
            let antwort = anfrage.send().await.expect("Endpunkt erreichbar");
            assert_eq!(
                antwort.status(),
                reqwest::StatusCode::UNAUTHORIZED,
                "ohne gültiges Token muss der Endpunkt 401 antworten"
            );
        }
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

/// Der Test-Agent: sammelt die Meldungen „Werkzeugliste geändert“.
#[derive(Clone)]
struct Zeuge {
    meldungen: tokio::sync::mpsc::UnboundedSender<()>,
}

impl ClientHandler for Zeuge {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            rmcp::model::ClientCapabilities::default(),
            Implementation::new("quiver-test-agent", "0.1.0"),
        )
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.meldungen.send(());
    }
}

// --------------------------------------------------------------- Testdaten

fn echo_bin() -> String {
    env!("CARGO_BIN_EXE_quiver-echo-mcp").to_string()
}

/// Ein Eintrag, der den Echo-Server startet.
fn eintrag(id: &str, umgebung: Werte, felder: Vec<Feld>) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "0.1.0".into(),
        titel: format!("Titel {id}"),
        beschreibung: format!("Beschreibung {id}"),
        mehrfach: None,
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: echo_bin(),
            argumente: Vec::new(),
            umgebung,
        },
        felder,
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
    }
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

// ------------------------------------------------------------------ Der Test

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn werkzeug_ist_nach_fuenf_sekunden_beim_agenten() {
    let p = Pruefung::neu().await;

    // ---------------------------------------------------------- 1. Admin
    let echo = eintrag(
        "echo",
        Werte::from([("QUIVER_ECHO_PRAEFIX".to_string(), "${PRAEFIX}".to_string())]),
        vec![feld("PRAEFIX", Quelle::Server, false)],
    );
    let geheim = eintrag(
        "geheim",
        Werte::from([("QUIVER_ECHO_PRAEFIX".to_string(), "${TOKEN}".to_string())]),
        vec![feld("TOKEN", Quelle::Geheim, true)],
    );
    p.put("/admin/eintraege/echo", &echo).await;
    p.put("/admin/eintraege/geheim", &geheim).await;

    let satz = Satz {
        id: SatzId::neu("basis"),
        titel: "Basis".into(),
        prioritaet: 0,
        eintraege: vec![
            SatzEintrag {
                eintrag: EintragId::neu("echo"),
                werte: Werte::new(),
            },
            SatzEintrag {
                eintrag: EintragId::neu("geheim"),
                werte: Werte::new(),
            },
        ],
    };
    p.put("/admin/saetze/basis", &satz).await;

    // Die Gruppe ist zunächst leer: der Satz wird erst in Schritt 4 zugeordnet.
    let gruppe = |saetze: Vec<SatzId>| Gruppe {
        id: GruppeId::neu("team"),
        titel: "Team".into(),
        saetze,
        entra: Vec::new(),
    };
    p.put("/admin/gruppen/team", &gruppe(Vec::new())).await;

    let code = p
        .admin(
            "POST",
            "/admin/codes",
            Some(json!({ "gruppen": ["team"], "minuten": 60 })),
        )
        .await;
    let code = code["code"].as_str().expect("Code").to_string();

    // ------------------------------------------------ 2. Einbinden, Koppeln
    p.kern
        .einbinden(&p.basis, &code, "Testrechner")
        .await
        .expect("Einbinden klappt");
    let agent_token = p.kern.agent_koppeln("test").await.expect("Agent gekoppelt");

    // Der MCP-Client des Agenten.
    let (sender, mut meldungen) = tokio::sync::mpsc::unbounded_channel();
    let transporte = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(p.endpunkt()).auth_header(&agent_token),
    );
    let agent = Zeuge { meldungen: sender }
        .serve(transporte)
        .await
        .expect("Agent verbindet sich");

    // ------------------------------------------- 3. Noch sieht er nichts
    let vorher = agent
        .peer()
        .list_tools(Some(PaginatedRequestParams::default()))
        .await
        .expect("tools/list");
    // Nur die eigenen Werkzeuge von quiver stehen da (Aufgabe 17), kein Eintrag.
    assert!(
        vorher
            .tools
            .iter()
            .all(|tool| !tool.name.contains("__") || tool.name.starts_with("quiver__")),
        "vor der Zuweisung läuft nichts: {:?}",
        vorher
            .tools
            .iter()
            .map(|t| t.name.clone())
            .collect::<Vec<_>>()
    );

    // ---------------------------- 4. Jetzt erst zuordnen: höchstens 5 s
    let beginn = Instant::now();
    p.put("/admin/gruppen/team", &gruppe(vec![SatzId::neu("basis")]))
        .await;

    let gemeldet = tokio::time::timeout(FRIST, meldungen.recv()).await;
    assert!(
        matches!(gemeldet, Ok(Some(()))),
        "der Agent muss within 5 s notifications/tools/list_changed bekommen"
    );

    let echo_werkzeug = warte_auf_werkzeug(agent.peer(), "echo__echo", beginn).await;
    assert!(
        echo_werkzeug.is_some(),
        "echo__echo muss innerhalb von 5 s in tools/list stehen"
    );
    let echo_werkzeug = echo_werkzeug.expect("gerade geprüft");
    assert_eq!(
        echo_werkzeug.description.as_deref(),
        Some("Gibt den übergebenen Text unverändert zurück"),
        "Beschreibung bleibt unverändert"
    );
    assert!(
        echo_werkzeug.input_schema.contains_key("properties"),
        "Schema bleibt unverändert"
    );

    // Aufrufen: der Text kommt zurück, das Präfix aus dem Server-Wert auch.
    let antwort = agent
        .peer()
        .call_tool(
            CallToolRequestParams::new("echo__echo")
                .with_arguments(eingaben(json!({ "text": "hallo" }))),
        )
        .await
        .expect("tools/call");
    assert_eq!(text_aus(&antwort), "hallo");

    // ------------------------------- 5. Geheim-Feld: warten, setzen, laufen
    let geheim_id = EintragId::neu("geheim");
    assert!(
        warte_auf_zustand(&p.kern, &geheim_id, |zustand| matches!(
            zustand,
            EintragZustand::WartetAufWerte { .. }
        ))
        .await,
        "ohne Pflichtfeld wartet der Eintrag auf Werte"
    );
    assert!(
        agent
            .peer()
            .list_tools(Some(PaginatedRequestParams::default()))
            .await
            .expect("tools/list")
            .tools
            .iter()
            .all(|tool| tool.name != "geheim__echo"),
        "ein wartender Eintrag bietet keine Werkzeuge an"
    );

    p.kern
        .werte_setzen(
            &geheim_id,
            Werte::new(),
            Werte::from([("TOKEN".to_string(), "geheim-".to_string())]),
        )
        .await
        .expect("Werte gesetzt");

    assert!(
        warte_auf_zustand(&p.kern, &geheim_id, |zustand| matches!(
            zustand,
            EintragZustand::Laeuft { .. }
        ))
        .await,
        "nach werte_setzen läuft der Eintrag"
    );

    let antwort = agent
        .peer()
        .call_tool(
            CallToolRequestParams::new("geheim__echo")
                .with_arguments(eingaben(json!({ "text": "x" }))),
        )
        .await
        .expect("tools/call beim zweiten Eintrag");
    assert_eq!(
        text_aus(&antwort),
        "geheim-x",
        "der Geheimwert landet in der Umgebung des Eintrags und nie beim Agenten"
    );

    // ---------------------------------------------------- 6. Ohne Token 401
    p.ohne_token_401().await;

    // Der Server hat die Meldung des Clients bekommen: beide Einträge laufen.
    let geraete = p.admin("GET", "/admin/geraete", None).await;
    let stand = geraete
        .as_array()
        .and_then(|liste| liste.first())
        .expect("das Gerät steht auf dem Server");
    let gemeldet = &stand["meldung"]["eintraege"];
    assert_eq!(
        gemeldet.as_array().map(Vec::len),
        Some(2),
        "der Client meldet beide Einträge: {stand}"
    );
    assert!(
        gemeldet.to_string().matches("laeuft").count() == 2,
        "beide Einträge laufen: {gemeldet}"
    );

    // Der Agent gilt im Modell als verbunden, solange seine Sitzung offen ist.
    assert!(
        p.kern
            .modell()
            .borrow()
            .agenten
            .iter()
            .any(|agent| agent.name == "test" && agent.verbunden),
        "der gekoppelte Agent hat eine offene Sitzung"
    );

    // Im Profil steht nur der Hash des Agent-Tokens, nie das Token selbst.
    let profil = std::fs::read_to_string(p.ordner.join("profil").join("zustand.json"))
        .expect("zustand.json");
    assert!(
        profil.contains("token_hash"),
        "der Hash des Agent-Tokens wird gespeichert"
    );
    assert!(
        !profil.contains(&agent_token),
        "das Agent-Token selbst steht nicht im Profil"
    );
    assert!(
        !profil.contains("geheim-"),
        "Geheimwerte stehen nicht im Profil"
    );

    // Aufräumen.
    let _ = agent.cancel().await;
    p.kern.beenden().await;
}

/// Ein abgeschalteter Eintrag: der Agent sieht das Werkzeug nicht mehr, der
/// Client hat den Eintrag angehalten (kein Laeufer mehr, kein Prozess) und
/// der Aufruf laeuft ins Leere. Alles innerhalb der Frist von 5 Sekunden.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abgeschalteter_eintrag_verschwindet_beim_agenten() {
    let p = Pruefung::neu().await;

    let abschalt = eintrag("abschalt", Werte::new(), Vec::new());
    p.put("/admin/eintraege/abschalt", &abschalt).await;
    p.put(
        "/admin/saetze/basis",
        &Satz {
            id: SatzId::neu("basis"),
            titel: "Basis".into(),
            prioritaet: 0,
            eintraege: vec![SatzEintrag {
                eintrag: EintragId::neu("abschalt"),
                werte: Werte::new(),
            }],
        },
    )
    .await;
    p.put(
        "/admin/gruppen/team",
        &Gruppe {
            id: GruppeId::neu("team"),
            titel: "Team".into(),
            saetze: vec![SatzId::neu("basis")],
            entra: Vec::new(),
        },
    )
    .await;

    let code = p
        .admin(
            "POST",
            "/admin/codes",
            Some(json!({ "gruppen": ["team"], "minuten": 60 })),
        )
        .await;
    let code = code["code"].as_str().expect("Code").to_string();

    p.kern
        .einbinden(&p.basis, &code, "Testrechner")
        .await
        .expect("Einbinden klappt");
    let agent_token = p.kern.agent_koppeln("test").await.expect("Agent gekoppelt");

    let (sender, mut meldungen) = tokio::sync::mpsc::unbounded_channel();
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(p.endpunkt()).auth_header(&agent_token),
    );
    let agent = Zeuge { meldungen: sender }
        .serve(transport)
        .await
        .expect("Agent verbindet sich");

    // Der Eintrag laeuft und ist beim Agenten sichtbar.
    let beginn = Instant::now();
    assert!(
        warte_auf_werkzeug(agent.peer(), "abschalt__echo", beginn)
            .await
            .is_some(),
        "vor dem Abschalten laeuft das Werkzeug"
    );

    // ------------------------------------------------------------ Abschalten
    let beginn = Instant::now();
    let mut aus = eintrag("abschalt", Werte::new(), Vec::new());
    aus.aktiv = false;
    p.put("/admin/eintraege/abschalt", &aus).await;

    // 1. Innerhalb von 5 s ist das Werkzeug aus tools/list verschwunden.
    let mut weg = false;
    while beginn.elapsed() < FRIST {
        let liste = agent
            .peer()
            .list_tools(Some(PaginatedRequestParams::default()))
            .await
            .expect("tools/list");
        if liste.tools.iter().all(|tool| tool.name != "abschalt__echo") {
            weg = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        weg,
        "abschalt__echo muss innerhalb von 5 s aus tools/list verschwinden"
    );

    // 2. Der Client hat den Eintrag angehalten: kein Laeufer, kein Prozess.
    let id = EintragId::neu("abschalt");
    let frist = Instant::now() + FRIST;
    let mut modell = p.kern.modell();
    loop {
        let weg_aus_dem_modell = !modell
            .borrow_and_update()
            .eintraege
            .iter()
            .any(|eintrag| eintrag.id == id);
        if weg_aus_dem_modell {
            break;
        }
        let rest = frist.saturating_duration_since(Instant::now());
        assert!(!rest.is_zero(), "der Eintrag bleibt im Modell stehen");
        let _ = tokio::time::timeout(rest, modell.changed()).await;
    }
    assert!(
        !p.ordner.join("profil").join("zustand.json").exists()
            || !std::fs::read_to_string(p.ordner.join("profil").join("zustand.json"))
                .expect("zustand.json")
                .contains("\"abschalt\""),
        "der abgeschaltete Eintrag steht nicht mehr im Soll des Profils"
    );

    // 3. Der Aufruf laeuft ins Leere: es gibt keinen Laeufer mehr.
    let aufruf = agent
        .peer()
        .call_tool(
            CallToolRequestParams::new("abschalt__echo")
                .with_arguments(eingaben(json!({ "text": "x" }))),
        )
        .await;
    assert!(
        aufruf.is_err(),
        "ohne Laeufer gibt es keine Antwort mehr: {aufruf:?}"
    );

    // Und der Agent bekommt die Aenderung auch gemeldet (tools/list_changed).
    assert!(
        matches!(
            tokio::time::timeout(FRIST, meldungen.recv()).await,
            Ok(Some(()))
        ),
        "der Agent muss ueber die geaenderte Werkzeugliste informiert werden"
    );
    let _ = agent.cancel().await;
    p.kern.beenden().await;
}

/// Teil A: jeder Agent nur einmal, aber ein neues Token auf Wunsch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_nur_einmal_koppeln_und_token_erneuern() {
    let p = Pruefung::neu().await;

    let alt = p.kern.agent_koppeln("pi").await.expect("erster Agent");
    // Derselbe Agent noch einmal – auch anders geschrieben – geht nicht.
    for name in ["pi", " PI ", "Pi"] {
        let fehler = p.kern.agent_koppeln(name).await.expect_err("doppelt");
        assert!(
            fehler.to_string().contains("schon gekoppelt"),
            "{name}: {fehler}"
        );
    }
    assert_eq!(
        p.kern.modell().borrow().agenten.len(),
        1,
        "nur ein Eintrag für pi"
    );

    // Ein anderer Name geht weiterhin.
    p.kern.agent_koppeln("hermes").await.expect("zweiter Agent");

    // Token erneuern: neues Token, das alte gilt sofort nicht mehr.
    let id = p
        .kern
        .modell()
        .borrow()
        .agenten
        .iter()
        .find(|agent| agent.name == "pi")
        .expect("pi gekoppelt")
        .id
        .clone();
    let neu = p.kern.agent_token_erneuern(&id).await.expect("neues Token");
    assert_ne!(alt, neu, "ein neues Token ist ein anderes");
    assert_eq!(
        p.kern.modell().borrow().agenten.len(),
        2,
        "Erneuern legt keinen zweiten Agenten an"
    );

    // Am Endpunkt: das alte Token wird abgewiesen, das neue kommt durch.
    let endpunkt = p.endpunkt();
    let frage = |token: String| {
        let http = p.http.clone();
        let endpunkt = endpunkt.clone();
        async move {
            http.post(&endpunkt)
                .bearer_auth(token)
                .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }))
                .send()
                .await
                .expect("Endpunkt erreichbar")
                .status()
        }
    };
    assert_eq!(
        frage(alt).await,
        reqwest::StatusCode::UNAUTHORIZED,
        "das alte Token gilt nicht mehr"
    );
    assert_ne!(
        frage(neu).await,
        reqwest::StatusCode::UNAUTHORIZED,
        "das neue Token wird angenommen"
    );

    // Unbekannte Kennung: kein stiller Erfolg.
    assert!(p.kern.agent_token_erneuern("gibtsnicht").await.is_err());

    p.kern.beenden().await;
}

/// Hilfsfunktion: Argumente als JSON-Objekt.
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

/// Warten, bis ein Werkzeug in `tools/list` steht (innerhalb der Frist ab `beginn`).
async fn warte_auf_werkzeug(
    peer: &Peer<RoleClient>,
    name: &str,
    beginn: Instant,
) -> Option<rmcp::model::Tool> {
    loop {
        let liste = peer
            .list_tools(Some(PaginatedRequestParams::default()))
            .await
            .ok()?;
        if let Some(werkzeug) = liste.tools.into_iter().find(|tool| tool.name == name) {
            return Some(werkzeug);
        }
        if beginn.elapsed() >= FRIST {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Warten, bis ein Eintrag im Modell einen bestimmten Zustand hat.
async fn warte_auf_zustand(
    kern: &Kern,
    id: &EintragId,
    passt: impl Fn(&EintragZustand) -> bool,
) -> bool {
    let frist = Instant::now() + FRIST;
    let mut modell = kern.modell();
    loop {
        {
            let sicht = modell.borrow_and_update();
            if let Some(eintrag) = sicht.eintraege.iter().find(|eintrag| &eintrag.id == id) {
                if passt(&eintrag.zustand) {
                    return true;
                }
            }
        }
        let rest = frist.saturating_duration_since(Instant::now());
        if rest.is_zero() {
            return false;
        }
        if tokio::time::timeout(rest, modell.changed()).await.is_err() {
            return false;
        }
    }
}

/// Ein Satz mit `echo`, der Gruppe `team` zugeordnet; das Gerät ist
/// eingebunden und das Werkzeug läuft beim Agenten.
async fn eingebunden_mit_echo(p: &Pruefung) -> rmcp::service::RunningService<RoleClient, Zeuge> {
    p.put(
        "/admin/eintraege/echo",
        &eintrag("echo", Werte::new(), Vec::new()),
    )
    .await;
    p.put(
        "/admin/saetze/basis",
        &Satz {
            id: SatzId::neu("basis"),
            titel: "Basis".into(),
            prioritaet: 0,
            eintraege: vec![SatzEintrag {
                eintrag: EintragId::neu("echo"),
                werte: Werte::new(),
            }],
        },
    )
    .await;
    p.put(
        "/admin/gruppen/team",
        &Gruppe {
            id: GruppeId::neu("team"),
            titel: "Team".into(),
            saetze: vec![SatzId::neu("basis")],
            entra: Vec::new(),
        },
    )
    .await;
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
    let agent_token = p.kern.agent_koppeln("test").await.expect("Agent gekoppelt");
    let (sender, _meldungen) = tokio::sync::mpsc::unbounded_channel();
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(p.endpunkt()).auth_header(&agent_token),
    );
    let agent = Zeuge { meldungen: sender }
        .serve(transport)
        .await
        .expect("Agent verbindet sich");
    assert!(
        warte_auf_werkzeug(agent.peer(), "echo__echo", Instant::now())
            .await
            .is_some(),
        "echo__echo läuft"
    );
    agent
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_entferntes_geraet_stoppt_alle_tools() {
    let p = Pruefung::neu().await;
    let agent = eingebunden_mit_echo(&p).await;

    // Der Admin entfernt das Gerät.
    let geraete = p.admin("GET", "/admin/geraete", None).await;
    let id = geraete[0]["geraet"]["id"]
        .as_str()
        .expect("Geräte-Id")
        .to_string();
    p.admin("DELETE", &format!("/admin/geraete/{id}"), None)
        .await;

    // Innerhalb der Frist: abgelehnt, nicht mehr eingebunden, kein Eintrag.
    let frist = Instant::now() + FRIST;
    let mut modell = p.kern.modell();
    loop {
        {
            let stand = modell.borrow_and_update();
            if stand.abgelehnt {
                assert_eq!(
                    stand.verbindung,
                    quiver_client::Verbindung::NichtEingebunden
                );
                assert!(stand.eintraege.is_empty(), "kein Eintrag mehr");
                break;
            }
        }
        let rest = frist.saturating_duration_since(Instant::now());
        assert!(!rest.is_zero(), "das Gerät gilt nicht als abgelehnt");
        let _ = tokio::time::timeout(rest, modell.changed()).await;
    }
    let liste = agent
        .peer()
        .list_tools(Some(PaginatedRequestParams::default()))
        .await
        .expect("tools/list");
    assert!(
        liste
            .tools
            .iter()
            .all(|tool| !tool.name.starts_with("echo__")),
        "keine Tools mehr beim Agenten"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ohne_anmeldung_am_server_startet_kein_tool() {
    let p = Pruefung::neu().await;
    let agent = eingebunden_mit_echo(&p).await;
    drop(agent);
    p.kern.beenden().await;

    // Neustart mit demselben Profil und Tresor – aber der Server ist weg.
    let zustand = p.ordner.join("profil").join("zustand.json");
    let text = std::fs::read_to_string(&zustand).expect("zustand.json");
    assert!(text.contains("echo"), "der Soll-Zustand ist gespeichert");
    let text = text.replace(&p.basis, "http://127.0.0.1:9");
    std::fs::write(&zustand, text).expect("zustand.json");

    let zweiter = Kern::starten(KernKonfig {
        profil: p.ordner.join("profil"),
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: p.tresor.clone(),
        browser: std::sync::Arc::new(quiver_client::SystemBrowser),
    })
    .await
    .expect("Kern startet");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let modell = zweiter.modell().borrow().clone();
    assert!(
        modell
            .eintraege
            .iter()
            .all(|eintrag| !matches!(eintrag.zustand, EintragZustand::Laeuft { .. })),
        "ohne Anmeldung am Server läuft nichts: {:?}",
        modell.eintraege
    );
    assert!(!modell.abgelehnt, "nicht erreichbar ist nicht abgelehnt");
    zweiter.beenden().await;
}
