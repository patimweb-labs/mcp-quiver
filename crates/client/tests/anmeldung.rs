// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Anmeldung eines Eintrags im Client (Aufgabe 10, ausgeführt für
//! `docs/aufgaben/10-anmeldung-im-fenster.md`).
//!
//! Der Ablauf ist der aus der Aufgabe:
//! 1. Ein Eintrag braucht eine Anmeldung; beim Start prüft der Client den
//!    `status`-Befehl → `Abgemeldet`.
//! 2. `Kern::anmelden` führt den `anmelden`-Befehl aus (hier die Attrappe
//!    `quiver-anmelde-attrappe`), prüft den Status und startet den Eintrag
//!    **neu** → `Angemeldet { konto }`, und der MCP-Prozess wurde ein zweites
//!    Mal gestartet (nachzählbar im Protokoll des Demo-Servers).
//! 3. `Kern::abmelden_eintrag` meldet ab → wieder `Abgemeldet`.
//!
//! Die Attrappe bekommt ihre Umgebung aus der Laufzeit des Eintrags, und ihr
//! Konto kommt aus einem `${feld}`-Platzhalter – beides wird hier geprüft.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use quiver_client::{AnmeldeZustand, Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{
    Anmeldung, Eintrag, EintragId, Feld, Gruppe, GruppeId, Herkunft, Laufzeit, Quelle, Satz,
    SatzEintrag, SatzId, Werte,
};
use quiver_protocol::EintragZustand;
use quiver_server::{app, Konfig};
use serde_json::{json, Value};

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// Wie lange auf einen Zustand gewartet wird.
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
    /// Wohin der Demo-Server jede Zeile schreibt („gestartet …").
    protokoll: PathBuf,
    /// Der Zustand der Anmelde-Attrappe.
    ablage: PathBuf,
}

impl Pruefung {
    async fn neu() -> Self {
        // Zähler **und** Zeitstempel: zwei Tests im selben Prozess dürfen sich
        // niemals denselben Ordner teilen (sonst zwei SQLite auf einer Datei).
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "quiver-client-anmeldung-{}-{nummer}-{stempel}",
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

        let kern = Kern::starten(KernKonfig {
            profil: ordner.join("profil"),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: std::sync::Arc::new(SpeicherTresor::neu()),
            browser: std::sync::Arc::new(quiver_client::SystemBrowser),
            vorgaben: quiver_client::Vorgaben::default(),
        })
        .await
        .expect("Kern startet");

        Self {
            protokoll: ordner.join("protokoll.log"),
            ablage: ordner.join("anmeldung.stand"),
            ordner,
            basis,
            cookie,
            kern,
            http,
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

    /// Wie oft der Demo-Server gestartet wurde – ein Neustart ist ein Start mehr.
    fn starts(&self) -> usize {
        std::fs::read_to_string(&self.protokoll)
            .unwrap_or_default()
            .lines()
            .filter(|zeile| zeile.contains("gestartet"))
            .count()
    }

    /// Warten, bis der Eintrag im Modell läuft und seine Werkzeuge kennt.
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

    /// Warten, bis der Eintrag im Modell einen bestimmten Anmeldezustand hat.
    async fn warte_auf_anmeldung(
        &self,
        id: &EintragId,
        erwartet: fn(&AnmeldeZustand) -> bool,
    ) -> AnmeldeZustand {
        let frist = Instant::now() + FRIST;
        let mut modell = self.kern.modell();
        loop {
            let zustand = modell
                .borrow_and_update()
                .eintraege
                .iter()
                .find(|eintrag| &eintrag.id == id)
                .and_then(|eintrag| eintrag.anmeldung.clone());
            if let Some(gefunden) = &zustand {
                if erwartet(gefunden) {
                    return gefunden.clone();
                }
            }
            let rest = frist.saturating_duration_since(Instant::now());
            assert!(!rest.is_zero(), "Anmeldezustand bleibt aus: {zustand:?}");
            let _ = tokio::time::timeout(rest, modell.changed()).await;
        }
    }
}

impl Drop for Pruefung {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

// --------------------------------------------------------------- Testdaten

fn demo_bin() -> String {
    env!("CARGO_BIN_EXE_quiver-demo-mcp").to_string()
}

fn attrappe_bin() -> String {
    env!("CARGO_BIN_EXE_quiver-anmelde-attrappe").to_string()
}

fn feld(name: &str, quelle: Quelle) -> Feld {
    Feld {
        name: name.into(),
        beschreibung: format!("Beschreibung {name}"),
        quelle,
        pflicht: true,
        standard: None,
        link: None,
        ..Default::default()
    }
}

/// Ein Eintrag mit Anmeldung: die Attrappe ist **ein** Programm, das den
/// MCP-Server startet (`mcp`) und die Anmeldung steuert – so wie ein echtes
/// Werkzeug. Alle Befehle bekommen dieselbe Umgebung, die Attrappe findet
/// ihre Datei über `${ABLAGE}`, ihr Konto über `${KONTO}`.
fn eintrag(p: &Pruefung) -> Eintrag {
    Eintrag {
        id: EintragId::neu("postfach"),
        version: "1.0.0".into(),
        titel: "Postfach".into(),
        beschreibung: "Mail lesen, mit Anmeldung im Browser.".into(),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: attrappe_bin(),
            argumente: vec!["mcp".to_string()],
            umgebung: Werte::from([
                ("QUIVER_ATTRAPPE_MCP".to_string(), demo_bin()),
                (
                    "QUIVER_DEMO_TOOLS".to_string(),
                    "lesen=Liest eine Mail".to_string(),
                ),
                (
                    "QUIVER_DEMO_LOG".to_string(),
                    p.protokoll.to_string_lossy().to_string(),
                ),
                ("QUIVER_ATTRAPPE_DATEI".to_string(), "${ABLAGE}".to_string()),
            ]),
        },
        felder: vec![
            feld("ABLAGE", Quelle::Server),
            feld("KONTO", Quelle::Server),
        ],
        aktiv: true,
        anmeldung: Some(Anmeldung {
            anmelden: vec!["anmelden".into(), "${KONTO}".into()],
            status: vec!["status".into()],
            abmelden: vec!["abmelden".into()],
        }),
        einrichtung: None,
        mehrfach: None,
    }
}

fn angemeldet(zustand: &AnmeldeZustand) -> bool {
    matches!(zustand, AnmeldeZustand::Angemeldet { .. })
}

fn abgemeldet(zustand: &AnmeldeZustand) -> bool {
    matches!(zustand, AnmeldeZustand::Abgemeldet)
}

// ------------------------------------------------------------------ Der Test

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anmelden_im_fenster_startet_den_eintrag_neu() {
    let p = Pruefung::neu().await;

    // ------------------------------------------------ 1. Einrichten beim Admin
    p.put("/admin/eintraege/postfach", &eintrag(&p)).await;
    p.put(
        "/admin/saetze/basis",
        &Satz {
            id: SatzId::neu("basis"),
            titel: "Basis".into(),
            prioritaet: 0,
            eintraege: vec![SatzEintrag {
                eintrag: EintragId::neu("postfach"),
                werte: Werte::from([
                    ("ABLAGE".to_string(), p.ablage.to_string_lossy().to_string()),
                    ("KONTO".to_string(), "test@beispiel.de".to_string()),
                ]),
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

    // ------------------------- 2. Beim Start: Status wird geprüft (Abgemeldet)
    let id = EintragId::neu("postfach");
    assert_eq!(
        p.warte_auf_anmeldung(&id, abgemeldet).await,
        AnmeldeZustand::Abgemeldet,
        "ohne Anmeldung ist der Eintrag abgemeldet"
    );
    // Der Eintrag selbst läuft (die Statusprüfung hält ihn nicht auf).
    let eintrag_sicht = p.warte_auf_laeuft(&id).await;
    assert_eq!(eintrag_sicht.tools, vec!["lesen".to_string()]);
    let vorher = p.starts();
    assert!(vorher >= 1, "der Demo-Server läuft");

    // ------------------------------------------- 3. Anmelden im Fenster
    p.kern.anmelden(&id).await.expect("Anmeldung läuft durch");

    assert_eq!(
        p.warte_auf_anmeldung(&id, angemeldet).await,
        AnmeldeZustand::Angemeldet {
            konto: Some("test@beispiel.de".to_string())
        },
        "das Konto kommt aus der ersten Zeile von stdout"
    );
    // Die Attrappe hat den Platzhalter `${KONTO}` gesehen, nicht den Namen.
    assert_eq!(
        std::fs::read_to_string(&p.ablage)
            .expect("Attrappe hat die Ablage geschrieben")
            .trim(),
        "test@beispiel.de",
        "der Platzhalter KONTO wird wie in der Laufzeit ersetzt"
    );

    // Der Eintrag wurde neu gestartet, damit der laufende Prozess das neue
    // Token sieht: ein Start mehr im Protokoll des Demo-Servers.
    let frist = Instant::now() + FRIST;
    while p.starts() <= vorher {
        assert!(
            Instant::now() < frist,
            "der Eintrag wurde nicht neu gestartet ({} Starts)",
            p.starts()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Und er läuft danach wieder mit seinen Werkzeugen.
    let frist = Instant::now() + FRIST;
    loop {
        let stand = p.kern.modell().borrow().clone();
        let sicht = stand
            .eintraege
            .iter()
            .find(|eintrag| eintrag.id == id)
            .cloned()
            .expect("der Eintrag steht im Modell");
        if matches!(sicht.zustand, EintragZustand::Laeuft { .. }) && sicht.tools == ["lesen"] {
            break;
        }
        assert!(Instant::now() < frist, "der Eintrag läuft nicht wieder");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // ------------------------------------------------- 4. Wieder abmelden
    p.kern
        .abmelden_eintrag(&id)
        .await
        .expect("Abmelden läuft durch");
    assert_eq!(
        p.warte_auf_anmeldung(&id, abgemeldet).await,
        AnmeldeZustand::Abgemeldet
    );
    assert!(
        std::fs::read_to_string(&p.ablage)
            .expect("Ablage lesbar")
            .trim()
            .is_empty(),
        "abgemeldet heißt: kein Konto mehr"
    );

    p.kern.beenden().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ohne_anmeldung_im_eintrag_gibt_es_keine_anmeldung() {
    let p = Pruefung::neu().await;
    let mut ohne = eintrag(&p);
    ohne.anmeldung = None;
    p.put("/admin/eintraege/postfach", &ohne).await;
    p.put(
        "/admin/saetze/basis",
        &Satz {
            id: SatzId::neu("basis"),
            titel: "Basis".into(),
            prioritaet: 0,
            eintraege: vec![SatzEintrag {
                eintrag: EintragId::neu("postfach"),
                werte: Werte::from([
                    ("ABLAGE".to_string(), p.ablage.to_string_lossy().to_string()),
                    ("KONTO".to_string(), "test@beispiel.de".to_string()),
                ]),
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

    let id = EintragId::neu("postfach");
    let frist = Instant::now() + FRIST;
    loop {
        let stand = p.kern.modell().borrow().clone();
        if let Some(sicht) = stand.eintraege.iter().find(|eintrag| eintrag.id == id) {
            assert_eq!(sicht.anmeldung, None, "ohne Anmeldung im Eintrag");
            break;
        }
        assert!(Instant::now() < frist, "der Eintrag kommt nicht an");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let fehler = p
        .kern
        .anmelden(&id)
        .await
        .expect_err("ohne Anmeldung gibt es nichts anzumelden");
    assert!(
        fehler.to_string().contains("Anmeldung"),
        "klare Meldung: {fehler}"
    );

    p.kern.beenden().await;
}
