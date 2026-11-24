// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Geheimnisse in einer eigenen, verschlüsselten Datei – ohne Schlüsselbund
//! des Betriebssystems.
//!
//! Warum nicht der Schlüsselbund: macOS fragt nach jedem neuen Build erneut
//! nach („Immer erlauben“), und unter Linux gibt es ihn je nach Distribution
//! gar nicht. Die Datei verhält sich überall gleich.
//!
//! Aufbau im Ordner (0700):
//! - `tresor.schluessel` – 32 zufällige Bytes (0600)
//! - `tresor.json` – `{"version":1,"nonce":…,"daten":…}`, die Daten sind die
//!   Schlüssel/Wert-Liste als JSON, verschlüsselt mit XChaCha20-Poly1305 (0600)
//!
//! Was das schützt – und was nicht: Wer den Benutzer-Account hat, kann beide
//! Dateien lesen; dagegen hilft nur ein Passwort beim Start. Die
//! Verschlüsselung verhindert, dass beim versehentlichen Lesen der Datei
//! (`cat`, `grep`, ein Agent, eine Sicherung nur von `tresor.json`) Klartext
//! herauskommt.
//!
//! Mehrere Prozesse dürfen gleichzeitig schreiben: jede Änderung liest die
//! Datei unter einer Dateisperre neu, ändert einen Wert und ersetzt die Datei
//! atomar (schreiben, dann umbenennen).

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;

/// So lange wird auf die Sperre des Tresors gewartet, bevor der Aufruf
/// aufgibt. Ohne Frist hing der Kern an einem Vorgang, der nie endet.
pub const SPERRE_FRIST: Duration = Duration::from_secs(5);
/// Der Abstand zwischen zwei Versuchen, die Sperre zu bekommen.
const SPERRE_ABSTAND: Duration = Duration::from_millis(20);

/// Ein Tresor in einem Ordner.
pub struct DateiTresor {
    ordner: PathBuf,
    /// `Some(text)`, wenn der Tresor nicht lesbar war. Dann wird **nichts**
    /// geschrieben und kein neuer Schlüssel erzeugt, bis der User „Neu
    /// beginnen“ wählt.
    kaputt: Mutex<Option<String>>,
}

impl std::fmt::Debug for DateiTresor {
    /// Nie Werte, nur der Ordner.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DateiTresor")
            .field("ordner", &self.ordner)
            .finish_non_exhaustive()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Huelle {
    version: u32,
    nonce: String,
    daten: String,
}

impl DateiTresor {
    /// Ein Tresor im Ordner `ordner` (wird bei Bedarf angelegt).
    pub fn im_ordner(ordner: impl Into<PathBuf>) -> Self {
        Self {
            ordner: ordner.into(),
            kaputt: Mutex::new(None),
        }
    }

    /// Der Ordner des Tresors.
    pub fn ordner(&self) -> &Path {
        &self.ordner
    }

    /// Warum der Tresor nicht lesbar ist (für das Fenster).
    pub fn fehler(&self) -> Option<String> {
        self.kaputt
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clone()
    }

    /// „Neu beginnen“: erst jetzt werden die alten Dateien beiseitegelegt
    /// (`<name>.kaputt-<zeit>`), nie überschrieben oder gelöscht. Ist der
    /// Tresor lesbar, geschieht nichts.
    pub fn neu_beginnen(&self) -> anyhow::Result<()> {
        let _sperre = self.sperren()?;
        let mut kaputt = self
            .kaputt
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        if kaputt.is_none() {
            return Ok(());
        }
        for name in ["tresor.json", "tresor.schluessel"] {
            let pfad = self.ordner.join(name);
            if pfad.exists() {
                let ziel = kaputt_pfad(&pfad);
                if ziel.exists() {
                    anyhow::bail!(
                        "{} gibt es schon – bitte gleich noch einmal",
                        ziel.display()
                    );
                }
                fs::rename(&pfad, &ziel)?;
            }
        }
        *kaputt = None;
        Ok(())
    }

    pub fn lesen(&self, schluessel: &str) -> anyhow::Result<Option<String>> {
        let _sperre = self.sperren()?;
        Ok(self.laden()?.get(schluessel).cloned())
    }

    /// Viele Schlüssel in **einem** Zugriff: eine Sperre, eine Entschlüsselung.
    pub fn werte_laden(&self, schluessel: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
        let _sperre = self.sperren()?;
        let werte = self.laden()?;
        Ok(schluessel
            .iter()
            .filter_map(|name| werte.get(name).map(|wert| (name.clone(), wert.clone())))
            .collect())
    }

    pub fn schreiben(&self, schluessel: &str, wert: &str) -> anyhow::Result<()> {
        let _sperre = self.sperren()?;
        let mut werte = self.laden()?;
        werte.insert(schluessel.to_string(), wert.to_string());
        self.speichern(&werte)
    }

    pub fn loeschen(&self, schluessel: &str) -> anyhow::Result<()> {
        let _sperre = self.sperren()?;
        let mut werte = self.laden()?;
        if werte.remove(schluessel).is_some() {
            self.speichern(&werte)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ innen

    fn ordner_anlegen(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.ordner)?;
        rechte(&self.ordner, 0o700);
        Ok(())
    }

    /// Eine Sperre über alle Prozesse (`tresor.sperre`), solange der Wert lebt.
    /// Mit Frist: hängt ein anderer Prozess, gibt der Aufruf auf, statt zu
    /// warten, bis das Programm beendet wird.
    fn sperren(&self) -> anyhow::Result<File> {
        self.ordner_anlegen()?;
        let datei = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.ordner.join("tresor.sperre"))?;
        let beginn = Instant::now();
        loop {
            match datei.try_lock() {
                Ok(()) => return Ok(datei),
                Err(std::fs::TryLockError::WouldBlock) if beginn.elapsed() < SPERRE_FRIST => {
                    std::thread::sleep(SPERRE_ABSTAND);
                }
                Err(_) => {
                    anyhow::bail!("Der Tresor ist gerade gesperrt – bitte gleich noch einmal")
                }
            }
        }
    }

    /// Der Zustand des Tresors, wenn er nicht lesbar ist. `Ok(())` heißt:
    /// alles in Ordnung.
    fn pruefen(&self) -> anyhow::Result<()> {
        match self
            .kaputt
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clone()
        {
            Some(text) => anyhow::bail!(text),
            None => Ok(()),
        }
    }

    /// Den Tresor als unlesbar kennzeichnen. Die Dateien bleiben **unberührt**:
    /// wer den Schlüssel oder eine Sicherung zurückspielt, kann nach einem
    /// Neustart weiterarbeiten, und ein Neustart beginnt nie still von vorn.
    fn kaputt(&self, text: impl Into<String>) -> anyhow::Error {
        let text = text.into();
        tracing::warn!("Tresor nicht lesbar: {text}");
        *self
            .kaputt
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner()) = Some(text.clone());
        anyhow::anyhow!("{text} – Datei zurückspielen und neu starten oder „Neu beginnen“ wählen")
    }

    fn schluessel(&self) -> anyhow::Result<[u8; 32]> {
        self.pruefen()?;
        let pfad = self.ordner.join("tresor.schluessel");
        match fs::read(&pfad) {
            Ok(bytes) => bytes
                .try_into()
                .map_err(|_| self.kaputt(format!("{} ist beschädigt", pfad.display()))),
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => {
                // Ein Tresor **ohne** Schlüssel wird nie stillschweigend mit
                // einem neuen Schlüssel überschrieben: die Werte wären sonst
                // endgültig verloren, ohne dass es jemand merkt.
                if self.ordner.join("tresor.json").exists() {
                    return Err(self.kaputt("Der Schlüssel des Tresors fehlt"));
                }
                let mut neu = [0u8; 32];
                rand::rng().fill_bytes(&mut neu);
                schreiben_atomar(&pfad, &neu)?;
                Ok(neu)
            }
            Err(fehler) => Err(fehler.into()),
        }
    }

    fn laden(&self) -> anyhow::Result<BTreeMap<String, String>> {
        self.pruefen()?;
        let pfad = self.ordner.join("tresor.json");
        let text = match fs::read_to_string(&pfad) {
            Ok(text) => text,
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeMap::new())
            }
            Err(fehler) => return Err(fehler.into()),
        };
        let huelle: Huelle = match serde_json::from_str(&text) {
            Ok(huelle) => huelle,
            Err(_) => return Err(self.kaputt(format!("{} ist beschädigt", pfad.display()))),
        };
        let nonce = B64
            .decode(&huelle.nonce)
            .ok()
            .filter(|nonce| nonce.len() == 24)
            .ok_or_else(|| self.kaputt(format!("{} ist beschädigt", pfad.display())))?;
        let daten = B64
            .decode(&huelle.daten)
            .map_err(|_| self.kaputt(format!("{} ist beschädigt", pfad.display())))?;
        let schluessel = self.schluessel()?;
        let klar = XChaCha20Poly1305::new(Key::from_slice(&schluessel))
            .decrypt(XNonce::from_slice(&nonce), daten.as_ref())
            .map_err(|_| {
                self.kaputt(format!(
                    "{} lässt sich nicht entschlüsseln (passt tresor.schluessel?)",
                    pfad.display()
                ))
            })?;
        serde_json::from_slice(&klar)
            .map_err(|_| self.kaputt(format!("{} ist beschädigt", pfad.display())))
    }

    fn speichern(&self, werte: &BTreeMap<String, String>) -> anyhow::Result<()> {
        let schluessel = self.schluessel()?;
        let mut nonce = [0u8; 24];
        rand::rng().fill_bytes(&mut nonce);
        let klar = serde_json::to_vec(werte)?;
        let daten = XChaCha20Poly1305::new(Key::from_slice(&schluessel))
            .encrypt(XNonce::from_slice(&nonce), klar.as_ref())
            .map_err(|_| anyhow::anyhow!("Verschlüsseln fehlgeschlagen"))?;
        let huelle = Huelle {
            version: 1,
            nonce: B64.encode(nonce),
            daten: B64.encode(daten),
        };
        schreiben_atomar(
            &self.ordner.join("tresor.json"),
            serde_json::to_string(&huelle)?.as_bytes(),
        )
    }
}

/// Der Name, unter dem ein kaputter Tresor liegen bleibt:
/// `tresor.json.kaputt-20260102T030405Z`.
fn kaputt_pfad(pfad: &Path) -> PathBuf {
    let mut name = pfad.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".kaputt-{}", stempel()));
    pfad.with_file_name(name)
}

/// Ein Zeitstempel für Dateinamen, ohne Uhr: `20260102T030405Z`.
fn stempel() -> String {
    let sekunden = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dauer| dauer.as_secs())
        .unwrap_or_default();
    let tage = i64::try_from(sekunden / 86_400).unwrap_or_default();
    let rest = sekunden % 86_400;
    let (jahr, monat, tag) = zivil(tage);
    format!(
        "{jahr:04}{monat:02}{tag:02}T{:02}{:02}{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Tage seit 1970 → Jahr, Monat, Tag (Howard Hinnant).
fn zivil(tage: i64) -> (i64, u32, u32) {
    let z = tage + 719_468;
    let aera = z.div_euclid(146_097);
    let jahreszeit = z.rem_euclid(146_097);
    let jahr_des_zeitalters =
        (jahreszeit - jahreszeit / 1_460 + jahreszeit / 36_524 - jahreszeit / 146_096) / 365;
    let jahr = jahr_des_zeitalters + aera * 400;
    let tag_des_jahres = jahreszeit
        - (365 * jahr_des_zeitalters + jahr_des_zeitalters / 4 - jahr_des_zeitalters / 100);
    let monatsteil = (5 * tag_des_jahres + 2) / 153;
    let tag = u32::try_from(tag_des_jahres - (153 * monatsteil + 2) / 5 + 1).unwrap_or_default();
    let monat = u32::try_from(if monatsteil < 10 {
        monatsteil + 3
    } else {
        monatsteil - 9
    })
    .unwrap_or_default();
    (jahr + i64::from(monat <= 2), monat, tag)
}

/// Erst in eine temporäre Datei (0600), dann umbenennen – nie halb geschrieben.
///
/// Auch der Export der Einstellungen (Aufgabe 15) schreibt so: die Datei
/// gehört von Anfang an nur dem Benutzer.
pub(crate) fn schreiben_atomar(pfad: &Path, inhalt: &[u8]) -> anyhow::Result<()> {
    let mut zwischen = pfad.as_os_str().to_owned();
    zwischen.push(format!(".{}.neu", std::process::id()));
    let zwischen = PathBuf::from(zwischen);
    {
        let mut optionen = OpenOptions::new();
        optionen.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            optionen.mode(0o600);
        }
        let mut datei = optionen.open(&zwischen)?;
        datei.write_all(inhalt)?;
        datei.sync_all()?;
    }
    fs::rename(&zwischen, pfad)?;
    rechte(pfad, 0o600);
    Ok(())
}

#[cfg(unix)]
fn rechte(pfad: &Path, modus: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(pfad, fs::Permissions::from_mode(modus));
}

#[cfg(not(unix))]
fn rechte(_pfad: &Path, _modus: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundlauf_und_kein_klartext_auf_der_platte() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path().join("t"));
        assert_eq!(tresor.lesen("a").unwrap(), None);
        tresor.schreiben("a", "streng-geheim-4711").unwrap();
        tresor.schreiben("b", "zwei").unwrap();
        assert_eq!(
            tresor.lesen("a").unwrap().as_deref(),
            Some("streng-geheim-4711")
        );

        // Ein zweiter Tresor auf demselben Ordner (anderer Prozess) sieht es.
        let zweiter = DateiTresor::im_ordner(ordner.path().join("t"));
        assert_eq!(zweiter.lesen("b").unwrap().as_deref(), Some("zwei"));

        let roh = fs::read_to_string(ordner.path().join("t/tresor.json")).unwrap();
        assert!(!roh.contains("streng-geheim"), "Klartext in der Datei");
        assert!(!roh.contains("\"a\""), "Schlüsselnamen im Klartext");

        tresor.loeschen("a").unwrap();
        assert_eq!(zweiter.lesen("a").unwrap(), None);
        tresor.loeschen("gibt-es-nicht").unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn nur_der_benutzer_darf_lesen() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path().join("t"));
        tresor.schreiben("a", "x").unwrap();
        for (datei, soll) in [
            ("", 0o700),
            ("tresor.json", 0o600),
            ("tresor.schluessel", 0o600),
        ] {
            let modus = fs::metadata(ordner.path().join("t").join(datei))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(modus, soll, "{datei}");
        }
    }

    #[test]
    fn ein_falscher_schluessel_wird_gemeldet_ohne_inhalt() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("a", "streng-geheim-4711").unwrap();
        fs::write(ordner.path().join("tresor.schluessel"), [7u8; 32]).unwrap();
        let fehler = tresor.lesen("a").unwrap_err().to_string();
        assert!(fehler.contains("entschlüsseln"), "{fehler}");
        assert!(!fehler.contains("streng-geheim"));
    }

    #[test]
    fn gleichzeitige_schreiber_verlieren_nichts() {
        let ordner = tempfile::tempdir().unwrap();
        let pfad = ordner.path().to_path_buf();
        let faeden: Vec<_> = (0..8)
            .map(|nummer| {
                let pfad = pfad.clone();
                std::thread::spawn(move || {
                    let tresor = DateiTresor::im_ordner(pfad);
                    for runde in 0..10 {
                        tresor.schreiben(&format!("{nummer}-{runde}"), "x").unwrap();
                    }
                })
            })
            .collect();
        for faden in faeden {
            faden.join().unwrap();
        }
        let tresor = DateiTresor::im_ordner(pfad);
        for nummer in 0..8 {
            for runde in 0..10 {
                assert!(tresor
                    .lesen(&format!("{nummer}-{runde}"))
                    .unwrap()
                    .is_some());
            }
        }
    }

    // ---------------------------------------------------------- Paket 3

    /// Fehlt der Schlüssel, obwohl ein Tresor da ist, wird **kein** neuer
    /// erzeugt: die Werte wären sonst endgültig verloren. Die alten Dateien
    /// bleiben als `.kaputt-<zeit>` liegen.
    #[test]
    fn ein_fehler_der_schluessel_erzeugt_keinen_neuen() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("a", "streng-geheim-4711").unwrap();
        fs::remove_file(ordner.path().join("tresor.schluessel")).unwrap();

        let fehler = tresor.lesen("a").unwrap_err().to_string();
        assert!(fehler.contains("Schlüssel"), "{fehler}");
        assert!(
            !ordner.path().join("tresor.schluessel").exists(),
            "kein neuer Schlüssel"
        );
        // Nichts verschoben: den Schlüssel zurückzuspielen genügt.
        assert!(ordner.path().join("tresor.json").exists());
        assert!(tresor.schreiben("b", "x").is_err(), "nichts überschrieben");
    }

    /// Auch ein **neuer** Prozess beginnt nicht still von vorn, und mit dem
    /// zurückgespielten Schlüssel ist alles wieder da.
    #[test]
    fn ein_neustart_beginnt_nicht_von_vorn_und_der_schluessel_hilft() {
        let ordner = tempfile::tempdir().unwrap();
        let schluessel = ordner.path().join("tresor.schluessel");
        DateiTresor::im_ordner(ordner.path())
            .schreiben("a", "streng-geheim-4711")
            .unwrap();
        let gesichert = fs::read(&schluessel).unwrap();
        fs::remove_file(&schluessel).unwrap();
        assert!(DateiTresor::im_ordner(ordner.path()).lesen("a").is_err());
        let neu = DateiTresor::im_ordner(ordner.path());
        assert!(neu.lesen("a").is_err());
        assert!(neu.schreiben("b", "x").is_err());
        fs::write(&schluessel, gesichert).unwrap();
        assert_eq!(
            DateiTresor::im_ordner(ordner.path())
                .lesen("a")
                .unwrap()
                .as_deref(),
            Some("streng-geheim-4711")
        );
    }

    /// „Neu beginnen“ ohne Fehler fasst nichts an.
    #[test]
    fn neu_beginnen_ohne_fehler_laesst_alles_stehen() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("a", "x").unwrap();
        tresor.neu_beginnen().unwrap();
        assert_eq!(tresor.lesen("a").unwrap().as_deref(), Some("x"));
    }

    /// Ein kaputter Tresor verhindert den Start nicht: der Fehler steht im
    /// Zustand, nichts wird überschrieben, und erst „Neu beginnen“ schreibt
    /// wieder.
    #[test]
    fn ein_kaputter_tresor_meldet_sich_und_wartet_auf_neu_beginnen() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("a", "streng-geheim-4711").unwrap();
        fs::write(ordner.path().join("tresor.json"), "kein json").unwrap();

        assert!(tresor.lesen("a").is_err(), "nicht lesbar");
        let fehler = tresor.fehler().expect("der Zustand kennt den Fehler");
        assert!(fehler.contains("beschädigt"), "{fehler}");
        assert!(
            tresor.schreiben("b", "x").is_err(),
            "ohne „Neu beginnen“ wird nichts geschrieben"
        );
        assert_eq!(
            fs::read_to_string(ordner.path().join("tresor.json")).unwrap(),
            "kein json",
            "vor „Neu beginnen“ bleibt alles, wie es ist"
        );

        // „Neu beginnen“ räumt den Zustand weg; danach geht es weiter.
        tresor.neu_beginnen().unwrap();
        assert_eq!(tresor.fehler(), None);
        // Die kaputte Datei liegt vollständig daneben.
        let kaputt: Vec<_> = fs::read_dir(ordner.path())
            .unwrap()
            .flatten()
            .map(|eintrag| eintrag.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("tresor.json.kaputt-"))
            .collect();
        assert_eq!(kaputt.len(), 1, "{kaputt:?}");
        assert_eq!(
            fs::read_to_string(ordner.path().join(&kaputt[0])).unwrap(),
            "kein json",
            "nichts wird überschrieben"
        );
        tresor.schreiben("c", "neu").unwrap();
        assert_eq!(tresor.lesen("c").unwrap().as_deref(), Some("neu"));
        assert_eq!(tresor.lesen("a").unwrap(), None, "der alte Wert ist weg");
    }

    /// Ein Abgleich liest den Tresor **einmal** – `werte_laden` liefert alle
    /// gesuchten Schlüssel in einem Zugriff.
    #[test]
    fn viele_werte_kommen_in_einem_zugriff() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("wert/a/rot", "eins").unwrap();
        tresor.schreiben("wert/a/blau", "zwei").unwrap();
        let gesucht = vec![
            "wert/a/rot".to_string(),
            "wert/a/blau".to_string(),
            "wert/a/gruen".to_string(),
        ];
        let werte = tresor.werte_laden(&gesucht).unwrap();
        assert_eq!(werte.len(), 2);
        assert_eq!(werte["wert/a/rot"], "eins");
        assert_eq!(werte["wert/a/blau"], "zwei");
    }

    /// Die Sperre hat eine Frist: hängt ein anderer Schreiber, gibt der Aufruf
    /// auf, statt den Kern zu blockieren.
    #[test]
    fn die_sperre_hat_eine_frist() {
        let ordner = tempfile::tempdir().unwrap();
        let tresor = DateiTresor::im_ordner(ordner.path());
        tresor.schreiben("a", "x").unwrap();
        // Ein anderer Prozess (hier ein Faden mit eigenem Deskriptor) hält die
        // Sperre.
        let pfad = ordner.path().join("tresor.sperre");
        let halter = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&pfad)
            .unwrap();
        halter.lock().unwrap();
        let beginn = std::time::Instant::now();
        let fehler = tresor.lesen("a").unwrap_err().to_string();
        assert!(fehler.contains("gesperrt"), "{fehler}");
        assert!(
            beginn.elapsed() >= SPERRE_FRIST,
            "es wurde gewartet, aber nicht ewig: {:?}",
            beginn.elapsed()
        );
        halter.unlock().unwrap();
        assert_eq!(tresor.lesen("a").unwrap().as_deref(), Some("x"));
    }
}
