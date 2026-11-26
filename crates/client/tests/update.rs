// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Client bietet ein Update an und tauscht sich selbst aus (Aufgabe 39).
//!
//! **Kein echtes Netz:** die „Veröffentlichung“ ist eine Attrappe auf Loopback
//! (sie stellt GitHub), der Server ist ein echter quiver-Server mit eigenem
//! Datenordner, und getauscht wird in einem Testordner – nie das laufende
//! Programm.
//!
//! Geprüft wird: der Client fragt seinen Server, prüft Größe und SHA-256,
//! startet die neue Fassung mit `--selbsttest`, tauscht erst dann, und Profil,
//! Angaben und Geräte-Token sind vorher und nachher gleich.
//!
//! Nur unter Unix: der Selbsttest startet die neue Fassung als eigenes
//! Programm, und die Attrappe dafür ist ein Shell-Skript. (Der Tausch selbst
//! ist in `crates/client/src/update/tausch/tests.rs` auf jedem System geprüft.)

#![cfg(unix)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::Path as Pfad;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use quiver_client::update::{Angebot, AngebotGrund, Aufstellung, Installationsart};
use quiver_client::{Kern, KernKonfig, SpeicherTresor, UpdateLage};
use serde_json::json;
use tokio::sync::Mutex;

#[path = "gemeinsam.rs"]
mod gemeinsam;

/// So lange wird auf einen Zustand gewartet.
const FRIST: Duration = Duration::from_secs(30);

/// Das Programm der neuen Fassung: es besteht den Selbsttest.
const NEU: &str = "#!/bin/sh\nexit 0\n";
/// Das Programm der alten Fassung (im Testordner).
const ALT: &str = "#!/bin/sh\nexit 1\n";

fn sha256(inhalt: &[u8]) -> String {
    hex::encode(<sha2::Sha256 as sha2::Digest>::digest(inhalt))
}

/// Der Paketname für **dieses** System (der Server gibt nur das heraus, was zu
/// dem System gehört, das das Gerät meldet).
fn paketname(version: &str) -> String {
    let system = quiver_domain::System::dieses();
    let arch = if system.arch == "x64" {
        "x86_64"
    } else {
        "arm64"
    };
    format!("quiver-{version}-{}-{arch}.zip", system.system)
}

/// Ein ZIP mit einer ausführbaren Datei (gespeichert).
fn zip_mit(name: &str, inhalt: &[u8]) -> Vec<u8> {
    let mut daten = Vec::new();
    let kopf = daten.len();
    daten.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
    daten.extend_from_slice(&20u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u32.to_le_bytes());
    daten.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    daten.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    daten.extend_from_slice(&(name.len() as u16).to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(name.as_bytes());
    daten.extend_from_slice(inhalt);
    let verzeichnis = daten.len();
    let mut mitte = Vec::new();
    mitte.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
    mitte.extend_from_slice(&20u16.to_le_bytes());
    mitte.extend_from_slice(&20u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u32.to_le_bytes());
    mitte.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    mitte.extend_from_slice(&(inhalt.len() as u32).to_le_bytes());
    mitte.extend_from_slice(&(name.len() as u16).to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u16.to_le_bytes());
    mitte.extend_from_slice(&0u32.to_le_bytes());
    mitte.extend_from_slice(&(kopf as u32).to_le_bytes());
    mitte.extend_from_slice(name.as_bytes());
    daten.extend_from_slice(&mitte);
    daten.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten.extend_from_slice(&1u16.to_le_bytes());
    daten.extend_from_slice(&1u16.to_le_bytes());
    daten.extend_from_slice(&(mitte.len() as u32).to_le_bytes());
    daten.extend_from_slice(&(verzeichnis as u32).to_le_bytes());
    daten.extend_from_slice(&0u16.to_le_bytes());
    daten
}

fn rechte_setzen(pfad: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut rechte = std::fs::metadata(pfad).expect("Rechte").permissions();
    rechte.set_mode(0o755);
    std::fs::set_permissions(pfad, rechte).expect("Rechte setzen");
}

/// Eine Attrappe der Veröffentlichung (GitHub): Antwort, Prüfsummen, Paket.
async fn attrappe_starten(version: &str, paket: Vec<u8>, paketname: String) -> String {
    let sums = Arc::new(Mutex::new(String::new()));
    let bytes = Arc::new(Mutex::new(paket));
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("Port frei");
    let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
    let version = version.to_string();
    let router = {
        let sums = sums.clone();
        let bytes = bytes.clone();
        let basis = basis.clone();
        let paketname = paketname.clone();
        Router::new()
            .route(
                "/releases/latest",
                get({
                    let basis = basis.clone();
                    let version = version.clone();
                    let paketname = paketname.clone();
                    let bytes = bytes.clone();
                    move || {
                        let basis = basis.clone();
                        let version = version.clone();
                        let paketname = paketname.clone();
                        let bytes = bytes.clone();
                        async move {
                            let groesse = bytes.lock().await.len();
                            let json = json!({
                                "tag_name": format!("v{version}"),
                                "html_url": format!("https://example.test/releases/tag/v{version}"),
                                "published_at": "2026-11-25T10:00:00Z",
                                "draft": false,
                                "prerelease": false,
                                "assets": [
                                    {
                                        "name": paketname,
                                        "browser_download_url": format!("{basis}/download/{paketname}"),
                                        "size": groesse,
                                    },
                                    {
                                        "name": "SHA256SUMS",
                                        "browser_download_url": format!("{basis}/download/SHA256SUMS"),
                                        "size": 128,
                                    }
                                ]
                            });
                            (StatusCode::OK, serde_json::to_string(&json).expect("JSON"))
                        }
                    }
                }),
            )
            .route(
                "/download/{name}",
                get({
                    let sums = sums.clone();
                    let bytes = bytes.clone();
                    let paketname = paketname.clone();
                    move |Pfad(name): Pfad<String>| {
                        let sums = sums.clone();
                        let bytes = bytes.clone();
                        let paketname = paketname.clone();
                        async move {
                            if name == "SHA256SUMS" {
                                return (StatusCode::OK, sums.lock().await.clone().into_bytes());
                            }
                            if name == paketname {
                                return (StatusCode::OK, bytes.lock().await.clone());
                            }
                            (StatusCode::NOT_FOUND, Vec::new())
                        }
                    }
                }),
            )
    };
    *sums.lock().await = format!("{}  {paketname}\n", sha256(&bytes.lock().await.clone()));
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    basis
}

/// Alles, was ein Update-Durchlauf braucht.
struct Welt {
    ordner: PathBuf,
    profil: PathBuf,
    daten: PathBuf,
    basis: String,
    ziel: PathBuf,
    http: reqwest::Client,
    cookie: String,
    kern: Kern,
}

impl Welt {
    /// `version`: was die Attrappe als neueste Version nennt.
    async fn neu(version: &str) -> Self {
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let ordner = std::env::temp_dir().join(format!(
            "quiver-client-update-{}-{nummer}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).expect("Ordner");
        let daten = ordner.join("server");
        let profil = ordner.join("profil");
        let ziel = ordner.join("programm");
        std::fs::create_dir_all(&ziel).expect("Programmordner");

        // Die Attrappe liefert ein ZIP mit dem neuen Programm.
        let name = paketname(version);
        let paket = zip_mit("quiver", NEU.as_bytes());
        let attrappe = attrappe_starten(version, paket, name).await;

        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("Port frei");
        let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
        let mut konfig = quiver_server::Konfig::neu(&daten);
        konfig.oeffentlich = basis.clone();
        konfig.programme = vec![daten.join("tools")];
        konfig.updates.an = true;
        konfig.updates.quelle = attrappe;
        let router = quiver_server::app(konfig).await.expect("App baut");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let http = reqwest::Client::new();
        let cookie = gemeinsam::sitzung(&http, &basis, &daten).await;

        // Das laufende Programm der Attrappe (im Testordner).
        let programm = ziel.join("quiver");
        std::fs::write(&programm, ALT).expect("alte Fassung");
        rechte_setzen(&programm);

        let kern = Kern::starten(KernKonfig {
            profil: profil.clone(),
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: Arc::new(SpeicherTresor::neu()),
            browser: Arc::new(quiver_client::SystemBrowser),
            vorgaben: quiver_client::Vorgaben::default(),
            aufstellung: Aufstellung {
                art: Installationsart::Zip,
                programm,
                schreibbar: true,
            },
        })
        .await
        .expect("Kern startet");
        let mut welt = Self {
            ordner,
            profil,
            daten,
            basis,
            ziel,
            http,
            cookie,
            kern,
        };
        welt.einbinden().await;
        welt
    }

    async fn einbinden(&mut self) {
        let antwort = self
            .http
            .post(format!("{}/api/v1/admin/codes", self.basis))
            .header("cookie", &self.cookie)
            .header("origin", &self.basis)
            .header("x-quiver", "1")
            .header("content-type", "application/json")
            .body(serde_json::to_vec(&json!({"gruppen": [], "minuten": 60})).expect("JSON"))
            .send()
            .await
            .expect("Code");
        let rumpf: serde_json::Value = antwort.json().await.expect("Code");
        let code = rumpf["code"].as_str().expect("Code").to_string();
        self.kern
            .einbinden(&self.basis, &code, "Testrechner")
            .await
            .expect("Einbinden klappt");
    }

    fn lage(&self) -> UpdateLage {
        self.kern.modell().borrow().update.clone()
    }

    async fn warte_auf_lage(&self, pruefung: impl Fn(&UpdateLage) -> bool) -> UpdateLage {
        let ende = Instant::now() + FRIST;
        loop {
            let lage = self.lage();
            if pruefung(&lage) {
                return lage;
            }
            assert!(Instant::now() < ende, "Zeit abgelaufen, Lage: {lage:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Der Inhalt des Profils, so wie er auf der Platte liegt.
    fn profil_inhalt(&self) -> Vec<(String, Vec<u8>)> {
        let mut liste = Vec::new();
        if let Ok(eintraege) = std::fs::read_dir(&self.profil) {
            for eintrag in eintraege.flatten() {
                if let Ok(inhalt) = std::fs::read(eintrag.path()) {
                    liste.push((eintrag.file_name().to_string_lossy().to_string(), inhalt));
                }
            }
        }
        liste.sort();
        liste
    }
}

impl Drop for Welt {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
        let _ = &self.daten;
    }
}

#[tokio::test]
async fn der_client_bietet_das_update_an_und_tauscht_sich_selbst() {
    let welt = Welt::neu("9.0.0").await;
    welt.kern.update_pruefen().await.expect("gefragt");
    let lage = welt
        .warte_auf_lage(|lage| matches!(lage, UpdateLage::Verfuegbar { .. }))
        .await;
    let UpdateLage::Verfuegbar {
        version,
        hinweis,
        angebot,
        ..
    } = lage
    else {
        panic!("erwartet: verfügbar");
    };
    assert_eq!(version, "9.0.0");
    assert_eq!(hinweis, "Version 9.0.0 ist verfügbar");
    let Angebot::Bereit { paket } = angebot else {
        panic!("erwartet: ein Angebot");
    };
    assert_eq!(paket.name, paketname("9.0.0"));
    assert_eq!(
        paket.sha256,
        sha256(zip_mit("quiver", NEU.as_bytes()).as_slice())
    );

    // Vor dem Tausch: das Profil, wie es ist.
    let vorher = welt.profil_inhalt();
    let programm = welt.ziel.join("quiver");
    assert_eq!(std::fs::read(&programm).expect("alt"), ALT.as_bytes());

    welt.kern.update_installieren().await.expect("getauscht");
    let lage = welt
        .warte_auf_lage(|lage| matches!(lage, UpdateLage::Getauscht { .. }))
        .await;
    assert!(matches!(lage, UpdateLage::Getauscht { .. }));

    // Die neue Fassung liegt am Platz, die alte als Sicherung daneben.
    assert_eq!(std::fs::read(&programm).expect("neu"), NEU.as_bytes());
    assert_eq!(
        std::fs::read(welt.ziel.join("quiver.alt")).expect("Sicherung"),
        ALT.as_bytes()
    );
    // Der Neustart ist vorgemerkt (das Fenster führt ihn nach dem Beenden aus).
    assert!(
        welt.profil.join("update-neustart.json").is_file(),
        "der Neustart ist nicht vorgemerkt"
    );
    // Das Profil ist unverändert: gleiche Dateien, gleicher Inhalt.
    let nachher: Vec<(String, Vec<u8>)> = welt
        .profil_inhalt()
        .into_iter()
        .filter(|(name, _)| name != "update-neustart.json" && name != "update-offen.json")
        .collect();
    assert_eq!(vorher, nachher, "das Profil wurde angefasst");

    // Der Selbsttest der neuen Fassung läuft wirklich (Exit 0).
    assert!(quiver_client::update::selbsttest(&programm));

    // Aufräumen: Marke und Neustart gehören dem nächsten Start.
    quiver_client::update::tausch::bestaetigen(&welt.profil);
    assert!(
        !welt.ziel.join("quiver.alt").exists(),
        "Sicherung bleibt sonst liegen"
    );
    welt.kern.beenden().await;
}

#[tokio::test]
async fn eine_vorgabe_schaltet_das_angebot_ab() {
    // Dieselbe Lage, aber die Firma will kein Selbst-Update.
    let ordner = std::env::temp_dir().join(format!(
        "quiver-client-update-vorgabe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dauer| dauer.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&ordner).expect("Ordner");
    let daten = ordner.join("server");
    let profil = ordner.join("profil");
    let ziel = ordner.join("programm");
    std::fs::create_dir_all(&ziel).expect("Ordner");
    let name = paketname("9.0.0");
    let attrappe = attrappe_starten("9.0.0", zip_mit("quiver", NEU.as_bytes()), name).await;
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("Port frei");
    let basis = format!("http://{}", listener.local_addr().expect("Adresse"));
    let mut konfig = quiver_server::Konfig::neu(&daten);
    konfig.oeffentlich = basis.clone();
    konfig.programme = vec![daten.join("tools")];
    konfig.updates.quelle = attrappe;
    let router = quiver_server::app(konfig).await.expect("App baut");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let http = reqwest::Client::new();
    let cookie = gemeinsam::sitzung(&http, &basis, &daten).await;
    let programm = ziel.join("quiver");
    std::fs::write(&programm, ALT).expect("alt");
    rechte_setzen(&programm);
    let kern = Kern::starten(KernKonfig {
        profil: profil.clone(),
        mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
        tresor: Arc::new(SpeicherTresor::neu()),
        browser: Arc::new(quiver_client::SystemBrowser),
        vorgaben: quiver_client::Vorgaben {
            update_angebot: Some(false),
            ..quiver_client::Vorgaben::default()
        },
        aufstellung: Aufstellung {
            art: Installationsart::Zip,
            programm: programm.clone(),
            schreibbar: true,
        },
    })
    .await
    .expect("Kern startet");
    let antwort = http
        .post(format!("{basis}/api/v1/admin/codes"))
        .header("cookie", &cookie)
        .header("origin", &basis)
        .header("x-quiver", "1")
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&json!({"gruppen": [], "minuten": 60})).expect("JSON"))
        .send()
        .await
        .expect("Code");
    let rumpf: serde_json::Value = antwort.json().await.expect("Code");
    let code = rumpf["code"].as_str().expect("Code").to_string();
    kern.einbinden(&basis, &code, "Testrechner")
        .await
        .expect("Einbinden");
    kern.update_pruefen().await.expect("gefragt");
    let lage = kern.modell().borrow().update.clone();
    let UpdateLage::Verfuegbar { angebot, .. } = lage else {
        panic!("erwartet: verfügbar, war {lage:?}");
    };
    assert_eq!(
        angebot,
        Angebot::Keins {
            grund: AngebotGrund::Vorgabe
        },
        "der Hinweis bleibt, der Knopf nicht"
    );
    // Und der Tausch ist gar nicht erst möglich.
    assert!(kern.update_installieren().await.is_err());
    assert_eq!(std::fs::read(&programm).expect("alt"), ALT.as_bytes());
    kern.beenden().await;
    let _ = std::fs::remove_dir_all(&ordner);
}

#[tokio::test]
async fn keine_vorabversion_und_kein_rueckschritt() {
    let welt = Welt::neu("0.0.1").await;
    welt.kern.update_pruefen().await.expect("gefragt");
    let lage = welt.lage();
    assert_eq!(
        lage,
        UpdateLage::Keins,
        "eine ältere Version ist kein Angebot"
    );
    welt.kern.beenden().await;
}

#[tokio::test]
async fn spaeter_blendet_den_hinweis_aus() {
    let welt = Welt::neu("9.0.0").await;
    welt.kern.update_pruefen().await.expect("gefragt");
    welt.warte_auf_lage(|lage| matches!(lage, UpdateLage::Verfuegbar { .. }))
        .await;
    welt.kern.update_spaeter();
    assert_eq!(
        welt.lage(),
        UpdateLage::Keins,
        "„Später“ nimmt den Hinweis weg"
    );
    welt.kern.beenden().await;
}
