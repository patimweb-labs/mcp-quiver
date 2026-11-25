// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Tools kommen vom Server (Aufgabe 37).
//!
//! Ein echter Server (auf Loopback, mit eigenem Datenordner) und ein echtes
//! Attrappen-Programm: der Client bindet sich ein, holt das Programm, prüft es
//! und startet es. Danach: eine neue Fassung, ein Entzug und die Rückkehr in
//! die Gruppe.
//!
//! Der Test setzt **kein** `QUIVER_TOOLS_DIR`: der Weg über die Ablage im
//! Profil ist genau der, den es zu prüfen gilt. Darum steht er in einer
//! eigenen Datei (ein eigener Testprozess).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use quiver_client::{EintragSicht, Kern, KernKonfig, SpeicherTresor};
use quiver_domain::{Eintrag, EintragId, Herkunft, Laufzeit, Programm, System, Werte};
use quiver_protocol::EintragZustand;

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// Wie lange auf einen Zustand gewartet wird.
const FRIST: Duration = Duration::from_secs(30);

/// Der Server, sein Programmordner und ein eingebundener Client.
struct Welt {
    ordner: PathBuf,
    profil: PathBuf,
    daten: PathBuf,
    basis: String,
    http: reqwest::Client,
    cookie: String,
    kern: Kern,
}

impl Welt {
    /// `programm`: die Bytes, die als `quiver-devops` bereitliegen.
    async fn neu(programm: &[u8]) -> Self {
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let ordner = std::env::temp_dir().join(format!(
            "quiver-tools-server-{}-{stempel}",
            std::process::id()
        ));
        std::fs::create_dir_all(&ordner).expect("Ordner");
        let daten = ordner.join("server");
        let profil = ordner.join("profil");
        let mut welt = Self {
            ordner: ordner.clone(),
            profil,
            daten: daten.clone(),
            basis: String::new(),
            http: reqwest::Client::new(),
            cookie: String::new(),
            kern: starte_kern(&ordner).await,
        };
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
        let mut konfig = quiver_server::Konfig::neu(&daten);
        konfig.oeffentlich = basis.clone();
        // Nur der Ordner dieses Tests: was sonst auf dem Rechner liegt (etwa
        // aus einem Abbild), gehört nicht in den Test.
        konfig.programme = vec![daten.join("tools")];
        let router = quiver_server::app(konfig).await.expect("App baut");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        welt.basis = basis;
        welt.cookie = gemeinsam::sitzung(&welt.http, &welt.basis, &welt.daten).await;
        welt.ablage_leeren();
        welt.ablegen(programm);
        welt.einrichten("devops", "quiver-devops").await;
        let code = welt.code("team").await;
        welt.kern
            .einbinden(&welt.basis, &code, "Testrechner")
            .await
            .expect("Einbinden klappt");
        welt
    }

    fn ablage_leeren(&self) {
        let ordner = self.daten.join("tools").join("linux").join("x64");
        std::fs::remove_dir_all(&ordner).ok();
    }

    /// Ein Programm in die Ablage des Servers legen.
    fn ablegen(&self, inhalt: &[u8]) {
        let system = System::dieses();
        let ordner = self
            .daten
            .join("tools")
            .join(&system.system)
            .join(&system.arch);
        std::fs::create_dir_all(&ordner).expect("Ordner");
        let ziel = ordner.join("quiver-devops");
        std::fs::write(&ziel, inhalt).expect("Programm");
        rechte_setzen(&ziel);
    }

    /// Ein Programm für das System dieses Rechners, wie es der Server nennt.
    fn programm(&self, inhalt: &[u8]) -> Programm {
        let system = System::dieses();
        Programm {
            name: "quiver-devops".into(),
            version: "1.0.0".into(),
            groesse: inhalt.len() as u64,
            sha256: hex::encode(<sha2::Sha256 as sha2::Digest>::digest(inhalt)),
            system: system.system,
            arch: system.arch,
        }
    }

    /// Eintrag, Satz und Gruppe anlegen.
    async fn einrichten(&self, id: &str, befehl: &str) {
        let eintrag = Eintrag {
            id: EintragId::neu(id),
            version: "1.0.0".into(),
            titel: "DevOps".into(),
            beschreibung: "Arbeitselemente lesen.".into(),
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: befehl.to_string(),
                argumente: Vec::new(),
                umgebung: Werte::from([(
                    "QUIVER_DEMO_TOOLS".to_string(),
                    "lesen=Liest ein Arbeitselement".to_string(),
                )]),
            },
            felder: Vec::new(),
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
            mehrfach: None,
        };
        let id = id.to_string();
        for (methode, pfad, rumpf) in [
            (
                "PUT",
                format!("/admin/eintraege/{id}"),
                serde_json::to_value(&eintrag).expect("JSON"),
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
            let antwort = self.admin(methode, pfad.clone(), rumpf).await;
            assert!(
                antwort.status().is_success(),
                "{pfad}: {}",
                antwort.status()
            );
        }
    }

    async fn admin(
        &self,
        methode: &str,
        pfad: String,
        rumpf: serde_json::Value,
    ) -> reqwest::Response {
        self.http
            .request(
                methode.parse().expect("Methode"),
                format!("{}/api/v1{pfad}", self.basis),
            )
            .header("cookie", &self.cookie)
            .header("origin", &self.basis)
            .header("x-quiver", "1")
            .json(&rumpf)
            .send()
            .await
            .expect("Server erreichbar")
    }

    async fn code(&self, gruppe: &str) -> String {
        let antwort = self
            .admin(
                "POST",
                "/admin/codes".to_string(),
                serde_json::json!({"gruppen": [gruppe], "minuten": 60}),
            )
            .await;
        let rumpf: serde_json::Value = antwort.json().await.expect("Code");
        rumpf["code"].as_str().expect("Code").to_string()
    }

    /// Die Gruppe „team“ neu schreiben: dasselbe Ergebnis, aber ein Anlass für
    /// den Abgleich (der Server meldet eine Änderung).
    async fn anstossen(&self) {
        let antwort = self
            .admin(
                "PUT",
                "/admin/gruppen/team".to_string(),
                serde_json::json!({"id": "team", "titel": "Team", "saetze": ["basis"]}),
            )
            .await;
        assert!(antwort.status().is_success());
    }

    /// Den Satz „basis“ mit genau diesen Einträgen schreiben: ein Anlass für
    /// den Abgleich, ohne dass sich etwas anderes ändert.
    async fn anstossen_mit_id(&self, satz: &str, ids: &[&str]) {
        let eintraege: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| serde_json::json!({"eintrag": id, "werte": {}}))
            .collect();
        let antwort = self
            .admin(
                "PUT",
                format!("/admin/saetze/{satz}"),
                serde_json::json!({"id": satz, "titel": "Basis", "prioritaet": 0,
                    "eintraege": eintraege}),
            )
            .await;
        assert!(antwort.status().is_success(), "{}", antwort.status());
    }

    /// Die Gruppe „team“ leer machen: der Zugang ist entzogen.
    async fn entziehen(&self) {
        let antwort = self
            .admin(
                "PUT",
                "/admin/gruppen/team".to_string(),
                serde_json::json!({"id": "team", "titel": "Team", "saetze": []}),
            )
            .await;
        assert!(antwort.status().is_success());
    }

    fn sicht(&self, id: &EintragId) -> Option<EintragSicht> {
        self.kern
            .modell()
            .borrow()
            .eintraege
            .iter()
            .find(|eintrag| &eintrag.id == id)
            .cloned()
    }

    /// Warten, bis der Eintrag läuft und seine Werkzeuge kennt.
    async fn warte_auf_laeuft(&self, id: &EintragId) -> EintragSicht {
        let frist = Instant::now() + FRIST;
        loop {
            if let Some(sicht) = self.sicht(id) {
                if matches!(sicht.zustand, EintragZustand::Laeuft { .. }) && !sicht.tools.is_empty()
                {
                    return sicht;
                }
            }
            assert!(
                Instant::now() < frist,
                "der Eintrag startet nicht: {:?}",
                self.sicht(id)
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Warten, bis eine Bedingung im Modell gilt.
    async fn warte_auf(&self, was: &str, bedingung: impl Fn(&Self) -> bool) {
        let frist = Instant::now() + FRIST;
        while !bedingung(self) {
            assert!(Instant::now() < frist, "nicht erreicht: {was}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Liegt die Fassung dieses Programms im Profil?
    fn im_profil(&self, programm: &Programm) -> PathBuf {
        self.profil
            .join("tools")
            .join(&programm.name)
            .join(&programm.sha256)
            .join(format!("{}{}", programm.name, std::env::consts::EXE_SUFFIX))
    }
}

impl Drop for Welt {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

async fn starte_kern(ordner: &Path) -> Kern {
    Kern::starten(KernKonfig {
        profil: ordner.join("profil"),
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: Arc::new(SpeicherTresor::neu()),
        browser: Arc::new(quiver_client::SystemBrowser),
    })
    .await
    .expect("Kern startet")
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

/// Ein Programm dieser Kiste, das sich als MCP-Server ausgibt (die Attrappe).
fn attrappe(name: &str) -> Vec<u8> {
    let pfad = match name {
        "demo" => env!("CARGO_BIN_EXE_quiver-demo-mcp"),
        "echo" => env!("CARGO_BIN_EXE_quiver-echo-mcp"),
        anderes => panic!("unbekannte Attrappe: {anderes}"),
    };
    std::fs::read(pfad).expect("Attrappe lesbar")
}

/// Anmeldung lädt das Programm vom Server, prüft es und startet es.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn das_programm_kommt_vom_server() {
    let inhalt = attrappe("demo");
    let welt = Welt::neu(&inhalt).await;
    let id = EintragId::neu("devops");

    let laeuft = welt.warte_auf_laeuft(&id).await;
    assert!(
        laeuft.tools.iter().any(|tool| tool == "lesen"),
        "die Werkzeuge des geladenen Programms fehlen: {:?}",
        laeuft.tools
    );
    assert_eq!(laeuft.nicht_installiert, None);
    assert!(!laeuft.nicht_fuer_system);

    // Die Ablage im Profil: tools/<name>/<sha256>/<programm>, 0700.
    let programm = welt.programm(&inhalt);
    let pfad = welt.im_profil(&programm);
    assert!(pfad.is_file(), "{} fehlt", pfad.display());
    assert_eq!(std::fs::read(&pfad).expect("Programm"), inhalt);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let rechte = std::fs::metadata(&pfad).expect("Rechte").permissions();
        assert_eq!(rechte.mode() & 0o777, 0o700, "nur der Benutzer");
        let ordner = pfad.parent().expect("Ordner");
        let rechte = std::fs::metadata(ordner).expect("Rechte").permissions();
        assert_eq!(rechte.mode() & 0o777, 0o700, "der Ordner auch");
    }
    // Zwischendateien bleiben nicht liegen.
    let ordner = pfad.parent().expect("Ordner");
    let reste: Vec<_> = std::fs::read_dir(ordner)
        .expect("Ordner")
        .flatten()
        .filter(|eintrag| eintrag.file_name().to_string_lossy().starts_with(".laden-"))
        .collect();
    assert!(reste.is_empty(), "Zwischendatei liegt noch da");

    welt.kern.beenden().await;
}

/// Eine neue Fassung auf dem Server wird geholt und gestartet; die alte fällt
/// nach dem erfolgreichen Start weg.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eine_neue_fassung_wird_geholt() {
    let alt = attrappe("demo");
    let welt = Welt::neu(&alt).await;
    let id = EintragId::neu("devops");
    welt.warte_auf_laeuft(&id).await;

    // Eine andere Fassung: ein anderes Programm mit anderen Werkzeugen.
    let neu = attrappe("echo");
    assert_ne!(alt, neu);
    welt.ablage_leeren();
    welt.ablegen(&neu);
    // Der Server rechnet die Revision bei der nächsten Frage neu; ein Anlass
    // (eine Änderung in der Verwaltung) weckt die Geräte.
    welt.anstossen().await;

    // Der Eintrag meldet sich mit den Werkzeugen der neuen Fassung.
    welt.warte_auf("die neue Fassung läuft", |welt| {
        welt.sicht(&id)
            .is_some_and(|sicht| sicht.tools.iter().any(|tool| tool == "echo"))
    })
    .await;

    // Die neue Fassung liegt im Profil, die alte ist nach dem Start weg.
    welt.warte_auf("die alte Fassung ist gelöscht", |welt| {
        !welt.im_profil(&welt.programm(&alt)).exists()
    })
    .await;
    assert!(welt.im_profil(&welt.programm(&neu)).is_file());

    welt.kern.beenden().await;
}

/// Entzug: das Programm wird gelöscht, die Konfiguration bleibt. Zurück in der
/// Gruppe läuft es ohne neue Angaben wieder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entzug_loescht_das_programm_und_behaelt_die_konfiguration() {
    let inhalt = attrappe("demo");
    let welt = Welt::neu(&inhalt).await;
    let id = EintragId::neu("devops");
    welt.warte_auf_laeuft(&id).await;
    let programm = welt.programm(&inhalt);
    assert!(welt.im_profil(&programm).is_file());

    welt.entziehen().await;
    // Das Gerät verliert den Eintrag; das Programm verschwindet.
    welt.warte_auf("das Programm ist gelöscht", |welt| {
        !welt.im_profil(&programm).exists()
    })
    .await;
    welt.warte_auf("der Eintrag ist aus dem Soll", |welt| {
        welt.sicht(&id).is_none()
    })
    .await;

    // Zurück in die Gruppe: dasselbe Gerät, keine neuen Angaben, es läuft.
    welt.anstossen().await;
    let laeuft = welt.warte_auf_laeuft(&id).await;
    assert!(laeuft.tools.iter().any(|tool| tool == "lesen"));

    welt.kern.beenden().await;
}

/// Ein Programm, das der Server für dieses System nicht hat, ist kein Fehler:
/// die Karte sagt „Für dieses System gibt es das Tool nicht.“ Ein fremdes
/// Programm, das hier fehlt, wird weiter als „nicht installiert“ gemeldet.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ein_programm_ohne_system_und_ein_fremdes_programm() {
    let inhalt = attrappe("demo");
    let welt = Welt::neu(&inhalt).await;
    // Ein zweiter Eintrag, den der Server nicht verteilt.
    welt.einrichten("fremd", "gibtsnicht-4711").await;
    welt.anstossen_mit_id("basis", &["devops", "fremd"]).await;

    let verteilt = EintragId::neu("devops");
    welt.warte_auf_laeuft(&verteilt).await;

    // Ein Eintrag, dessen Programm es für dieses System nicht gibt: der
    // Eintrag nennt ein Programm, das der Server nicht hat.
    welt.einrichten("ohne", "quiver-gibtsnicht").await;
    welt.anstossen_mit_id("basis", &["devops", "ohne", "fremd"])
        .await;
    welt.warte_auf("„für dieses System gibt es das nicht“", |welt| {
        welt.sicht(&EintragId::neu("ohne"))
            .is_some_and(|sicht| sicht.nicht_fuer_system)
    })
    .await;
    let ohne = welt.sicht(&EintragId::neu("ohne")).expect("Eintrag");
    assert_eq!(ohne.nicht_installiert, None);
    assert!(
        !matches!(ohne.zustand, EintragZustand::Laeuft { .. }),
        "startet nicht: {:?}",
        ohne.zustand
    );

    welt.warte_auf("das fremde Programm fehlt", |welt| {
        welt.sicht(&EintragId::neu("fremd"))
            .is_some_and(|sicht| sicht.nicht_installiert.is_some())
    })
    .await;

    welt.kern.beenden().await;
}
