// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Einrichtung eines Eintrags im Client (Aufgabe 11, ausgeführt für
//! `docs/aufgaben/11-einrichten.md`).
//!
//! Der Ablauf ist der aus der Aufgabe:
//! 1. Ein Eintrag bietet eine Einrichtung an; im Fenster steht „Einrichten“,
//!    der Zustand ist `Bereit`.
//! 2. `Kern::einrichten` führt den Befehl aus (hier die Attrappe
//!    `quiver-anmelde-attrappe`, Modus `einrichten`), ohne Frist. Exit 0 →
//!    `Bereit` **und der Eintrag wird neu gestartet** (nachzählbar im
//!    Protokoll des Demo-Servers), damit neue Profile sofort wirken.
//! 3. Exit 1 heißt „abgebrochen“: `Bereit` **ohne** Neustart.
//! 4. Nur eine Einrichtung je Eintrag: ein zweiter Klick tut nichts.
//!
//! Die Attrappe bekommt ihre Umgebung aus der Laufzeit des Eintrags, ihr Ziel
//! aus einem `${feld}`-Platzhalter – beides wird hier geprüft.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use quiver_client::{EinrichtZustand, Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{
    Einrichtung, Eintrag, EintragId, Feld, Gruppe, GruppeId, Herkunft, Laufzeit, Quelle, Satz,
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
    /// Der Zustand der Anmelde-Attrappe (Konto).
    ablage: PathBuf,
    /// Wohin die Einrichtung schreibt.
    ziel: PathBuf,
    /// Auf diese Datei wartet die Attrappe im Test, bevor sie fertig wird.
    marker: PathBuf,
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
            "quiver-client-einrichten-{}-{nummer}-{stempel}",
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
        })
        .await
        .expect("Kern startet");

        Self {
            protokoll: ordner.join("protokoll.log"),
            ablage: ordner.join("anmeldung.stand"),
            ziel: ordner.join("einrichtung.stand"),
            marker: ordner.join("weiter.fertig"),
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

    /// Den Eintrag einbinden: Admin-Daten setzen, Code holen, Gerät anmelden.
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
                    werte: Werte::from([
                        (
                            "ABLAGE".to_string(),
                            self.ablage.to_string_lossy().to_string(),
                        ),
                        ("ZIEL".to_string(), self.ziel.to_string_lossy().to_string()),
                    ]),
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

    /// Warten, bis der Eintrag im Modell einen bestimmten Einrichtungszustand hat.
    async fn warte_auf_einrichtung(
        &self,
        id: &EintragId,
        erwartet: fn(&EinrichtZustand) -> bool,
    ) -> EinrichtZustand {
        let frist = Instant::now() + FRIST;
        let mut modell = self.kern.modell();
        loop {
            let zustand = modell
                .borrow_and_update()
                .eintraege
                .iter()
                .find(|eintrag| &eintrag.id == id)
                .and_then(|eintrag| eintrag.einrichtung.clone());
            if let Some(gefunden) = &zustand {
                if erwartet(gefunden) {
                    return gefunden.clone();
                }
            }
            let rest = frist.saturating_duration_since(Instant::now());
            assert!(
                !rest.is_zero(),
                "Einrichtungszustand bleibt aus: {zustand:?}"
            );
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

/// Ein Eintrag mit Einrichtung: **ein** Programm startet den MCP-Server
/// (`mcp`) und öffnet für die Einrichtung seine Seite (`einrichten`). Alle
/// Befehle bekommen dieselbe Umgebung, das Ziel kommt aus `${ZIEL}`.
fn eintrag(p: &Pruefung, id: &str, einrichten: Vec<String>) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "1.0.0".into(),
        titel: "Outline".into(),
        beschreibung: "Wikis verwalten, Zugänge in Profilen.".into(),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: attrappe_bin(),
            argumente: vec!["mcp".to_string()],
            umgebung: Werte::from([
                ("QUIVER_ATTRAPPE_MCP".to_string(), demo_bin()),
                (
                    "QUIVER_DEMO_TOOLS".to_string(),
                    "lesen=Liest eine Seite".to_string(),
                ),
                (
                    "QUIVER_DEMO_LOG".to_string(),
                    p.protokoll.to_string_lossy().to_string(),
                ),
                ("QUIVER_ATTRAPPE_DATEI".to_string(), "${ABLAGE}".to_string()),
            ]),
        },
        felder: vec![feld("ABLAGE", Quelle::Server), feld("ZIEL", Quelle::Server)],
        aktiv: true,
        anmeldung: None,
        einrichtung: Some(Einrichtung { einrichten }),
        mehrfach: None,
    }
}

fn bereit(zustand: &EinrichtZustand) -> bool {
    matches!(zustand, EinrichtZustand::Bereit)
}

fn laeuft(zustand: &EinrichtZustand) -> bool {
    zustand.ist_laeuft()
}

// ------------------------------------------------------------------ Die Tests

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn einrichten_schreibt_das_ziel_und_startet_den_eintrag_neu() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("outline");
    p.einbinden(eintrag(
        &p,
        "outline",
        vec!["einrichten".into(), "${ZIEL}".into()],
    ))
    .await;

    // Vor der Einrichtung: der Eintrag bietet sie an und ist „bereit“.
    let sicht = p.warte_auf_laeuft(&id).await;
    assert_eq!(
        sicht.einrichtung,
        Some(EinrichtZustand::Bereit),
        "ohne eigenen Zustand ist die Einrichtung bereit"
    );
    let vorher = p.starts();
    assert!(vorher >= 1, "der Demo-Server läuft");

    // Einrichten: der Befehl öffnet die Seite (hier: schreibt eine Datei).
    p.kern
        .einrichten(&id)
        .await
        .expect("Einrichtung läuft durch");

    assert_eq!(
        p.warte_auf_einrichtung(&id, bereit).await,
        EinrichtZustand::Bereit
    );
    // Die Attrappe hat den Platzhalter `${ZIEL}` gesehen, nicht den Namen.
    assert_eq!(
        std::fs::read_to_string(&p.ziel)
            .expect("die Einrichtung hat ihre Datei geschrieben")
            .trim(),
        "eingerichtet",
        "der Platzhalter ZIEL wird wie in der Laufzeit ersetzt"
    );

    // Der Eintrag wurde neu gestartet, damit neue Profile sofort wirken: ein
    // Start mehr im Protokoll des Demo-Servers.
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
    let sicht = p.warte_auf_laeuft(&id).await;
    assert_eq!(sicht.tools, vec!["lesen".to_string()]);

    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abbruch_ist_bereit_ohne_neustart() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("outline");
    // Die Attrappe endet bei `einrichten abbruch` mit Exit 1 – so wie ein Tool,
    // dessen Seite der User einfach schließt.
    p.einbinden(eintrag(
        &p,
        "outline",
        vec!["einrichten".into(), "abbruch".into()],
    ))
    .await;
    p.warte_auf_laeuft(&id).await;
    let vorher = p.starts();

    p.kern
        .einrichten(&id)
        .await
        .expect("abgebrochen ist kein Fehler");

    assert_eq!(
        p.warte_auf_einrichtung(&id, bereit).await,
        EinrichtZustand::Bereit,
        "Exit 1 heißt abgebrochen, nicht Fehler"
    );
    // Etwas Zeit für einen etwaigen Neustart, dann muss die Zahl stehen.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        p.starts(),
        vorher,
        "nach einem Abbruch wird nicht neu gestartet"
    );
    assert!(
        !p.ziel.exists(),
        "abgebrochen heißt: die Einrichtung hat nichts geschrieben"
    );

    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_zweiter_klick_waehrend_der_einrichtung_tut_nichts() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("outline");
    // Die Attrappe wartet auf eine Datei, bevor sie fertig wird – so lässt sich
    // der Zustand „läuft“ festhalten.
    let mut eintrag = eintrag(&p, "outline", vec!["einrichten".into(), "${ZIEL}".into()]);
    if let Laufzeit::Stdio { umgebung, .. } = &mut eintrag.laufzeit {
        umgebung.insert(
            "QUIVER_ATTRAPPE_WARTE_AUF".to_string(),
            p.marker.to_string_lossy().to_string(),
        );
    }
    p.einbinden(eintrag).await;
    p.warte_auf_laeuft(&id).await;
    let vorher = p.starts();

    // Erster Klick: läuft, bis die Datei da ist.
    let kern = p.kern.clone();
    let lauf = tokio::spawn({
        let id = id.clone();
        async move { kern.einrichten(&id).await }
    });
    assert_eq!(
        p.warte_auf_einrichtung(&id, laeuft).await,
        EinrichtZustand::Laeuft
    );

    // Zweiter Klick: tut nichts – kein Fehler, kein zweiter Lauf.
    p.kern
        .einrichten(&id)
        .await
        .expect("der zweite Klick ist kein Fehler");
    assert_eq!(
        p.kern
            .modell()
            .borrow()
            .eintraege
            .iter()
            .find(|eintrag| eintrag.id == id)
            .expect("der Eintrag steht im Modell")
            .einrichtung,
        Some(EinrichtZustand::Laeuft),
        "der zweite Klick ändert nichts"
    );
    assert!(p.starts() <= vorher, "und startet nichts neu");

    // Jetzt darf die erste Einrichtung fertig werden.
    std::fs::write(&p.marker, "weiter").expect("Marker schreibbar");
    lauf.await
        .expect("Aufgabe läuft")
        .expect("Einrichtung fertig");
    assert_eq!(
        p.warte_auf_einrichtung(&id, bereit).await,
        EinrichtZustand::Bereit
    );
    assert_eq!(
        std::fs::read_to_string(&p.ziel)
            .expect("die Einrichtung hat geschrieben")
            .trim(),
        "eingerichtet"
    );

    p.kern.beenden().await;
}
