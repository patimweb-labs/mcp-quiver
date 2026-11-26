// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Einstellungen der Tools sichern und auf einem zweiten Gerät einspielen
//! (Aufgabe 15, `docs/aufgaben/15-export-import.md`).
//!
//! Geprüft wird der Ablauf aus der Aufgabe:
//! 1. Export und Import auf einem frischen Kern ergeben dieselben Werte und
//!    Zugänge – mit und ohne Geheimnisse.
//! 2. Ohne Passwort steht kein Geheimnis in der Datei.
//! 3. Ein falsches Passwort ist ein Fehler, der Tresor bleibt unverändert.
//! 4. Gemischt, nicht ersetzt: was nur am Gerät steht, bleibt.
//! 5. Ein Eintrag, den es hier nicht gibt, wird übersprungen und gemeldet.
//!
//! Jedes „Gerät“ ist ein eigener Kern mit eigenem Profilordner. Den Soll-Zustand
//! schreibt der Test vorher in `zustand.json`: der Kern lädt ihn beim Start, ohne
//! dass ein Server laufen muss.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use quiver_client::{Kern, KernKonfig, SpeicherTresor, Tresor, VorschauArt};
use quiver_domain::{
    Eintrag, EintragId, Feld, Herkunft, Laufzeit, Mehrfach, Quelle, Soll, SollEintrag, Werte,
};

// ------------------------------------------------------------- Testgerüst

/// Ein Gerät: eigener Ordner, eigener Kern, eigener Tresor.
struct Geraet {
    ordner: PathBuf,
    kern: Kern,
    tresor: Arc<SpeicherTresor>,
}

impl Geraet {
    /// Ein frisches Gerät mit diesem Soll-Zustand und diesen lokalen Werten.
    async fn neu(soll: &Soll, werte: BTreeMap<String, Werte>, zugaenge: Zugaenge) -> Self {
        Self::mit_geheimnissen(soll, werte, zugaenge, &[]).await
    }

    /// Wie [`Geraet::neu`], aber der Tresor ist schon gefüllt – der Kern liest
    /// die Geheimnisse beim Start.
    async fn mit_geheimnissen(
        soll: &Soll,
        werte: BTreeMap<String, Werte>,
        zugaenge: Zugaenge,
        geheimnisse: &[(&str, &str)],
    ) -> Self {
        let ordner = neuer_ordner();
        let profil = ordner.join("profil");
        std::fs::create_dir_all(&profil).expect("Profilordner");
        let stand = serde_json::json!({
            "lokale_werte": werte,
            "zugaenge": zugaenge,
            "soll": soll,
        });
        std::fs::write(
            profil.join("zustand.json"),
            serde_json::to_vec_pretty(&stand).expect("JSON"),
        )
        .expect("Profil schreibbar");
        let tresor = Arc::new(SpeicherTresor::neu());
        for (schluessel, wert) in geheimnisse {
            tresor
                .schreiben(schluessel, wert)
                .expect("Tresor schreibbar");
        }
        let kern = Kern::starten(KernKonfig {
            profil,
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 0)),
            tresor: tresor.clone(),
            browser: std::sync::Arc::new(quiver_client::SystemBrowser),
        })
        .await
        .expect("Kern startet");
        Self {
            ordner,
            kern,
            tresor,
        }
    }

    /// Was der Kern in `zustand.json` abgelegt hat.
    fn gespeichert(&self) -> serde_json::Value {
        let text = std::fs::read_to_string(self.ordner.join("profil/zustand.json"))
            .expect("Profil lesbar");
        serde_json::from_str(&text).expect("Profil ist JSON")
    }

    fn datei(&self, name: &str) -> PathBuf {
        self.ordner.join(name)
    }

    async fn beenden(self) {
        self.kern.beenden().await;
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

type Zugaenge = BTreeMap<String, BTreeMap<String, Werte>>;

fn neuer_ordner() -> PathBuf {
    static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
    let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
    let stempel = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Systemzeit nach 1970")
        .as_nanos();
    let ordner = std::env::temp_dir().join(format!(
        "quiver-client-einstellungen-{}-{nummer}-{stempel}",
        std::process::id()
    ));
    std::fs::create_dir_all(&ordner).expect("Ordner");
    ordner
}

fn werte(paare: &[(&str, &str)]) -> Werte {
    paare
        .iter()
        .map(|(name, wert)| ((*name).to_string(), (*wert).to_string()))
        .collect()
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

/// Ein Eintrag, der ohne das Pflichtfeld `PFLICHT` nicht startet – so bleibt
/// der Test frei von Kindprozessen.
fn eintrag(id: &str, titel: &str, mehrfach: Option<Mehrfach>) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "1.0.0".into(),
        titel: titel.into(),
        beschreibung: format!("Beschreibung {titel}"),
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach,
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: "quiver-gibt-es-nicht".into(),
            argumente: Vec::new(),
            umgebung: Werte::new(),
        },
        felder: vec![
            // Fehlt immer: der Eintrag wartet auf Werte und startet nichts.
            feld("PFLICHT", Quelle::Client, true),
            feld("anmelde_passwort", Quelle::Geheim, false),
        ],
    }
}

/// Die Zugangsliste wie bei quiver-ssh (Aufgabe 14).
fn mehrfach() -> Mehrfach {
    Mehrfach {
        titel: "Verbindungen".into(),
        einzahl: "Verbindung".into(),
        felder: vec![
            feld("host", Quelle::Client, true),
            feld("port", Quelle::Client, false),
            feld("sudo_passwort", Quelle::Geheim, false),
        ],
    }
}

fn soll(ids: &[&str]) -> Soll {
    Soll::neu(
        ids.iter()
            .map(|id| SollEintrag {
                eintrag: eintrag(id, &format!("Titel {id}"), (id == &"ssh").then(mehrfach)),
                werte: Werte::new(),
                fest: Default::default(),
            })
            .collect(),
    )
}

// --------------------------------------------------------------- Die Tests

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rundlauf_ohne_passwort_ergibt_dieselben_werte_und_zugaenge() {
    let soll = soll(&["ssh", "echo"]);
    let mut zugaenge = Zugaenge::new();
    zugaenge.insert(
        "ssh".to_string(),
        BTreeMap::from([(
            "web-01".to_string(),
            werte(&[("host", "web-01.example"), ("port", "22")]),
        )]),
    );
    let a = Geraet::mit_geheimnissen(
        &soll,
        BTreeMap::from([
            ("ssh".to_string(), werte(&[("PFLICHT", "da")])),
            ("echo".to_string(), werte(&[("PFA D", "/tmp")])),
        ]),
        zugaenge,
        // Ein Geheimnis liegt im Tresor – ohne Passwort darf es nicht mit.
        &[("wert/ssh/anmelde_passwort", "streng-geheim")],
    )
    .await;

    let pfad = a.datei("sicherung.json");
    let zusammenfassung = a
        .kern
        .einstellungen_exportieren(&pfad, None)
        .expect("Export klappt");
    assert_eq!(zusammenfassung.eintraege, 2);
    assert_eq!(zusammenfassung.zugaenge, 1);
    assert_eq!(zusammenfassung.geheimnisse, 0);
    assert!(!zusammenfassung.mit_geheimnissen());

    let text = std::fs::read_to_string(&pfad).expect("Datei lesbar");
    assert!(
        !text.contains("streng-geheim"),
        "ohne Passwort steht kein Geheimnis in der Datei"
    );
    assert!(text.contains("mcp-quiver/einstellungen"), "{text}");
    assert!(text.contains("\"geheim\": null"), "{text}");

    // Ein frisches Gerät mit demselben Soll-Zustand.
    let b = Geraet::neu(&soll, BTreeMap::new(), Zugaenge::new()).await;
    let vorschau = b
        .kern
        .einstellungen_pruefen(&pfad, None)
        .expect("Vorschau klappt");
    assert!(!vorschau.hat_geheimnisse);
    assert_eq!(vorschau.uebernommen(), 2);
    assert_eq!(vorschau.uebersprungen(), 0);
    for eintrag in &vorschau.eintraege {
        assert_eq!(eintrag.art, VorschauArt::Neu, "{}", eintrag.id);
        assert!(!eintrag.titel.is_empty());
    }

    let zusammenfassung = b
        .kern
        .einstellungen_importieren(&pfad, None)
        .await
        .expect("Import klappt");
    assert_eq!(zusammenfassung.eintraege, 2);
    assert_eq!(zusammenfassung.geheimnisse, 0);
    assert!(zusammenfassung.uebersprungen.is_empty());

    // Dieselben Werte und Zugänge wie auf dem ersten Gerät.
    let stand_a = a.gespeichert();
    let stand_b = b.gespeichert();
    assert_eq!(stand_b["lokale_werte"], stand_a["lokale_werte"]);
    assert_eq!(stand_b["zugaenge"], stand_a["zugaenge"]);
    assert_eq!(stand_b["lokale_werte"]["ssh"]["PFLICHT"], "da");
    assert_eq!(
        stand_b["zugaenge"]["ssh"]["web-01"]["host"],
        "web-01.example"
    );
    assert!(
        b.tresor.alles().is_empty(),
        "ohne Passwort kommt kein Geheimnis mit"
    );

    a.beenden().await;
    b.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rundlauf_mit_geheimnissen_und_falschem_passwort() {
    let soll = soll(&["ssh"]);
    let mut zugaenge = Zugaenge::new();
    zugaenge.insert(
        "ssh".to_string(),
        BTreeMap::from([(
            "web-01".to_string(),
            werte(&[("host", "web-01.example"), ("port", "22")]),
        )]),
    );
    let a = Geraet::mit_geheimnissen(
        &soll,
        BTreeMap::from([("ssh".to_string(), werte(&[("PFLICHT", "da")]))]),
        zugaenge,
        &[
            ("wert/ssh/anmelde_passwort", "streng-geheim"),
            ("zugang/ssh/web-01/sudo_passwort", "auch-geheim"),
        ],
    )
    .await;

    let pfad = a.datei("sicherung.json");
    let zusammenfassung = a
        .kern
        .einstellungen_exportieren(&pfad, Some("ein-langes-passwort"))
        .expect("Export klappt");
    assert_eq!(zusammenfassung.geheimnisse, 2);
    assert!(zusammenfassung.mit_geheimnissen());
    let text = std::fs::read_to_string(&pfad).expect("Datei lesbar");
    for geheim in ["streng-geheim", "auch-geheim"] {
        assert!(!text.contains(geheim), "Klartext in der Datei: {geheim}");
    }

    let b = Geraet::neu(&soll, BTreeMap::new(), Zugaenge::new()).await;
    // Die Vorschau geht auch ohne Passwort – sie zeigt nur keine Geheimnisse.
    let vorschau = b
        .kern
        .einstellungen_pruefen(&pfad, None)
        .expect("Vorschau klappt");
    assert!(vorschau.hat_geheimnisse, "die Datei führt Geheimnisse");
    assert_eq!(vorschau.uebernommen(), 1);

    // Ein falsches Passwort ist ein Fehler – und ändert nichts.
    let fehler = b
        .kern
        .einstellungen_importieren(&pfad, Some("falsches-passwort"))
        .await
        .expect_err("falsches Passwort");
    assert!(
        format!("{fehler:#}").contains("Passwort ist falsch"),
        "{fehler:#}"
    );
    assert!(b.tresor.alles().is_empty(), "der Tresor bleibt unverändert");
    let leer = b.gespeichert();
    assert_eq!(leer["lokale_werte"], serde_json::json!({}));
    assert_eq!(leer["zugaenge"], serde_json::json!({}));

    // Ohne Passwort wird ebenfalls nichts übernommen.
    let fehler = b
        .kern
        .einstellungen_importieren(&pfad, None)
        .await
        .expect_err("ohne Passwort");
    assert!(format!("{fehler:#}").contains("Passwort"), "{fehler:#}");

    // Mit dem richtigen Passwort kommt alles an – Werte und Geheimnisse.
    let zusammenfassung = b
        .kern
        .einstellungen_importieren(&pfad, Some("ein-langes-passwort"))
        .await
        .expect("Import klappt");
    assert_eq!(zusammenfassung.geheimnisse, 2);
    let tresor = b.tresor.alles();
    assert_eq!(
        tresor.get("wert/ssh/anmelde_passwort").map(String::as_str),
        Some("streng-geheim")
    );
    assert_eq!(
        tresor
            .get("zugang/ssh/web-01/sudo_passwort")
            .map(String::as_str),
        Some("auch-geheim")
    );
    let stand = b.gespeichert();
    assert_eq!(stand["lokale_werte"]["ssh"]["PFLICHT"], "da");
    assert_eq!(stand["zugaenge"]["ssh"]["web-01"]["host"], "web-01.example");

    a.beenden().await;
    b.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mischen_behaelt_werte_die_nur_am_geraet_stehen() {
    let soll = soll(&["ssh"]);
    let a = Geraet::neu(
        &soll,
        BTreeMap::from([("ssh".to_string(), werte(&[("PFLICHT", "vom-ersten")]))]),
        Zugaenge::new(),
    )
    .await;
    let pfad = a.datei("sicherung.json");
    a.kern
        .einstellungen_exportieren(&pfad, None)
        .expect("Export klappt");

    let b = Geraet::neu(
        &soll,
        BTreeMap::from([(
            "ssh".to_string(),
            werte(&[("PFLICHT", "vom-zweiten"), ("NUR_HIER", "bleibt")]),
        )]),
        Zugaenge::new(),
    )
    .await;
    let vorschau = b
        .kern
        .einstellungen_pruefen(&pfad, None)
        .expect("Vorschau klappt");
    let zeile = &vorschau.eintraege[0];
    assert_eq!(zeile.art, VorschauArt::Aendert, "der Eintrag ist da");
    assert_eq!(zeile.neue, 0, "PFLICHT gibt es hier schon");
    assert_eq!(zeile.geaenderte, 1);

    b.kern
        .einstellungen_importieren(&pfad, None)
        .await
        .expect("Import klappt");
    let stand = b.gespeichert();
    assert_eq!(
        stand["lokale_werte"]["ssh"]["PFLICHT"], "vom-ersten",
        "gleichnamige Werte überschreibt die Datei"
    );
    assert_eq!(
        stand["lokale_werte"]["ssh"]["NUR_HIER"], "bleibt",
        "was nur am Gerät steht, bleibt"
    );

    a.beenden().await;
    b.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unbekannter_eintrag_wird_uebersprungen_und_gemeldet() {
    // Das erste Gerät kennt einen Eintrag mehr als das zweite.
    let a = Geraet::neu(
        &soll(&["ssh", "nur-hier"]),
        BTreeMap::from([
            ("ssh".to_string(), werte(&[("PFLICHT", "da")])),
            ("nur-hier".to_string(), werte(&[("PFLICHT", "auch")])),
        ]),
        Zugaenge::new(),
    )
    .await;
    let pfad = a.datei("sicherung.json");
    a.kern
        .einstellungen_exportieren(&pfad, None)
        .expect("Export klappt");

    let b = Geraet::neu(&soll(&["ssh"]), BTreeMap::new(), Zugaenge::new()).await;
    let vorschau = b
        .kern
        .einstellungen_pruefen(&pfad, None)
        .expect("Vorschau klappt");
    assert_eq!(vorschau.uebernommen(), 1);
    assert_eq!(vorschau.uebersprungen(), 1);
    let weg = vorschau
        .eintraege
        .iter()
        .find(|eintrag| eintrag.id == "nur-hier")
        .expect("der unbekannte Eintrag steht in der Vorschau");
    assert_eq!(weg.art, VorschauArt::Uebersprungen);
    assert_eq!(weg.titel, "nur-hier", "ohne Eintrag hier nur die Kennung");

    let zusammenfassung = b
        .kern
        .einstellungen_importieren(&pfad, None)
        .await
        .expect("Import klappt");
    assert_eq!(zusammenfassung.eintraege, 1);
    assert_eq!(zusammenfassung.uebersprungen, vec!["nur-hier".to_string()]);
    let stand = b.gespeichert();
    assert!(
        stand["lokale_werte"].get("nur-hier").is_none(),
        "ein unbekannter Eintrag wird nicht angelegt"
    );
    assert_eq!(stand["lokale_werte"]["ssh"]["PFLICHT"], "da");

    a.beenden().await;
    b.beenden().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eine_fremde_datei_ist_ein_fehler_ohne_inhalt() {
    let b = Geraet::neu(&soll(&["ssh"]), BTreeMap::new(), Zugaenge::new()).await;
    let pfad = b.datei("fremd.json");
    std::fs::write(
        &pfad,
        serde_json::json!({
            "format": "etwas-anderes",
            "version": 1,
            "eintraege": { "ssh": { "werte": { "PFLICHT": "streng-geheim" } } }
        })
        .to_string(),
    )
    .expect("Datei schreibbar");
    for ergebnis in [
        b.kern
            .einstellungen_pruefen(&pfad, None)
            .err()
            .map(|e| format!("{e:#}")),
        b.kern
            .einstellungen_importieren(&pfad, None)
            .await
            .err()
            .map(|e| format!("{e:#}")),
    ] {
        let fehler = ergebnis.expect("muss scheitern");
        assert!(!fehler.contains("streng-geheim"), "{fehler}");
        assert!(fehler.contains("keine mcp-quiver-Sicherung"), "{fehler}");
    }
    assert!(b.gespeichert()["lokale_werte"]
        .as_object()
        .unwrap()
        .is_empty());
    b.beenden().await;
}

/// Die Rechte der Datei sind 0600 – auch beim Export über den Kern.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn die_sicherung_gehoert_nur_dem_benutzer() {
    use std::os::unix::fs::PermissionsExt;
    let a = Geraet::neu(
        &soll(&["ssh"]),
        BTreeMap::from([("ssh".to_string(), werte(&[("PFLICHT", "da")]))]),
        Zugaenge::new(),
    )
    .await;
    let pfad = a.datei("sicherung.json");
    a.kern
        .einstellungen_exportieren(&pfad, None)
        .expect("Export klappt");
    let modus = std::fs::metadata(&pfad)
        .expect("Datei da")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(modus, 0o600, "Rechte der Sicherung");
    a.beenden().await;
}

/// Der Dateiname, den der Speichern-Dialog vorschlägt.
#[test]
fn der_dateiname_traegt_das_datum() {
    let name = quiver_client::einstellungen::dateiname();
    assert!(name.starts_with("quiver-einstellungen-"), "{name}");
    assert!(Path::new(&name)
        .extension()
        .is_some_and(|ext| ext == "json"));
}
