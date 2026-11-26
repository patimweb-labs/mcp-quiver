// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Einträge am Gerät abschalten (Aufgabe 19,
//! `docs/aufgaben/19-am-geraet-abschalten.md`).
//!
//! Der Ablauf ist der aus der Aufgabe, Ende zu Ende mit echtem Server und
//! Demo-MCP:
//! 1. **Aus:** Der Prozess wird beendet, die Werkzeuge verschwinden in unter
//!    einer Sekunde aus `tools/list`, beide Agenten bekommen `list_changed`,
//!    und ein Aufruf danach endet mit der klaren Meldung.
//! 2. **Aus bleibt aus:** Neustart, Änderung am Server, `werte_setzen`,
//!    Anmelden, Einrichten und Import starten nichts.
//! 3. **An:** läuft sofort wieder – genau ein Prozess, auch bei 50-maligem
//!    Schalten mit Abgleich dazwischen.
//! 4. **Speicherfehler:** nichts wird geschaltet.
//! 5. Der Server bekommt `am-geraet-aus` gemeldet (kein Fehler).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use quiver_client::{Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{
    Anmeldung, Eintrag, EintragId, Gruppe, GruppeId, Herkunft, Laufzeit, Satz, SatzEintrag, SatzId,
    Werte,
};
use quiver_protocol::EintragZustand;
use quiver_server::{app, Konfig};
use rmcp::model::{CallToolRequestParams, ClientConfig, Implementation, PaginatedRequestParams};
use rmcp::service::NotificationContext;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{ClientHandler, Peer, RoleClient, ServiceExt};
use serde_json::{json, Value};

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// Wie lange auf einen Zustand gewartet wird.
const FRIST: Duration = Duration::from_secs(10);
/// So schnell muss „aus“ wirken.
const SCHNELL: Duration = Duration::from_secs(1);

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
    profil: PathBuf,
    /// Je laufendem Demo-Prozess eine Datei mit seiner PID (`QUIVER_DEMO_PID`).
    pids: PathBuf,
    /// Wohin der Demo-Server jede Zeile schreibt.
    protokoll: PathBuf,
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
            "quiver-client-abschalten-{}-{nummer}-{stempel}",
            std::process::id()
        ));
        let daten = ordner.join("server");
        std::fs::create_dir_all(&daten).expect("Ordner");
        let pids = ordner.join("pids");
        std::fs::create_dir_all(&pids).expect("Ordner");

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

        let profil = ordner.join("profil");
        let tresor = Arc::new(SpeicherTresor::neu());
        let kern = Kern::starten(KernKonfig {
            profil: profil.clone(),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: tresor.clone(),
            browser: std::sync::Arc::new(quiver_client::SystemBrowser),
            vorgaben: quiver_client::Vorgaben::default(),
            aufstellung: quiver_client::update::Aufstellung::nicht_tauschen(),
        })
        .await
        .expect("Kern startet");

        Self {
            protokoll: ordner.join("protokoll.log"),
            pids,
            profil,
            ordner,
            basis,
            cookie,
            kern,
            http,
            tresor,
        }
    }

    /// Einen neuen Kern auf demselben Profil – so sieht ein Neustart aus.
    async fn kern_neu(&mut self) {
        self.kern.beenden().await;
        self.kern = Kern::starten(KernKonfig {
            profil: self.profil.clone(),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: self.tresor.clone(),
            browser: std::sync::Arc::new(quiver_client::SystemBrowser),
            vorgaben: quiver_client::Vorgaben::default(),
            aufstellung: quiver_client::update::Aufstellung::nicht_tauschen(),
        })
        .await
        .expect("Kern startet neu");
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

    /// Ein Eintrag, ein Satz, eine Gruppe – und das Gerät bindet sich ein.
    async fn einbinden(&self, eintrag: Eintrag) {
        self.put(&format!("/admin/eintraege/{}", eintrag.id), &eintrag)
            .await;
        self.satz_zuordnen(std::slice::from_ref(&eintrag.id)).await;
        self.einbinden_code().await;
    }

    async fn satz_zuordnen(&self, ids: &[EintragId]) {
        self.put(
            "/admin/saetze/basis",
            &Satz {
                id: SatzId::neu("basis"),
                titel: "Basis".into(),
                prioritaet: 0,
                eintraege: ids
                    .iter()
                    .map(|id| SatzEintrag {
                        eintrag: id.clone(),
                        werte: Werte::new(),
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
    }

    async fn einbinden_code(&self) {
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

    /// Ein Agent am Endpunkt dieses Geräts; er sammelt `list_changed`.
    async fn agent(&self, name: &str) -> (rmcp::service::RunningService<RoleClient, Zeuge>, Zeuge) {
        let token = self
            .kern
            .agent_koppeln(name)
            .await
            .expect("Agent gekoppelt");
        let (sender, empfänger) = tokio::sync::mpsc::unbounded_channel();
        let zeuge = Zeuge {
            meldungen: sender,
            empfänger: Arc::new(std::sync::Mutex::new(None)),
        };
        let transporte = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(self.endpunkt()).auth_header(&token),
        );
        let dienst = zeuge
            .clone()
            .serve(transporte)
            .await
            .expect("Agent verbindet sich");
        // Der Empfänger bleibt beim Zeugen; der Aufrufer nimmt ihn über den Zeugen.
        zeuge.empfaenger_setzen(empfänger);
        (dienst, zeuge)
    }

    /// Warten, bis der Eintrag im Modell läuft und seine Werkzeuge kennt.
    async fn warte_auf_laeuft(&self, id: &EintragId) {
        self.warte_auf("läuft", |sicht| {
            &sicht.id == id
                && matches!(sicht.zustand, EintragZustand::Laeuft { .. })
                && !sicht.tools.is_empty()
        })
        .await;
    }

    /// Warten, bis ein Eintrag im Modell eine Bedingung erfüllt.
    async fn warte_auf(&self, was: &str, bedingung: impl Fn(&quiver_client::EintragSicht) -> bool) {
        let frist = Instant::now() + FRIST;
        let mut modell = self.kern.modell();
        while Instant::now() < frist {
            let stand = modell.borrow_and_update().clone();
            if stand.eintraege.iter().any(&bedingung) {
                return;
            }
            let rest = frist.saturating_duration_since(Instant::now());
            let _ = tokio::time::timeout(rest, modell.changed()).await;
        }
        panic!(
            "„{was}“ stellt sich nicht ein: {:?}",
            self.kern.modell().borrow().eintraege
        );
    }

    /// Wie viele Demo-Prozesse wirklich laufen (PID-Dateien des Demo-MCP).
    fn prozesse(&self) -> usize {
        let Ok(eintraege) = std::fs::read_dir(&self.pids) else {
            return 0;
        };
        eintraege
            .flatten()
            .filter_map(|eintrag| eintrag.file_name().to_string_lossy().parse::<u32>().ok())
            .filter(|pid| laeuft(*pid))
            .count()
    }

    /// Warten, bis genau `erwartet` Demo-Prozesse laufen.
    async fn warte_auf_prozesse(&self, erwartet: usize, frist: Duration) {
        let ende = Instant::now() + frist;
        loop {
            let jetzt = self.prozesse();
            if jetzt == erwartet {
                return;
            }
            assert!(
                Instant::now() < ende,
                "{jetzt} Prozesse statt {erwartet} nach {frist:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Die gemeldeten Zustände des Geräts am Server.
    async fn meldung(&self) -> Value {
        let geraete = self.admin("GET", "/admin/geraete", None).await;
        geraete
            .as_array()
            .and_then(|liste| liste.first())
            .and_then(|stand| stand.get("meldung"))
            .and_then(|meldung| meldung.get("eintraege"))
            .cloned()
            .unwrap_or(Value::Null)
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

/// Der Test-Agent: sammelt „Werkzeugliste geändert“.
#[derive(Clone)]
struct Zeuge {
    meldungen: tokio::sync::mpsc::UnboundedSender<()>,
    /// Der Empfänger ist nicht klonbar – er liegt hinter einem Mutex.
    empfänger: Arc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<()>>>>,
}

impl Zeuge {
    fn empfaenger_setzen(&self, empfänger: tokio::sync::mpsc::UnboundedReceiver<()>) {
        *self.empfänger.lock().unwrap_or_else(|e| e.into_inner()) = Some(empfänger);
    }

    /// Warten, bis `list_changed` kommt.
    async fn warte_auf_list_changed(&self) {
        let mut empfänger = self
            .empfänger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .expect("Empfänger gesetzt");
        let gemeldet = tokio::time::timeout(SCHNELL, empfänger.recv()).await;
        *self.empfänger.lock().unwrap_or_else(|e| e.into_inner()) = Some(empfänger);
        assert!(
            matches!(gemeldet, Ok(Some(()))),
            "innerhalb von {SCHNELL:?} muss notifications/tools/list_changed kommen"
        );
    }
}

impl ClientHandler for Zeuge {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            rmcp::model::ClientCapabilities::default(),
            Implementation::new("quiver-abschalt-zeuge", "0.1.0"),
        )
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        let _ = self.meldungen.send(());
    }
}

/// Läuft dieser Prozess noch? (`kill -0`, ohne zusätzliche Abhängigkeit.)
#[cfg(unix)]
fn laeuft(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Windows: `tasklist` fragt nach der PID. Der Programmname muss passen, sonst
/// zählte ein fremder Prozess mit wiederverwendeter PID. Scheitert die Abfrage,
/// ist das ein Testfehler und kein „läuft nicht“.
#[cfg(windows)]
fn laeuft(pid: u32) -> bool {
    let ausgabe = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .expect("tasklist");
    assert!(ausgabe.status.success(), "tasklist: {}", ausgabe.status);
    String::from_utf8_lossy(&ausgabe.stdout)
        .to_ascii_lowercase()
        .contains("quiver-demo-mcp")
}

// --------------------------------------------------------------- Testdaten

fn demo_bin() -> String {
    env!("CARGO_BIN_EXE_quiver-demo-mcp").to_string()
}

/// Der Demo-MCP, mit PID-Ablage und Protokoll.
fn eintrag(p: &Pruefung, id: &str, titel: &str) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "0.1.0".into(),
        titel: titel.to_string(),
        beschreibung: format!("Beschreibung {titel}"),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: demo_bin(),
            argumente: Vec::new(),
            umgebung: Werte::from([
                (
                    "QUIVER_DEMO_TOOLS".to_string(),
                    "echo=Gibt den Text zurück;ping=Antwortet".to_string(),
                ),
                ("QUIVER_DEMO_NAME".to_string(), id.to_string()),
                (
                    "QUIVER_DEMO_LOG".to_string(),
                    p.protokoll.to_string_lossy().to_string(),
                ),
                (
                    "QUIVER_DEMO_PID".to_string(),
                    p.pids.to_string_lossy().to_string(),
                ),
            ]),
        },
        felder: Vec::new(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
    }
}

/// Ein Werkzeug aufrufen; das Ergebnis ist Absicht (Fehler sind erlaubt).
async fn aufrufen(
    peer: &Peer<RoleClient>,
    name: &str,
) -> Result<rmcp::model::CallToolResult, rmcp::service::ServiceError> {
    let argumente: rmcp::model::JsonObject =
        serde_json::from_value(json!({ "text": "hallo" })).expect("JSON-Objekt");
    peer.call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(argumente))
        .await
}

/// Stehen die Werkzeuge eines Eintrags in `tools/list`?
async fn bietet_an(peer: &Peer<RoleClient>, eintrag: &str) -> bool {
    peer.list_tools(Some(PaginatedRequestParams::default()))
        .await
        .expect("tools/list")
        .tools
        .iter()
        .any(|tool| tool.name.starts_with(&format!("{eintrag}__")))
}

/// Der Fehlertext eines gescheiterten Aufrufs.
fn fehlertext(fehler: &rmcp::service::ServiceError) -> String {
    format!("{fehler:?}")
}

// ------------------------------------------------------------------ Die Tests

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aus_beendet_den_prozess_und_an_startet_ihn_wieder() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.einbinden(eintrag(&p, "echo", "Echo")).await;
    p.warte_auf_laeuft(&id).await;
    p.warte_auf_prozesse(1, FRIST).await;

    let (agent_a, zeuge_a) = p.agent("pi").await;
    let (_agent_b, zeuge_b) = p.agent("hermes").await;
    assert!(bietet_an(agent_a.peer(), "echo").await);

    // ------------------------------------------------------------------ Aus
    let beginn = Instant::now();
    p.kern
        .eintrag_schalten(&id, false)
        .await
        .expect("abschaltbar");
    let sicht = p
        .kern
        .modell()
        .borrow()
        .eintraege
        .iter()
        .find(|eintrag| eintrag.id == id)
        .cloned()
        .expect("der Eintrag steht im Modell");
    assert!(sicht.am_geraet_aus, "die Wahl steht im Modell");
    assert_eq!(sicht.zustand, EintragZustand::AmGeraetAus);
    assert!(sicht.tools.is_empty(), "keine Werkzeuge mehr");
    assert!(
        beginn.elapsed() < SCHNELL,
        "„aus“ muss sofort wirken, nicht nach {:?}",
        beginn.elapsed()
    );
    // Der Prozess ist beendet – in unter einer Sekunde.
    p.warte_auf_prozesse(0, SCHNELL).await;

    // Beide Agenten bekommen `list_changed` und sehen das Werkzeug nicht mehr.
    zeuge_a.warte_auf_list_changed().await;
    zeuge_b.warte_auf_list_changed().await;
    assert!(
        !bietet_an(agent_a.peer(), "echo").await,
        "die Werkzeuge sind aus tools/list verschwunden"
    );

    // Ein Aufruf danach endet mit der klaren Meldung – nie mit „unbekannt“.
    let fehler = aufrufen(agent_a.peer(), "echo__echo")
        .await
        .expect_err("ein abgeschalteter Eintrag antwortet nicht");
    let text = fehlertext(&fehler);
    assert!(text.contains("abgeschaltet"), "{text}");
    assert!(
        text.contains("quiver-Fenster unter Tools"),
        "die Meldung sagt, wo man es einschaltet: {text}"
    );
    assert!(!text.contains("unbekannt"), "{text}");

    // Der Server bekommt die Wahl gemeldet – als eigener Zustand.
    let gemeldet = p.meldung().await;
    assert!(
        gemeldet.to_string().contains("am-geraet-aus"),
        "der Server sieht „am Gerät abgeschaltet“: {gemeldet}"
    );

    // ------------------------------------------------------------------- An
    p.kern
        .eintrag_schalten(&id, true)
        .await
        .expect("einschaltbar");
    p.warte_auf_laeuft(&id).await;
    p.warte_auf_prozesse(1, FRIST).await;
    zeuge_a.warte_auf_list_changed().await;
    assert!(
        bietet_an(agent_a.peer(), "echo").await,
        "mit „an“ ist das Werkzeug wieder da"
    );
    let antwort = aufrufen(agent_a.peer(), "echo__echo")
        .await
        .expect("der Aufruf geht wieder");
    assert!(format!("{antwort:?}").contains("hallo"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aus_bleibt_aus_ueber_neustart_server_werte_und_import() {
    let mut p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.einbinden(eintrag(&p, "echo", "Echo")).await;
    p.warte_auf_laeuft(&id).await;
    p.kern
        .eintrag_schalten(&id, false)
        .await
        .expect("abschaltbar");
    p.warte_auf_prozesse(0, SCHNELL).await;

    // 1. `werte_setzen` startet nichts.
    p.kern
        .werte_setzen(
            &id,
            Werte::from([("PFAD".to_string(), "/tmp/echo".to_string())]),
            Werte::new(),
        )
        .await
        .expect("Werte setzbar");
    // 2. Anmelden und Einrichten sind gesperrt (der Eintrag bietet beides nicht
    //    an – der Kern weist es trotzdem ab, bevor ein Befehl läuft).
    assert!(p.kern.anmelden(&id).await.is_err());
    assert!(p.kern.abmelden_eintrag(&id).await.is_err());
    assert!(p.kern.einrichten(&id).await.is_err());

    // 3. Der Server ändert den Eintrag (neue Version) → bleibt aus.
    let mut geaendert = eintrag(&p, "echo", "Echo");
    geaendert.version = "0.2.0".into();
    geaendert.beschreibung = "Neu vom Server".into();
    p.put("/admin/eintraege/echo", &geaendert).await;
    p.warte_auf("die neue Version kommt an", |sicht| {
        sicht.id == id && sicht.beschreibung == "Neu vom Server"
    })
    .await;
    p.warte_auf_prozesse(0, FRIST).await;

    // 4. Import: die Datei sagt „an“, das Gerät sagt „aus“ – aus bleibt aus.
    let datei = p.ordner.join("sicherung.json");
    std::fs::write(
        &datei,
        serde_json::to_string_pretty(&json!({
            "format": "mcp-quiver/einstellungen",
            "version": 1,
            "erstellt": "2026-11-23T10:11:12Z",
            "eintraege": { "echo": { "werte": { "PFAD": "/tmp/echo" }, "am_geraet_aus": false } },
            "geheim": null
        }))
        .expect("JSON"),
    )
    .expect("schreibbar");
    p.kern
        .einstellungen_importieren(&datei, None)
        .await
        .expect("Import klappt");
    p.warte_auf_prozesse(0, FRIST).await;

    // Die Sicherung nimmt die Wahl mit.
    let sicherung = p.ordner.join("export.json");
    p.kern
        .einstellungen_exportieren(&sicherung, None)
        .expect("Export klappt");
    let text = std::fs::read_to_string(&sicherung).expect("lesbar");
    assert!(
        text.contains("\"am_geraet_aus\": true"),
        "die Wahl gehört in die Sicherung: {text}"
    );
    let vorschau = p
        .kern
        .einstellungen_pruefen(&sicherung, None)
        .expect("prüfbar");
    assert!(
        vorschau
            .eintraege
            .iter()
            .any(|eintrag| eintrag.am_geraet_aus),
        "die Vorschau zeigt die Wahl: {vorschau:?}"
    );

    // 5. Neustart: der gespeicherte Stand ist abgeschaltet, und das bleibt so.
    p.kern_neu().await;
    p.warte_auf("der Eintrag ist wieder im Soll", |sicht| sicht.id == id)
        .await;
    p.warte_auf("der Eintrag bleibt abgeschaltet", |sicht| {
        sicht.id == id && sicht.am_geraet_aus
    })
    .await;
    // Ein Moment Zeit: nichts darf starten.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(p.prozesse(), 0, "nach dem Neustart läuft nichts");
    assert!(
        p.kern
            .modell()
            .borrow()
            .eintraege
            .iter()
            .any(|sicht| sicht.id == id && sicht.zustand == EintragZustand::AmGeraetAus),
        "auch nach dem Neustart steht „abgeschaltet“ im Modell"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn schnelles_schalten_mit_abgleich_laesst_genau_einen_prozess() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.einbinden(eintrag(&p, "echo", "Echo")).await;
    p.warte_auf_laeuft(&id).await;

    // 50-mal an/aus mit einem Abgleich dazwischen (der Server ändert den
    // Eintrag, der Client gleicht ab).
    for nummer in 0..50 {
        p.kern.eintrag_schalten(&id, false).await.expect("aus");
        p.kern.eintrag_schalten(&id, true).await.expect("an");
        let mut geaendert = eintrag(&p, "echo", "Echo");
        geaendert.version = format!("0.1.{nummer}");
        p.put("/admin/eintraege/echo", &geaendert).await;
        // Kurz Luft lassen, damit sich Start und Abgleich überlappen können.
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    p.kern.eintrag_schalten(&id, true).await.expect("an");
    p.warte_auf_laeuft(&id).await;
    // Genau ein Prozess – ein überholter Start darf keinen übrig lassen.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(p.prozesse(), 1, "genau ein Prozess nach 50 Schaltvorgängen");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_speicherfehler_schaltet_nichts() {
    use std::os::unix::fs::PermissionsExt;

    let p = Pruefung::neu().await;
    let id = EintragId::neu("echo");
    p.einbinden(eintrag(&p, "echo", "Echo")).await;
    p.warte_auf_laeuft(&id).await;
    p.warte_auf_prozesse(1, FRIST).await;

    // Das Profil lässt sich nicht mehr schreiben.
    std::fs::set_permissions(&p.profil, std::fs::Permissions::from_mode(0o555))
        .expect("Rechte setzbar");
    let fehler = p
        .kern
        .eintrag_schalten(&id, false)
        .await
        .expect_err("ohne Speichern wird nicht geschaltet");
    assert!(
        format!("{fehler:#}").to_lowercase().contains("permission"),
        "der Fehler nennt den Grund: {fehler:#}"
    );

    // Nichts ist geschaltet: der Eintrag läuft weiter, der Prozess auch.
    let sicht = p
        .kern
        .modell()
        .borrow()
        .eintraege
        .iter()
        .find(|eintrag| eintrag.id == id)
        .cloned()
        .expect("der Eintrag steht im Modell");
    assert!(!sicht.am_geraet_aus, "nichts wurde geschaltet");
    assert!(
        matches!(sicht.zustand, EintragZustand::Laeuft { .. }),
        "der Eintrag läuft weiter: {:?}",
        sicht.zustand
    );
    assert_eq!(p.prozesse(), 1, "der Prozess lebt weiter");

    // Aufräumen, damit das Verzeichnis löschbar bleibt.
    std::fs::set_permissions(&p.profil, std::fs::Permissions::from_mode(0o755)).ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_eintrag_mit_anmeldung_startet_abgeschaltet_nie() {
    let p = Pruefung::neu().await;
    let mut postfach = eintrag(&p, "postfach", "Postfach");
    postfach.anmeldung = Some(Anmeldung {
        anmelden: vec!["anmelden".into()],
        status: vec!["status".into()],
        abmelden: vec!["abmelden".into()],
    });
    p.einbinden(postfach).await;
    let id = EintragId::neu("postfach");
    p.warte_auf_laeuft(&id).await;
    p.kern.eintrag_schalten(&id, false).await.expect("aus");
    p.warte_auf_prozesse(0, SCHNELL).await;

    // Die Anmeldung ist gesperrt, solange der Eintrag aus ist – kein Befehl
    // läuft, kein Prozess entsteht.
    let fehler = p.kern.anmelden(&id).await.expect_err("gesperrt");
    assert!(format!("{fehler:#}").contains("abgeschaltet"), "{fehler:#}");
    assert_eq!(p.prozesse(), 0, "kein Prozess für die Anmeldung");
}
