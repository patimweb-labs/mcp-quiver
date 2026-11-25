// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Mehrere Zugänge je Eintrag im Client (Aufgabe 14, ausgeführt für
//! `docs/aufgaben/14-mehrere-zugaenge.md`).
//!
//! Der Ablauf ist der aus der Aufgabe:
//! 1. Ein Eintrag führt eine Zugangsliste (`mehrfach`) und **läuft auch ohne
//!    Zugang** – das Tool meldet dann selbst „kein Zugang“.
//! 2. `Kern::zugang_speichern` legt Zugänge an: nicht-geheime Werte in die
//!    Client-Konfiguration, Geheimes in den Tresor unter
//!    `zugang/<eintrag>/<name>/<feld>`.
//! 3. Beim Start stehen **alle** Zugänge samt Geheimnis als JSON in
//!    `QUIVER_ZUGAENGE`; nach jeder Änderung startet der Eintrag neu.
//! 4. `Kern::zugang_loeschen` entfernt den Zugang **und** seine Geheimnisse.
//!
//! Das Fenster sieht nie ein Geheimnis: `EintragSicht.zugaenge` trägt nur,
//! welche Geheimfelder gesetzt sind.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use quiver_client::{Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{
    Eintrag, EintragId, Feld, Gruppe, GruppeId, Herkunft, Laufzeit, Mehrfach, Quelle, Satz,
    SatzEintrag, SatzId, Werte, Zugang,
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
    /// Wohin das Tool beim Start `QUIVER_ZUGAENGE` schreibt.
    zugaenge_datei: PathBuf,
    /// Die Ablage der Attrappe (An-/Abmeldezustand, hier ungenutzt).
    ablage: PathBuf,
    /// Der Tresor im Speicher – hier lässt sich nachsehen, was drin liegt.
    tresor: Arc<SpeicherTresor>,
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
            "quiver-client-zugaenge-{}-{nummer}-{stempel}",
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
            protokoll: ordner.join("protokoll.log"),
            zugaenge_datei: ordner.join("zugaenge.json"),
            ablage: ordner.join("anmeldung.stand"),
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

    /// Wie oft der Demo-Server gestartet wurde – ein Neustart ist ein Start mehr.
    fn starts(&self) -> usize {
        std::fs::read_to_string(&self.protokoll)
            .unwrap_or_default()
            .lines()
            .filter(|zeile| zeile.contains("gestartet"))
            .count()
    }

    /// Warten, bis der Eintrag neu gestartet wurde (ein Start mehr im Protokoll
    /// des Demo-Servers). Der Start läuft im Hintergrund; die Sicht kann schon
    /// die neue Liste zeigen, bevor der Prozess seine Zeile geschrieben hat.
    async fn warte_auf_start(&self, vorher: usize) {
        let frist = Instant::now() + FRIST;
        while self.starts() <= vorher {
            assert!(
                Instant::now() < frist,
                "der Eintrag startet nicht neu ({} Starts, vorher {vorher})",
                self.starts()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Was das Tool beim Start in `QUIVER_ZUGAENGE` gesehen hat. `None`, solange
    /// die Datei (noch) nicht lesbar ist – sie entsteht bei jedem Start neu.
    fn zugaenge_im_tool(&self) -> Option<Vec<Zugang>> {
        let text = std::fs::read_to_string(&self.zugaenge_datei).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Warten, bis das Tool eine Zugangsliste sieht, die zur Bedingung passt.
    /// Die Datei entsteht bei jedem Start neu – eine alte Füllung zählt nicht.
    async fn warte_auf_zugaenge(
        &self,
        was: &str,
        bedingung: impl Fn(&[Zugang]) -> bool,
    ) -> Vec<Zugang> {
        let frist = Instant::now() + FRIST;
        loop {
            if let Some(liste) = self.zugaenge_im_tool() {
                if bedingung(&liste) {
                    return liste;
                }
            }
            assert!(
                Instant::now() < frist,
                "das Tool sieht nicht „{was}“: {:?}",
                self.zugaenge_im_tool()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
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

/// Die Zugangsliste aus der Aufgabe: quiver-ssh mit „Verbindungen“ (Name,
/// Host, Port, Benutzer, Anmelde-Passwort, sudo-Passwort).
fn mehrfach() -> Mehrfach {
    Mehrfach {
        titel: "Verbindungen".into(),
        einzahl: "Verbindung".into(),
        felder: vec![
            feld("benutzer", Quelle::Client, true),
            feld("host", Quelle::Client, true),
            feld("port", Quelle::Client, false),
            feld("anmelde_passwort", Quelle::Geheim, false),
            feld("sudo_passwort", Quelle::Geheim, false),
        ],
    }
}

/// Ein Eintrag mit Zugangsliste. Das Programm schreibt beim Start
/// `QUIVER_ZUGAENGE` in eine Datei – so ist nachzählbar, was das Tool sieht.
fn eintrag(p: &Pruefung) -> Eintrag {
    Eintrag {
        id: EintragId::neu("ssh"),
        version: "1.0.0".into(),
        titel: "SSH".into(),
        beschreibung: "Verbindungen zu Servern.".into(),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: attrappe_bin(),
            argumente: vec!["mcp".to_string()],
            umgebung: Werte::from([
                ("QUIVER_ATTRAPPE_MCP".to_string(), demo_bin()),
                (
                    "QUIVER_DEMO_TOOLS".to_string(),
                    "verbinden=Verbindet einen Host".to_string(),
                ),
                (
                    "QUIVER_DEMO_LOG".to_string(),
                    p.protokoll.to_string_lossy().to_string(),
                ),
                (
                    "QUIVER_ZUGAENGE_DATEI".to_string(),
                    p.zugaenge_datei.to_string_lossy().to_string(),
                ),
                // Die Attrappe braucht für jeden Befehl ihre Ablage.
                (
                    "QUIVER_ATTRAPPE_DATEI".to_string(),
                    p.ablage.to_string_lossy().to_string(),
                ),
            ]),
        },
        felder: Vec::new(),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: Some(mehrfach()),
    }
}

fn werte(paare: &[(&str, &str)]) -> Werte {
    paare
        .iter()
        .map(|(name, wert)| ((*name).to_string(), (*wert).to_string()))
        .collect()
}

// ------------------------------------------------------------------ Die Tests

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zwei_zugaenge_stehen_beim_start_im_tool_einer_weniger_nach_dem_loeschen() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("ssh");
    p.einbinden(eintrag(&p)).await;

    // 1. Ohne Zugang läuft der Eintrag trotzdem – das Tool bekommt „[]“.
    let sicht = p.warte_auf_laeuft(&id).await;
    let zugaenge = sicht.zugaenge.expect("der Eintrag führt eine Zugangsliste");
    assert_eq!(zugaenge.titel, "Verbindungen");
    assert_eq!(zugaenge.einzahl, "Verbindung");
    assert_eq!(zugaenge.felder.len(), 5, "alle Felder der Liste");
    assert!(zugaenge.liste.is_empty(), "noch kein Zugang");
    assert_eq!(
        p.warte_auf_zugaenge("kein Zugang", <[Zugang]>::is_empty)
            .await,
        Vec::new(),
        "ein Eintrag ohne Zugang läuft und sieht eine leere Liste"
    );

    // 2. Zwei Zugänge anlegen – einer mit beiden Geheimnissen, einer mit einem.
    p.kern
        .zugang_speichern(
            &id,
            "web-01",
            werte(&[
                ("benutzer", "root"),
                ("host", "web-01.example"),
                ("port", "22"),
            ]),
            werte(&[
                ("anmelde_passwort", "geheim-1"),
                ("sudo_passwort", "geheim-2"),
            ]),
        )
        .await
        .expect("Zugang anlegen");
    p.kern
        .zugang_speichern(
            &id,
            "db-01",
            werte(&[("benutzer", "admin"), ("host", "db-01.example")]),
            werte(&[("anmelde_passwort", "geheim-3")]),
        )
        .await
        .expect("zweiter Zugang");

    let liste = p
        .warte_auf_zugaenge("zwei Zugänge", |liste| liste.len() == 2)
        .await;
    assert_eq!(
        liste
            .iter()
            .map(|zugang| zugang.name.as_str())
            .collect::<Vec<_>>(),
        vec!["db-01", "web-01"],
        "beide Zugänge stehen in QUIVER_ZUGAENGE, nach Namen sortiert"
    );
    let web = liste
        .iter()
        .find(|zugang| zugang.name == "web-01")
        .expect("web-01");
    for (name, wert) in [
        ("benutzer", "root"),
        ("host", "web-01.example"),
        ("port", "22"),
    ] {
        assert_eq!(
            web.werte.get(name).map(String::as_str),
            Some(wert),
            "der Wert {name} steht im JSON"
        );
    }
    assert_eq!(
        web.werte.get("anmelde_passwort").map(String::as_str),
        Some("geheim-1"),
        "das Geheimnis ist beim Start eingesetzt"
    );
    assert_eq!(
        web.werte.get("sudo_passwort").map(String::as_str),
        Some("geheim-2")
    );
    let db = liste
        .iter()
        .find(|zugang| zugang.name == "db-01")
        .expect("db-01");
    assert_eq!(
        db.werte.get("anmelde_passwort").map(String::as_str),
        Some("geheim-3")
    );
    assert!(
        !db.werte.contains_key("sudo_passwort"),
        "ein nicht gesetztes Geheimnis fehlt im JSON"
    );

    // Das Fenster sieht die Zugänge, aber kein Geheimnis.
    let sicht = p.warte_auf_laeuft(&id).await;
    let zugaenge = sicht.zugaenge.expect("Zugangsliste");
    assert_eq!(zugaenge.liste.len(), 2);
    let web = zugaenge
        .liste
        .iter()
        .find(|zugang| zugang.name == "web-01")
        .expect("web-01 in der Sicht");
    assert_eq!(
        web.werte.get("host").map(String::as_str),
        Some("web-01.example")
    );
    assert!(
        !web.werte.keys().any(|name| name.contains("passwort")),
        "Geheimes steht nie im Modell: {:?}",
        web.werte
    );
    assert_eq!(
        web.geheim_gesetzt,
        vec!["anmelde_passwort", "sudo_passwort"]
    );

    // Die Geheimnisse liegen im Tresor, nicht im Profil.
    let abgelegt = p.tresor.alles();
    assert_eq!(
        abgelegt
            .get("zugang/ssh/web-01/anmelde_passwort")
            .map(String::as_str),
        Some("geheim-1")
    );
    let profil = std::fs::read_to_string(p.ordner.join("profil/zustand.json")).expect("Profil");
    assert!(
        !profil.contains("geheim-1"),
        "Geheimnisse gehören nicht ins Profil: {profil}"
    );

    // 3. Einen Zugang löschen: er verschwindet aus dem JSON, seine Geheimnisse
    //    aus dem Tresor.
    let vorher = p.starts();
    p.kern
        .zugang_loeschen(&id, "web-01")
        .await
        .expect("Zugang löschen");
    let liste = p
        .warte_auf_zugaenge("nur db-01", |liste| {
            liste.len() == 1 && liste[0].name == "db-01"
        })
        .await;
    assert_eq!(liste[0].name, "db-01", "nur db-01 bleibt");
    p.warte_auf_start(vorher).await;
    let abgelegt = p.tresor.alles();
    for feld in ["anmelde_passwort", "sudo_passwort"] {
        assert!(
            !abgelegt.contains_key(&format!("zugang/ssh/web-01/{feld}")),
            "das Geheimnis {feld} von web-01 muss weg sein: {abgelegt:?}"
        );
    }
    assert_eq!(
        abgelegt
            .get("zugang/ssh/db-01/anmelde_passwort")
            .map(String::as_str),
        Some("geheim-3"),
        "die Geheimnisse des anderen Zugangs bleiben"
    );

    p.kern.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn regeln_fuer_namen_pflichtfelder_und_leere_geheimnisse() {
    let p = Pruefung::neu().await;
    let id = EintragId::neu("ssh");
    p.einbinden(eintrag(&p)).await;
    p.warte_auf_laeuft(&id).await;

    // Der Name folgt `zugangsname_gueltig` (A–Z, a–z, 0–9, - und _, 1–40 Zeichen).
    for name in ["", "web 01", "wäb-01", "web/01"] {
        let fehler = p
            .kern
            .zugang_speichern(
                &id,
                name,
                werte(&[("benutzer", "root"), ("host", "h")]),
                Werte::new(),
            )
            .await
            .expect_err("kein gültiger Name");
        assert!(fehler.to_string().contains("Name"), "{name:?}: {fehler}");
    }

    // Pflichtfelder kommen aus `mehrfach.felder`: host fehlt.
    let fehler = p
        .kern
        .zugang_speichern(&id, "web-01", werte(&[("benutzer", "root")]), Werte::new())
        .await
        .expect_err("host ist Pflicht");
    assert!(fehler.to_string().contains("host"), "{fehler}");

    // Ein leeres Geheimfeld heißt „unverändert“.
    p.kern
        .zugang_speichern(
            &id,
            "web-01",
            werte(&[("benutzer", "root"), ("host", "web-01.example")]),
            werte(&[("anmelde_passwort", "geheim-1")]),
        )
        .await
        .expect("anlegen");
    p.kern
        .zugang_speichern(
            &id,
            "web-01",
            werte(&[("benutzer", "admin"), ("host", "web-01.example")]),
            werte(&[("anmelde_passwort", "")]),
        )
        .await
        .expect("ändern");
    assert_eq!(
        p.tresor
            .alles()
            .get("zugang/ssh/web-01/anmelde_passwort")
            .map(String::as_str),
        Some("geheim-1"),
        "ein leeres Geheimfeld überschreibt nichts"
    );
    let liste = p
        .warte_auf_zugaenge("geänderter Benutzer", |liste| {
            liste.len() == 1 && liste[0].werte.get("benutzer").map(String::as_str) == Some("admin")
        })
        .await;
    assert_eq!(
        liste[0].werte.get("benutzer").map(String::as_str),
        Some("admin"),
        "der geänderte lokale Wert steht im JSON"
    );
    assert_eq!(
        liste[0].werte.get("anmelde_passwort").map(String::as_str),
        Some("geheim-1")
    );

    // Ein einzelnes Geheimnis lässt sich entfernen (z. B. das Anmelde-Passwort,
    // nachdem der Schlüssel eingerichtet ist).
    p.kern
        .zugang_geheim_loeschen(&id, "web-01", "anmelde_passwort")
        .await
        .expect("Geheimnis löschen");
    assert!(!p
        .tresor
        .alles()
        .contains_key("zugang/ssh/web-01/anmelde_passwort"));
    let liste = p
        .warte_auf_zugaenge("ohne Anmelde-Passwort", |liste| {
            liste.len() == 1 && !liste[0].werte.contains_key("anmelde_passwort")
        })
        .await;
    assert!(
        !liste[0].werte.contains_key("anmelde_passwort"),
        "das entfernte Geheimnis steht nicht mehr im JSON: {:?}",
        liste[0].werte
    );
    let sicht = p.warte_auf_laeuft(&id).await;
    assert!(
        sicht.zugaenge.expect("Liste").liste[0]
            .geheim_gesetzt
            .is_empty(),
        "nichts mehr gesetzt"
    );

    // Einträge ohne Zugangsliste lehnen Zugänge ab.
    let fehler = p
        .kern
        .zugang_speichern(
            &EintragId::neu("gibt-es-nicht"),
            "web-01",
            Werte::new(),
            Werte::new(),
        )
        .await
        .expect_err("unbekannter Eintrag");
    assert!(fehler.to_string().contains("nicht zugewiesen"), "{fehler}");

    p.kern.beenden().await;
}
