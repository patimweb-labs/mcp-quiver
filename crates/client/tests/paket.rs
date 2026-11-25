// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Werkzeuge im Paket (Aufgabe 35): ein Eintrag nennt nur den
//! Programmnamen, der Client sucht ihn im Ordner neben dem eigenen Programm.
//!
//! Der Test stellt ein Paket nach — ein Ordner mit „Werkzeugen“, den
//! `QUIVER_TOOLS_DIR` benennt (so wie ein Bündel seinen Ordner mitbringt) — und
//! bindet einen Eintrag ein, dessen Befehl **kein Pfad** ist. Der Eintrag muss
//! starten und seine Werkzeuge anbieten.
//!
//! Dazu der Altbestand: ein Eintrag mit einem festen Pfad, den es hier nicht
//! gibt. Sein Dateiname liegt im Paket, also läuft die mitgelieferte Fassung,
//! und die Protokollseite sagt, warum.
//!
//! Jeder Test setzt `QUIVER_TOOLS_DIR` für den ganzen Prozess; darum steht er
//! in einer eigenen Datei (ein eigener Testprozess) und läuft allein.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use quiver_client::{EintragSicht, Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{Eintrag, EintragId, Herkunft, Laufzeit, Soll, SollEintrag, Werte};
use quiver_protocol::EintragZustand;

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// Wie lange auf einen Zustand gewartet wird.
const FRIST: Duration = Duration::from_secs(20);

/// Ein nachgestelltes Paket: Ordner, Profil, Soll.
struct Paket {
    ordner: PathBuf,
    profil: PathBuf,
    /// Der Ordner mit den „mitgelieferten“ Werkzeugen.
    werkzeuge: PathBuf,
    soll: Soll,
}

impl Paket {
    /// `befehl` ist das, was im Eintrag steht — ein Name oder ein Pfad.
    fn neu(name: &str, befehl: &str) -> Self {
        Self::im_ordner(name, befehl, "quiver-client-paket")
    }

    /// Dasselbe in einem Ordner mit eigenem Namen — für Pfade mit Leerzeichen
    /// und Umlauten (Aufgabe 35, §6).
    fn im_ordner(name: &str, befehl: &str, praefix: &str) -> Self {
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let ordner =
            std::env::temp_dir().join(format!("{praefix}-{}-{stempel}", std::process::id()));
        let werkzeuge = ordner.join("werkzeuge");
        std::fs::create_dir_all(&werkzeuge).expect("Ordner");

        // Das Beispiel-Werkzeug unter dem Namen, den der Eintrag nennt.
        let ziel = werkzeuge.join(format!("quiver-devops{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(env!("CARGO_BIN_EXE_quiver-demo-mcp"), &ziel).expect("Werkzeug kopieren");
        rechte_setzen(&ziel);

        let eintrag = Eintrag {
            id: EintragId::neu(name),
            version: "1.0.0".into(),
            titel: "DevOps".into(),
            beschreibung: "Arbeitselemente lesen.".into(),
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: befehl.to_string(),
                argumente: Vec::new(),
                umgebung: Werte::from([
                    ("QUIVER_DEMO_NAME".to_string(), "devops".to_string()),
                    (
                        "QUIVER_DEMO_TOOLS".to_string(),
                        "lesen=Liest ein Arbeitselement".to_string(),
                    ),
                ]),
            },
            felder: Vec::new(),
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
            mehrfach: None,
        };
        Self {
            profil: ordner.join("profil"),
            ordner,
            werkzeuge,
            soll: Soll::neu(vec![SollEintrag {
                eintrag,
                werte: Werte::new(),
            }]),
        }
    }

    /// Server mit dem Eintrag, Kern, der sich einbindet.
    async fn kern(&self) -> Kern {
        let daten = self.ordner.join("server");
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
        let http = reqwest::Client::new();
        let mut konfig = quiver_server::Konfig::neu(&daten);
        konfig.oeffentlich = basis.clone();
        let router = quiver_server::app(konfig).await.expect("App baut");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let cookie = gemeinsam::sitzung(&http, &basis, &daten).await;
        let admin = |methode: &str, pfad: String, rumpf: serde_json::Value| {
            http.request(
                methode.parse().expect("Methode"),
                format!("{basis}/api/v1{pfad}"),
            )
            .header("cookie", &cookie)
            .header("origin", &basis)
            .header("x-quiver", "1")
            .json(&rumpf)
            .send()
        };
        let eintrag = &self.soll.eintraege[0].eintrag;
        let id = eintrag.id.als_str().to_string();
        for (methode, pfad, rumpf) in [
            (
                "PUT",
                format!("/admin/eintraege/{id}"),
                serde_json::to_value(eintrag).expect("JSON"),
            ),
            (
                "PUT",
                "/admin/saetze/basis".to_string(),
                serde_json::json!({"id": "basis", "titel": "Basis", "prioritaet": 0,
                    "eintraege": [{"eintrag": id, "werte": {}}]}),
            ),
            (
                "PUT",
                "/admin/gruppen/team".to_string(),
                serde_json::json!({"id": "team", "titel": "Team", "saetze": ["basis"]}),
            ),
        ] {
            let antwort = admin(methode, pfad.clone(), rumpf)
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
            "/admin/codes".to_string(),
            serde_json::json!({"gruppen": ["team"], "minuten": 60}),
        )
        .await
        .expect("Code")
        .json()
        .await
        .expect("Code-JSON");

        std::env::set_var(quiver_client::ORDNER_SCHALTER, &self.werkzeuge);

        let kern = Kern::starten(KernKonfig {
            profil: self.profil.clone(),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: Arc::new(SpeicherTresor::neu()),
            browser: Arc::new(quiver_client::SystemBrowser),
        })
        .await
        .expect("Kern startet");
        kern.einbinden(&basis, code["code"].as_str().expect("Code"), "Testrechner")
            .await
            .expect("Einbinden klappt");
        kern
    }
}

impl Drop for Paket {
    fn drop(&mut self) {
        std::env::remove_var(quiver_client::ORDNER_SCHALTER);
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

#[cfg(unix)]
fn rechte_setzen(pfad: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut rechte = std::fs::metadata(pfad).expect("Rechte").permissions();
    rechte.set_mode(0o755);
    std::fs::set_permissions(pfad, rechte).expect("Rechte setzen");
}

#[cfg(not(unix))]
fn rechte_setzen(_pfad: &Path) {}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn werkzeuge_kommen_aus_dem_paket() {
    let id = EintragId::neu("devops");

    // 1. Der Eintrag nennt nur den Namen – kein Pfad, nichts vom
    //    Entwicklerrechner.
    {
        let paket = Paket::neu("devops", "quiver-devops");
        let kern = paket.kern().await;
        let laeuft = warte_auf_laeuft(&kern, &id).await;
        assert!(
            laeuft.tools.iter().any(|tool| tool == "lesen"),
            "die Werkzeuge des mitgelieferten Programms fehlen: {:?}",
            laeuft.tools
        );
        kern.beenden().await;
    }

    // 2. Altbestand: so stand es in der Bibliothek — der Pfad einer
    //    Entwicklermaschine. Der Name ist mitgeliefert, also läuft er, und
    //    die Protokollseite sagt warum.
    {
        let alt = "/niemand/hat/hier/quiver-tools/target/release/quiver-devops";
        let paket = Paket::neu("devops", alt);
        let kern = paket.kern().await;
        let laeuft = warte_auf_laeuft(&kern, &id).await;
        assert!(
            laeuft.tools.iter().any(|tool| tool == "lesen"),
            "die mitgelieferte Fassung läuft nicht: {:?}",
            laeuft.tools
        );

        // Der Vermerk steht in der Protokollseite des Eintrags — mit dem
        // alten Pfad, denn das Protokoll bleibt auf dem Gerät.
        let zeilen = kern.prozess_log(&id, &quiver_client::LogFilter::default());
        assert!(
            zeilen
                .iter()
                .any(|zeile| zeile.text.contains("quiver-devops") && zeile.text.contains(alt)),
            "kein Vermerk im Protokoll: {zeilen:?}"
        );
        kern.beenden().await;
    }
}

/// Fehlt das Programm auf diesem Gerät, sagt das Modell es einmal
/// (`nicht_installiert`) und die Anmeldung wird gar nicht erst geprüft; der
/// Hinweis für den Admin steht im Protokoll des Eintrags (Aufgabe 35, §5).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_fehlt_und_der_admin_liest_den_hinweis_im_protokoll() {
    let id = EintragId::neu("devops");
    let mut paket = Paket::neu("devops", "quiver-gibtsnicht");
    // Der Eintrag bietet eine Anmeldung an – ohne Programm gibt es nichts zu
    // prüfen, und die Karte zeigt keinen Anmelde-Knopf.
    paket.soll.eintraege[0].eintrag.anmeldung = Some(quiver_domain::Anmeldung {
        anmelden: vec!["anmelden".into()],
        status: vec!["status".into()],
        abmelden: vec!["abmelden".into()],
    });
    let kern = paket.kern().await;

    let frist = Instant::now() + FRIST;
    let stand = loop {
        if let Some(sicht) = sicht(&kern, &id) {
            // Gewartet wird auf den Befund; der Startversuch läuft nebenher.
            if sicht.nicht_installiert.is_some()
                && matches!(sicht.zustand, EintragZustand::Fehler { .. })
            {
                break sicht;
            }
        }
        assert!(
            Instant::now() < frist,
            "kein Befund im Modell: {:?}",
            sicht(&kern, &id)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        stand.nicht_installiert.as_deref(),
        Some("quiver-gibtsnicht")
    );
    // Die Anmeldung wurde nicht geprüft: sie steht auf „unbekannt“.
    assert_eq!(
        stand.anmeldung,
        Some(quiver_client::AnmeldeZustand::Unbekannt)
    );
    // Gemeldet wird der Name, nie ein fremder Pfad.
    assert!(
        !stand
            .nicht_installiert
            .as_deref()
            .unwrap_or_default()
            .contains('/'),
        "der Befund nennt einen Pfad"
    );

    // Der Hinweis für den Admin steht im Protokoll des Eintrags.
    let zeilen = kern.prozess_log(&id, &quiver_client::LogFilter::default());
    assert!(
        zeilen
            .iter()
            .any(|zeile| zeile.text.contains("quiver-gibtsnicht") && zeile.text.contains("Admin")),
        "kein Hinweis im Protokoll: {zeilen:?}"
    );

    kern.beenden().await;
}

/// Ein entpacktes Paket in einem Ordner mit Leerzeichen und Umlauten, gestartet
/// aus einem anderen Arbeitsverzeichnis: der Client findet den Ordner neben
/// seinem Programm und das Werkzeug antwortet (Aufgabe 35, §6).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn leerraum_und_umlaute_im_pfad_stoeren_nicht() {
    let paket = Paket::im_ordner("devops", "quiver-devops", "Quiver Paket Äöü mit Leerraum");
    let kern = paket.kern().await;
    let laeuft = warte_auf_laeuft(&kern, &EintragId::neu("devops")).await;
    assert!(
        laeuft.tools.iter().any(|tool| tool == "lesen"),
        "das Werkzeug aus dem Ordner mit Leerzeichen und Umlauten antwortet nicht: {:?}",
        laeuft.tools
    );
    assert_eq!(laeuft.nicht_installiert, None);
    kern.beenden().await;
}
