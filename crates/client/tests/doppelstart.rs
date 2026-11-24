// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Doppelstart eines Eintrags (Aufgabe 12,
//! `docs/aufgaben/12-doppelstart-und-log.md`, Teil A).
//!
//! Der beobachtete Fehler: zwei Start-Aufgaben für denselben Eintrag liefen
//! überlappend (damals: gespeicherter Soll beim Start, kurz darauf der Soll
//! vom Server); die ältere schrieb nach dem Aufbau in den Läufer der jüngeren
//! und leerte am Ende dessen Werkzeugliste. Der Eintrag lief, aber die
//! Agenten sahen kein Werkzeug.
//!
//! Heute startet nichts mehr aus dem gespeicherten Stand (erst nach der
//! Anmeldung am Server). Das Überlappen gibt es trotzdem: der erste Start
//! kommt hier vom Server, der zweite von neuen lokalen Werten.
//!
//! Der Test macht das Überlappen fest: der erste Start hängt in der
//! Startphase (`QUIVER_DEMO_WARTE_AUF`), der zweite läuft daneben fertig, erst
//! danach wird der erste freigegeben. Er darf die Werkzeugliste nicht mehr
//! anfassen.
//!
//! Der Test läuft mehrfach hintereinander: ein Fehler, der von einer
//! Reihenfolge abhängt, soll nicht einmal zufällig grün sein.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quiver_client::{EintragSicht, Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{Eintrag, EintragId, Herkunft, Laufzeit, Soll, SollEintrag, Werte};
use quiver_protocol::EintragZustand;

/// Wie oft der Ablauf wiederholt wird.
const DURCHLAEUFE: usize = 5;
/// Wie lange auf einen Zustand gewartet wird.
const FRIST: Duration = Duration::from_secs(10);

// ------------------------------------------------------------------ Gerüst

struct Geruest {
    ordner: PathBuf,
    profil: PathBuf,
    /// Wohin der Demo-Server jede Zeile schreibt.
    protokoll: PathBuf,
    /// Existiert diese Datei beim Start, wartet der Demo-Server.
    arm: PathBuf,
    /// Auf diese Datei wartet der Demo-Server dann.
    frei: PathBuf,
    soll: Soll,
}

impl Geruest {
    fn neu() -> Self {
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "quiver-client-doppelstart-{}-{stempel}",
            std::process::id()
        ));
        std::fs::create_dir_all(&ordner).expect("Ordner");

        let protokoll = ordner.join("protokoll.log");
        let arm = ordner.join("arm");
        let frei = ordner.join("frei");

        let eintrag = Eintrag {
            id: EintragId::neu("outline"),
            version: "1.0.0".into(),
            titel: "Outline".into(),
            beschreibung: "Wikis verwalten.".into(),
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: env!("CARGO_BIN_EXE_quiver-demo-mcp").to_string(),
                argumente: Vec::new(),
                umgebung: Werte::from([
                    ("QUIVER_DEMO_NAME".to_string(), "outline".to_string()),
                    (
                        "QUIVER_DEMO_TOOLS".to_string(),
                        "lesen=Liest eine Seite".to_string(),
                    ),
                    (
                        "QUIVER_DEMO_LOG".to_string(),
                        protokoll.to_string_lossy().to_string(),
                    ),
                    (
                        "QUIVER_DEMO_WARTE_AUF".to_string(),
                        arm.to_string_lossy().to_string(),
                    ),
                    (
                        "QUIVER_DEMO_FREIGABE".to_string(),
                        frei.to_string_lossy().to_string(),
                    ),
                ]),
            },
            felder: Vec::new(),
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
            mehrfach: None,
        };
        let soll = Soll::neu(vec![SollEintrag {
            eintrag,
            werte: Werte::new(),
        }]);

        Self {
            profil: ordner.join("profil"),
            ordner,
            protokoll,
            arm,
            frei,
            soll,
        }
    }

    /// Ein Server mit diesem Eintrag in Gruppe `team`, dazu ein Kern, der
    /// sich dort einbindet – der erste Start kommt also vom Server.
    async fn kern(&self) -> Kern {
        const ADMIN: &str = "admin-token-fuer-den-doppelstart-test";
        let router = quiver_server::app(quiver_server::Konfig {
            daten: self.ordner.join("server"),
            admin_token: ADMIN.to_string(),
            admin_dist: None,
        })
        .await
        .expect("App baut");
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let http = reqwest::Client::new();
        let admin = |methode: &str, pfad: &str, rumpf: serde_json::Value| {
            http.request(
                methode.parse().expect("Methode"),
                format!("{basis}/api/v1{pfad}"),
            )
            .bearer_auth(ADMIN)
            .json(&rumpf)
            .send()
        };
        let eintrag = &self.soll.eintraege[0].eintrag;
        for (methode, pfad, rumpf) in [
            (
                "PUT",
                "/admin/eintraege/outline",
                serde_json::to_value(eintrag).expect("JSON"),
            ),
            (
                "PUT",
                "/admin/saetze/basis",
                serde_json::json!({"id": "basis", "titel": "Basis", "prioritaet": 0,
                    "eintraege": [{"eintrag": "outline", "werte": {}}]}),
            ),
            (
                "PUT",
                "/admin/gruppen/team",
                serde_json::json!({"id": "team", "titel": "Team", "saetze": ["basis"]}),
            ),
        ] {
            let antwort = admin(methode, pfad, rumpf)
                .await
                .expect("Server erreichbar");
            assert!(
                antwort.status().is_success(),
                "{pfad}: {}",
                antwort.status()
            );
        }
        let code: serde_json::Value = admin(
            "POST",
            "/admin/codes",
            serde_json::json!({"gruppen": ["team"], "minuten": 60}),
        )
        .await
        .expect("Code")
        .json()
        .await
        .expect("Code-JSON");

        let kern = Kern::starten(KernKonfig {
            profil: self.profil.clone(),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: Arc::new(SpeicherTresor::neu()),
        })
        .await
        .expect("Kern startet");
        kern.einbinden(&basis, code["code"].as_str().expect("Code"), "Testrechner")
            .await
            .expect("Einbinden klappt");
        kern
    }

    fn protokoll(&self) -> String {
        std::fs::read_to_string(&self.protokoll).unwrap_or_default()
    }

    /// Warten, bis eine Zeile im Protokoll des Demo-Servers steht.
    async fn warte_auf_zeile(&self, text: &str) {
        let frist = Instant::now() + FRIST;
        while !self.protokoll().contains(text) {
            assert!(
                Instant::now() < frist,
                "„{text}“ bleibt aus; Protokoll: {}",
                self.protokoll()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Drop for Geruest {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

fn sicht(kern: &Kern, id: &EintragId) -> Option<EintragSicht> {
    kern.modell()
        .borrow()
        .eintraege
        .iter()
        .find(|eintrag| &eintrag.id == id)
        .cloned()
}

/// Warten, bis der Eintrag läuft und seine Werkzeuge kennt.
async fn warte_auf_laeuft(kern: &Kern, id: &EintragId) -> EintragSicht {
    let frist = Instant::now() + FRIST;
    loop {
        if let Some(sicht) = sicht(kern, id) {
            if matches!(sicht.zustand, EintragZustand::Laeuft { .. }) && !sicht.tools.is_empty() {
                return sicht;
            }
        }
        assert!(
            Instant::now() < frist,
            "der Eintrag startet nicht: {:?}",
            sicht(kern, id)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ------------------------------------------------------------------ Der Test

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn doppelstart_behaelt_die_werkzeugliste() {
    let id = EintragId::neu("outline");
    for durchlauf in 1..=DURCHLAEUFE {
        let p = Geruest::neu();
        // Der erste Start (vom Server) bleibt stehen.
        std::fs::write(&p.arm, "arm").expect("Arm schreibbar");

        let kern = p.kern().await;
        p.warte_auf_zeile("warte auf Freigabe").await;

        // Zweiter Start desselben Eintrags: derselbe Soll, neue lokale Werte.
        // Die Attrappe wartet jetzt nicht mehr.
        std::fs::remove_file(&p.arm).expect("Arm entfernbar");
        kern.werte_setzen(
            &id,
            Werte::from([("MERKE".to_string(), format!("durchlauf-{durchlauf}"))]),
            Werte::new(),
        )
        .await
        .expect("Werte setzen");

        // Der zweite Start läuft und kennt sein Werkzeug.
        let laeuft = warte_auf_laeuft(&kern, &id).await;
        assert_eq!(
            laeuft.tools,
            vec!["lesen".to_string()],
            "der zweite Start bietet sein Werkzeug an (Durchlauf {durchlauf})"
        );

        // Jetzt den ersten Start freigeben: er ist überholt und darf nichts
        // mehr schreiben.
        std::fs::write(&p.frei, "frei").expect("Freigabe schreibbar");
        tokio::time::sleep(Duration::from_millis(500)).await;

        let danach = sicht(&kern, &id).expect("der Eintrag steht im Modell");
        assert_eq!(
            danach.tools,
            vec!["lesen".to_string()],
            "der überholte Start leert die Werkzeugliste nicht (Durchlauf {durchlauf})"
        );
        assert!(
            matches!(danach.zustand, EintragZustand::Laeuft { tools: 1 }),
            "der Eintrag läuft weiter (Durchlauf {durchlauf}): {:?}",
            danach.zustand
        );

        kern.beenden().await;
    }
}
