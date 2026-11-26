// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Programme, die der Client vom Server geladen hat (Aufgabe 37).
//!
//! Ablage im Profil: `tools/<name>/<sha256>/<programm>`. Der Ordner ist `0700`,
//! die Datei `0700` (Windows: nur der Benutzer, über die Rechte des Profils).
//! Ein Programm wird **nie** an einen Pfad vom Server geschrieben: es zählt
//! allein der Name nach [`quiver_domain::ist_verteilt`], und der Ordner darunter
//! ist die Prüfsumme.
//!
//! Geladen wird in eine Zwischendatei im Zielordner: Größe und SHA-256 werden
//! beim Schreiben mitgezählt, danach `fsync`, dann umbenennen – erst dann wird
//! die Datei ausführbar. Ein abgebrochener Ladevorgang hinterlässt so nie ein
//! halbes Programm unter dem richtigen Namen.
//!
//! Vor **jedem** Start wird die Datei erneut geprüft (dieselbe Datei, kein
//! Verweis, Größe und Prüfsumme): stimmt etwas nicht, wird sie neu geladen.
//! Alte Fassungen fallen nach dem ersten erfolgreichen Start weg.
//!
//! `QUIVER_TOOLS_DIR` gilt hier **nicht**: dieser Schalter zeigt auf einen
//! Entwicklungsordner mit flach liegenden Programmen ([`crate::tools`]). Die
//! Ablage im Profil bleibt die Ablage im Profil.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use quiver_domain::{Laufzeit, Programm};
use quiver_protocol::PRAEFIX;
use sha2::{Digest, Sha256};

/// So lange darf ein Programm-Laden höchstens dauern (große Dateien, langsame
/// Leitung).
pub const LADEN_FRIST: Duration = Duration::from_secs(15 * 60);

/// Warum eine Datei nicht taugt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruefFehler {
    /// Es gibt sie nicht (mehr).
    Fehlt,
    /// Sie ist ein Verweis oder keine gewöhnliche Datei.
    KeinProgramm,
    /// Sie ist nicht so groß, wie das Soll sagt.
    Groesse { ist: u64, soll: u64 },
    /// Die Prüfsumme stimmt nicht.
    Pruefsumme,
}

impl std::fmt::Display for PruefFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PruefFehler::Fehlt => f.write_str("das Programm fehlt"),
            PruefFehler::KeinProgramm => f.write_str("das ist kein Programm"),
            PruefFehler::Groesse { ist, soll } => {
                write!(f, "das Programm ist {ist} statt {soll} Bytes groß")
            }
            PruefFehler::Pruefsumme => f.write_str("die Prüfsumme des Programms stimmt nicht"),
        }
    }
}

/// Wie ein Eintrag zu seinem Programm kommt (Aufgabe 37).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aufloesung {
    /// Kein verteiltes Programm: der Eintrag startet ein fremdes Programm
    /// (`npx`, `uvx`, `docker`, einen festen Pfad).
    Fremd,
    /// Das Programm liegt geprüft bereit und wird von hier gestartet.
    Pfad(PathBuf),
    /// Der Eintrag erwartet ein Programm vom Server, für dieses System gibt es
    /// das aber nicht.
    NichtFuerSystem(String),
    /// Das Programm gehört hierher, die Datei fehlt oder ist beschädigt.
    Beschaedigt(String),
}

/// Den Befehl eines Eintrags zu einem startbaren Programm machen.
///
/// Der einzige Weg, auf dem ein verteiltes Programm zu einem Pfad wird. Ein
/// Programmname vom Server wird nie als Pfad benutzt: der Pfad entsteht aus
/// [`Ablage::pfad`], und die Datei muss die Prüfsumme aus dem Soll tragen.
///
/// Führt der Server das Programm nicht, bleibt ein **vorhandener** fester
/// Pfad: das ist dann das eigene Programm des Betreibers, das wie bisher läuft
/// (Aufgabe 37).
pub fn aufloesen(ablage: &Ablage, programme: &[Programm], laufzeit: &Laufzeit) -> Aufloesung {
    let Some(name) = crate::tools::verteilt_name(laufzeit) else {
        return Aufloesung::Fremd;
    };
    // Entwicklung: ein ausgechecktes Programm geht vor (QUIVER_TOOLS_DIR).
    if let Some(pfad) = crate::tools::im_entwicklungsordner(&name) {
        return Aufloesung::Pfad(pfad);
    }
    let Some(programm) = programme.iter().find(|programm| programm.name == name) else {
        // Der Server führt dieses Programm nicht. Steht im Eintrag ein Pfad,
        // den es auf diesem Gerät gibt, ist das das eigene Programm des
        // Betreibers – es läuft, wie es dasteht.
        if let Some(pfad) = eigener_pfad(laufzeit) {
            return Aufloesung::Pfad(pfad);
        }
        return Aufloesung::NichtFuerSystem(name);
    };
    match ablage.pruefen(programm) {
        Ok(pfad) => Aufloesung::Pfad(pfad),
        Err(_) => Aufloesung::Beschaedigt(name),
    }
}

/// Ein fester Pfad aus dem Eintrag, wenn er auf diesem Gerät eine Datei ist.
///
/// Führt der Server das Programm nicht, läuft dieser Pfad wie bisher – das ist
/// das eigene Programm des Betreibers (Aufgabe 37).
pub(crate) fn eigener_pfad(laufzeit: &Laufzeit) -> Option<PathBuf> {
    let Laufzeit::Stdio { befehl, .. } = laufzeit else {
        return None;
    };
    let befehl = befehl.trim();
    if !crate::tools::ist_pfad(befehl) {
        return None;
    }
    let pfad = Path::new(befehl);
    pfad.is_file().then(|| pfad.to_path_buf())
}

/// Eine Laufzeit mit einem festen Programmpfad (Argumente und Umgebung
/// bleiben, wie sie sind).
pub fn mit_pfad(laufzeit: &Laufzeit, pfad: &Path) -> Laufzeit {
    match laufzeit {
        Laufzeit::Stdio {
            argumente,
            umgebung,
            ..
        } => Laufzeit::Stdio {
            befehl: pfad.to_string_lossy().to_string(),
            argumente: argumente.clone(),
            umgebung: umgebung.clone(),
        },
        anderes => anderes.clone(),
    }
}

/// Warum ein Eintrag mit seinem Programm nicht starten kann (Aufgabe 37).
#[derive(Debug, thiserror::Error)]
pub enum StartFehler {
    /// Der Server verteilt das Programm nicht für dieses System. Kein Fehler,
    /// eine Auskunft – ein neuer Versuch ändert nichts.
    #[error("Für dieses System gibt es das Tool nicht.")]
    NichtFuerSystem,
    /// Das Programm fehlt oder ließ sich nicht laden. Ein neuer Versuch kann
    /// helfen (der Server ist vielleicht gerade weg).
    #[error("Das Programm »{0}« lässt sich nicht laden")]
    NichtLadbar(String),
}

/// So groß darf ein Programm höchstens sein (dieselbe Grenze wie im Server).
const PROGRAMM_BYTES_MAX: u64 = 128 * 1024 * 1024;

/// So lange wird nach dem Stoppen auf das Ende des Prozesses gewartet, bevor
/// ein Programm gelöscht wird (Aufgabe 37).
///
/// Unter Windows hält ein laufender Prozess seine Datei fest: ein Löschen
/// daneben scheitert **still**, und beim nächsten Start läuft die alte Fassung
/// weiter. Deshalb wird es bis zu dieser Frist erneut versucht.
pub const LOESCHEN_FRIST: Duration = Duration::from_secs(10);
/// Der Abstand zwischen zwei Löschversuchen.
const LOESCHEN_TAKT: Duration = Duration::from_millis(50);

/// Taugt die Angabe des Servers? Name und Prüfsumme werden Teile eines Pfades,
/// deshalb wird beides geprüft, **bevor** ein Ordner angelegt oder angefasst
/// wird: der Name wie in [`quiver_domain::ist_verteilt`], die Prüfsumme genau
/// 64 kleine Hex-Zeichen.
fn angabe_gueltig(programm: &Programm) -> bool {
    quiver_domain::ist_verteilt(&programm.name)
        && programm.sha256.len() == 64
        && programm
            .sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && programm.groesse <= PROGRAMM_BYTES_MAX
}

/// Die Ablage der verteilten Programme im Profil.
#[derive(Debug, Clone)]
pub struct Ablage {
    wurzel: PathBuf,
}

impl Ablage {
    /// Die Ablage eines Profils: `<profil>/tools`.
    pub fn neu(profil: &Path) -> Self {
        Self {
            wurzel: profil.join("tools"),
        }
    }

    /// Die Wurzel (für Log und Tests).
    pub fn wurzel(&self) -> &Path {
        &self.wurzel
    }

    /// Der Ordner eines Programms (alle Fassungen): `tools/<name>`.
    fn ordner(&self, name: &str) -> PathBuf {
        self.wurzel.join(name)
    }

    /// Der Pfad einer bestimmten Fassung: `tools/<name>/<sha256>/<programm>`.
    ///
    /// Der Programmname ist der Name des Eintrags, unter Windows mit `.exe` –
    /// nie etwas, das der Server schickt.
    pub fn pfad(&self, programm: &Programm) -> PathBuf {
        self.ordner(&programm.name)
            .join(&programm.sha256)
            .join(format!("{}{}", programm.name, std::env::consts::EXE_SUFFIX))
    }

    /// Prüfen, ob diese Fassung da und unversehrt ist.
    pub fn pruefen(&self, programm: &Programm) -> Result<PathBuf, PruefFehler> {
        if !angabe_gueltig(programm) {
            return Err(PruefFehler::KeinProgramm);
        }
        let pfad = self.pfad(programm);
        let meta = std::fs::symlink_metadata(&pfad).map_err(|_| PruefFehler::Fehlt)?;
        // Kein Verweis: was nach draußen zeigt, ist kein Programm dieser Ablage.
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(PruefFehler::KeinProgramm);
        }
        if meta.len() != programm.groesse {
            return Err(PruefFehler::Groesse {
                ist: meta.len(),
                soll: programm.groesse,
            });
        }
        let sha256 = datei_sha256(&pfad).map_err(|_| PruefFehler::Fehlt)?;
        if sha256 != programm.sha256 {
            return Err(PruefFehler::Pruefsumme);
        }
        Ok(pfad)
    }

    /// Gibt es diese Fassung, und stimmt sie? (`None` heißt: nein)
    pub fn vorhanden(&self, programm: &Programm) -> Option<PathBuf> {
        self.pruefen(programm).ok()
    }

    /// Ein Programm vom Server laden, prüfen und ablegen.
    ///
    /// Danach steht es unter [`Ablage::pfad`] und ist ausführbar. Was schon
    /// richtig daliegt, wird nicht erneut geladen.
    pub async fn holen(
        &self,
        client: &reqwest::Client,
        server: &str,
        token: &str,
        programm: &Programm,
    ) -> anyhow::Result<PathBuf> {
        self.holen_mit(client, server, token, programm, |_| {})
            .await
    }

    /// Wie [`Ablage::holen`], meldet aber nach jedem Stück die geladene Menge
    /// (`fortschritt`), damit die Karte den Anteil zeigen kann.
    pub async fn holen_mit(
        &self,
        client: &reqwest::Client,
        server: &str,
        token: &str,
        programm: &Programm,
        mut fortschritt: impl FnMut(u64),
    ) -> anyhow::Result<PathBuf> {
        if !angabe_gueltig(programm) {
            anyhow::bail!("Der Server nennt ein ungültiges Programm");
        }
        if let Ok(pfad) = self.pruefen(programm) {
            return Ok(pfad);
        }
        let ziel = self.pfad(programm);
        let ordner = ziel.parent().expect("die Ablage hat einen Ordner");
        std::fs::create_dir_all(self.ordner(&programm.name))?;
        std::fs::create_dir_all(ordner)?;
        rechte(&self.wurzel, 0o700)?;
        rechte(&self.ordner(&programm.name), 0o700)?;
        rechte(ordner, 0o700)?;

        // Die Anfrage: nur Name und System – kein Pfad, keine Weiterleitung.
        let antwort = client
            .get(format!("{server}{PRAEFIX}/geraet/tools/{}", programm.name))
            .bearer_auth(token)
            .query(&[
                ("system", programm.system.as_str()),
                ("arch", programm.arch.as_str()),
            ])
            .timeout(LADEN_FRIST)
            .send()
            .await
            .map_err(|fehler| anyhow::anyhow!("Programm nicht abholbar: {fehler}"))?;
        // Ein Entzug während des Ladens ist ein 403 und damit derselbe Fehler
        // wie beim Soll: der Aufrufer behandelt ihn gleich.
        let antwort = crate::abgleich::pruefen(antwort).await?;
        if let Some(laenge) = antwort.content_length() {
            if laenge != programm.groesse {
                anyhow::bail!(
                    "Der Server schickt {laenge} Bytes, angekündigt waren {}",
                    programm.groesse
                );
            }
        }

        // Zwischendatei im Zielordner: dasselbe Dateisystem, ein Umbenennen
        // ohne Kopie.
        let zwischen = ordner.join(format!(
            ".laden-{}-{}-{}",
            programm.sha256,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|dauer| dauer.as_nanos())
                .unwrap_or(0)
        ));
        let mut datei = {
            let mut bauer = tokio::fs::OpenOptions::new();
            bauer.create_new(true).write(true);
            #[cfg(unix)]
            bauer.mode(0o600);
            bauer.open(&zwischen).await?
        };
        let ergebnis = schreiben(&mut datei, antwort, programm, &mut fortschritt).await;
        drop(datei);
        if let Err(fehler) = ergebnis {
            let _ = tokio::fs::remove_file(&zwischen).await;
            return Err(fehler);
        }

        // Umbenennen: von hier an ist es das Programm. Erst jetzt die Rechte.
        if let Ok(meta) = tokio::fs::symlink_metadata(&ziel).await {
            if meta.file_type().is_symlink() || meta.is_file() {
                // Eine kaputte Fassung, die dieselbe Prüfsumme trägt, gibt es
                // nicht – aber ein Verweis darf nicht im Weg stehen.
                let _ = tokio::fs::remove_file(&ziel).await;
            }
        }
        tokio::fs::rename(&zwischen, &ziel)
            .await
            .map_err(|fehler| anyhow::anyhow!("Programm nicht ablegbar: {fehler}"))?;
        rechte(&ziel, 0o700)?;
        ordner_sync(ordner);
        Ok(ziel)
    }

    /// Ein Programm löschen und dabei auf das Ende des Prozesses warten.
    ///
    /// `stoppe_in` bricht nur ab; der Prozess lebt noch einen Augenblick (und
    /// unter Windows hält er seine Datei so lange fest). Deshalb wird bis
    /// [`LOESCHEN_FRIST`] erneut versucht. `false` heißt: es liegt noch da –
    /// der nächste Abgleich und der nächste Start versuchen es wieder.
    pub async fn loeschen_warten(&self, name: &str) -> bool {
        if !quiver_domain::ist_verteilt(name) {
            return true;
        }
        self.ordner_warten(&self.ordner(name), name).await
    }

    /// Einen Ordner löschen, bis er weg ist (oder die Frist um ist).
    async fn ordner_warten(&self, ordner: &Path, name: &str) -> bool {
        let ende = std::time::Instant::now() + LOESCHEN_FRIST;
        loop {
            match std::fs::remove_dir_all(ordner) {
                Ok(()) => {
                    tracing::info!("Programm {name} gelöscht");
                    return true;
                }
                Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => return true,
                Err(fehler) => {
                    if std::time::Instant::now() >= ende {
                        tracing::warn!(
                            "Programm {name} ließ sich nicht löschen ({fehler}); \
                             der nächste Abgleich versucht es erneut"
                        );
                        return false;
                    }
                    tokio::time::sleep(LOESCHEN_TAKT).await;
                }
            }
        }
    }

    /// Alte Fassungen eines Programms löschen – nach dem ersten erfolgreichen
    /// Start (Aufgabe 37).
    /// `false` heißt: eine alte Fassung ließ sich (noch) nicht löschen.
    pub fn alte_fassungen(&self, name: &str, behalten: &str) -> bool {
        let ordner = self.ordner(name);
        let Ok(eintraege) = std::fs::read_dir(&ordner) else {
            return true;
        };
        let mut alles = true;
        for eintrag in eintraege.flatten() {
            let pfad = eintrag.path();
            if !eintrag.file_type().map(|art| art.is_dir()).unwrap_or(false) {
                continue;
            }
            if eintrag.file_name().to_string_lossy() == behalten {
                continue;
            }
            if std::fs::remove_dir_all(&pfad).is_ok() {
                tracing::info!("alte Fassung von {name} gelöscht");
            } else {
                alles = false;
            }
        }
        alles
    }

    /// Alle Programme löschen, die nicht mehr im Soll stehen (Aufgabe 37).
    ///
    /// Die Konfiguration bleibt: was der User eingetragen hat, hängt an der
    /// Kennung des Eintrags und nicht an der Datei. Es werden nur Ordner
    /// angefasst, die wie verteilte Programme heißen – nichts anderes.
    ///
    /// Je Programm wird gewartet, bis der Prozess seine Datei freigibt; was
    /// nach [`LOESCHEN_FRIST`] noch da ist, kommt beim nächsten Abgleich und
    /// beim nächsten Start wieder dran.
    pub async fn aufraeumen(&self, behalten: &BTreeSet<String>) -> usize {
        let Ok(eintraege) = std::fs::read_dir(&self.wurzel) else {
            return 0;
        };
        let mut offen = 0;
        for eintrag in eintraege.flatten() {
            let name = eintrag.file_name().to_string_lossy().to_string();
            if !quiver_domain::ist_verteilt(&name) || behalten.contains(&name) {
                continue;
            }
            if !self.loeschen_warten(&name).await {
                offen += 1;
            }
        }
        offen
    }

    /// Ein Programm löschen (alle Fassungen) – wartet auf das Prozessende.
    pub async fn loeschen(&self, name: &str) {
        self.loeschen_warten(name).await;
    }

    /// Alles löschen (Abmelden am Gerät, Aufgabe 37). Die Konfiguration bleibt.
    pub async fn alles_loeschen(&self) {
        self.aufraeumen(&BTreeSet::new()).await;
    }
}

/// Den Rumpf in die Zwischendatei schreiben, dabei Größe und Prüfsumme prüfen.
async fn schreiben(
    datei: &mut tokio::fs::File,
    antwort: reqwest::Response,
    programm: &Programm,
    fortschritt: &mut impl FnMut(u64),
) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut h = Sha256::new();
    let mut gesamt: u64 = 0;
    let mut strom = antwort.bytes_stream();
    while let Some(stueck) = strom.next().await {
        let stueck = stueck.map_err(|fehler| anyhow::anyhow!("Laden abgebrochen: {fehler}"))?;
        gesamt = gesamt.saturating_add(stueck.len() as u64);
        if gesamt > programm.groesse {
            anyhow::bail!(
                "Der Server schickt mehr als angekündigt ({} Bytes)",
                programm.groesse
            );
        }
        h.update(&stueck);
        datei.write_all(&stueck).await?;
        fortschritt(gesamt);
    }
    if gesamt != programm.groesse {
        anyhow::bail!(
            "Das Programm kam unvollständig ({gesamt} von {} Bytes)",
            programm.groesse
        );
    }
    let sha256 = hex::encode(h.finalize());
    if sha256 != programm.sha256 {
        anyhow::bail!("Die Prüfsumme des geladenen Programms stimmt nicht");
    }
    datei.flush().await?;
    datei.sync_all().await?;
    Ok(())
}

/// Die Prüfsumme einer Datei.
fn datei_sha256(pfad: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut datei = std::fs::File::open(pfad)?;
    let mut h = Sha256::new();
    let mut puffer = vec![0u8; 64 * 1024];
    loop {
        let gelesen = datei.read(&mut puffer)?;
        if gelesen == 0 {
            break;
        }
        h.update(&puffer[..gelesen]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Rechte setzen (Windows: der Profilordner gehört schon dem Benutzer).
fn rechte(pfad: &Path, modus: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(pfad, std::fs::Permissions::from_mode(modus))?;
    }
    #[cfg(not(unix))]
    let _ = (pfad, modus);
    Ok(())
}

/// Den Ordner nach dem Umbenennen synchronisieren (Linux/macOS; Windows kann
/// einen Ordner nicht als Datei öffnen – dort ist es nicht nötig).
fn ordner_sync(ordner: &Path) {
    #[cfg(unix)]
    if let Ok(datei) = std::fs::File::open(ordner) {
        let _ = datei.sync_all();
    }
    #[cfg(not(unix))]
    let _ = ordner;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baum(name: &str) -> PathBuf {
        // Prozessnummer **und** Zähler: eindeutig je Test, und ohne Zeichen,
        // die unter Windows in einem Pfad nicht vorkommen dürfen.
        static ZAEHLER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let ordner = std::env::temp_dir().join(format!(
            "quiver-ablage-{}-{nummer}-{name}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&ordner).ok();
        std::fs::create_dir_all(&ordner).expect("Testordner");
        ordner
    }

    fn programm(name: &str, inhalt: &[u8]) -> Programm {
        Programm {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            groesse: inhalt.len() as u64,
            sha256: hex::encode(Sha256::digest(inhalt)),
            system: "linux".to_string(),
            arch: "x64".to_string(),
        }
    }

    /// Ein winziger Server, der ein Programm ausliefert (oder einen Fehler).
    async fn server(koerper: Vec<u8>, status: axum::http::StatusCode) -> String {
        use axum::routing::get;
        let router = axum::Router::new().route(
            "/api/v1/geraet/tools/{name}",
            get({
                let koerper = koerper.clone();
                move || {
                    let koerper = koerper.clone();
                    async move {
                        (
                            status,
                            [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
                            koerper,
                        )
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Port");
        let adresse = listener.local_addr().expect("Adresse");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        format!("http://{adresse}")
    }

    #[tokio::test]
    async fn ein_programm_wird_geladen_geprueft_und_abgelegt() {
        let wurzel = baum("laden");
        let ablage = Ablage::neu(&wurzel);
        let inhalt = b"#!/bin/sh\necho hallo\n".to_vec();
        let programm = programm("quiver-devops", &inhalt);
        let basis = server(inhalt.clone(), axum::http::StatusCode::OK).await;

        let pfad = ablage
            .holen(&crate::netz::client(), &basis, "token", &programm)
            .await
            .expect("geladen");
        assert!(pfad.starts_with(wurzel.join("tools").join("quiver-devops")));
        assert_eq!(std::fs::read(&pfad).expect("Datei"), inhalt);
        assert!(pfad.to_string_lossy().contains(&programm.sha256));
        // Zweimal laden lädt nicht neu, sondern nimmt, was schon stimmt.
        let nochmal = ablage
            .holen(&crate::netz::client(), &basis, "token", &programm)
            .await
            .expect("schon da");
        assert_eq!(nochmal, pfad);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let rechte = std::fs::metadata(&pfad).expect("Rechte").permissions();
            assert_eq!(
                rechte.mode() & 0o777,
                0o700,
                "ausführbar nur für den Benutzer"
            );
        }
    }

    #[tokio::test]
    async fn eine_falsche_pruefsumme_wird_nicht_abgelegt() {
        let wurzel = baum("pruefsumme");
        let ablage = Ablage::neu(&wurzel);
        // Gleiche Länge, anderer Inhalt: nur die Prüfsumme kann das merken.
        let inhalt = b"richtig".to_vec();
        let programm = programm("quiver-devops", &inhalt);
        let basis = server(b"falsch!".to_vec(), axum::http::StatusCode::OK).await;
        let fehler = ablage
            .holen(&crate::netz::client(), &basis, "token", &programm)
            .await
            .expect_err("muss scheitern");
        let text = format!("{fehler:#}");
        assert!(text.contains("Prüfsumme"), "{text}");
        assert!(ablage.vorhanden(&programm).is_none());
        // Keine halbe Datei unter dem richtigen Namen.
        let ordner = ablage.pfad(&programm);
        assert!(!ordner.exists(), "{} liegt da", ordner.display());
    }

    #[tokio::test]
    async fn eine_zu_kurze_antwort_wird_erkannt() {
        let wurzel = baum("kurz");
        let ablage = Ablage::neu(&wurzel);
        let inhalt = b"0123456789".to_vec();
        let programm = programm("quiver-devops", &inhalt);
        let basis = server(b"012".to_vec(), axum::http::StatusCode::OK).await;
        let fehler = ablage
            .holen(&crate::netz::client(), &basis, "token", &programm)
            .await
            .expect_err("muss scheitern");
        let text = format!("{fehler:#}");
        assert!(
            text.contains("unvollständig") || text.contains("schickt"),
            "{text}"
        );
        assert!(ablage.vorhanden(&programm).is_none());
    }

    /// Name und Prüfsumme kommen vom Server und werden Teile eines Pfades: was
    /// nicht genau passt, legt weder einen Ordner an noch wird etwas abgerufen.
    #[tokio::test]
    async fn eine_ungueltige_angabe_legt_keinen_ordner_an() {
        let profil = baum("ungueltig");
        let ablage = Ablage::neu(&profil);
        let gut = programm("quiver-devops", b"hallo");
        let aussen = baum("aussen");
        let mut faelle = Vec::new();
        for sha256 in [
            "../../../aussen".to_string(),
            aussen.to_string_lossy().to_string(),
            "A".repeat(64),
            "a".repeat(63),
            String::new(),
        ] {
            faelle.push(Programm {
                sha256,
                ..gut.clone()
            });
        }
        for name in [
            "../quiver-devops",
            " quiver-devops",
            "quiver-devops ",
            "devops",
        ] {
            faelle.push(Programm {
                name: name.to_string(),
                ..gut.clone()
            });
        }
        faelle.push(Programm {
            groesse: PROGRAMM_BYTES_MAX + 1,
            ..gut.clone()
        });
        for fall in faelle {
            assert_eq!(ablage.pruefen(&fall), Err(PruefFehler::KeinProgramm));
            let fehler = ablage
                .holen(&reqwest::Client::new(), "http://127.0.0.1:9", "t", &fall)
                .await
                .expect_err("abgelehnt");
            assert!(
                format!("{fehler}").contains("ungültiges Programm"),
                "{fehler}"
            );
        }
        assert!(!ablage.wurzel().exists());
        assert_eq!(std::fs::read_dir(&aussen).unwrap().count(), 0);
    }

    #[test]
    fn eine_manipulierte_datei_faellt_bei_der_pruefung_durch() {
        let wurzel = baum("manipuliert");
        let ablage = Ablage::neu(&wurzel);
        let inhalt = b"original".to_vec();
        let programm = programm("quiver-devops", &inhalt);
        let pfad = ablage.pfad(&programm);
        std::fs::create_dir_all(pfad.parent().expect("Ordner")).expect("Ordner");
        std::fs::write(&pfad, b"geaendert").expect("Datei");
        assert!(matches!(
            ablage.pruefen(&programm),
            Err(PruefFehler::Groesse { .. }) | Err(PruefFehler::Pruefsumme)
        ));
        // Gleiche Länge, anderer Inhalt: die Prüfsumme fällt auf.
        std::fs::write(&pfad, b"geaender").expect("Datei");
        assert_eq!(ablage.pruefen(&programm), Err(PruefFehler::Pruefsumme));
    }

    #[cfg(unix)]
    #[test]
    fn ein_verweis_gilt_nicht_als_programm() {
        let wurzel = baum("verweis");
        let ablage = Ablage::neu(&wurzel);
        let inhalt = b"original".to_vec();
        let programm = programm("quiver-devops", &inhalt);
        let pfad = ablage.pfad(&programm);
        std::fs::create_dir_all(pfad.parent().expect("Ordner")).expect("Ordner");
        std::fs::write(&pfad, &inhalt).expect("Datei");
        let ziel = wurzel.join("woanders");
        std::fs::rename(&pfad, &ziel).expect("verschieben");
        std::os::unix::fs::symlink(&ziel, &pfad).expect("Verweis");
        assert_eq!(ablage.pruefen(&programm), Err(PruefFehler::KeinProgramm));
    }

    #[tokio::test]
    async fn alte_fassungen_und_nicht_zugewiesene_programme_werden_geloescht() {
        let wurzel = baum("aufraeumen");
        let ablage = Ablage::neu(&wurzel);
        let alt = programm("quiver-devops", b"alt");
        let neu = programm("quiver-m365", b"neu");
        for programm in [&alt, &neu] {
            let pfad = ablage.pfad(programm);
            std::fs::create_dir_all(pfad.parent().expect("Ordner")).expect("Ordner");
            std::fs::write(&pfad, b"x").expect("Datei");
        }
        // Der Eintrag bleibt, aber in einer anderen Fassung.
        let andere = Programm {
            sha256: "f".repeat(64),
            ..alt.clone()
        };
        let pfad = ablage.pfad(&andere);
        std::fs::create_dir_all(pfad.parent().expect("Ordner")).expect("Ordner");
        std::fs::write(&pfad, b"x").expect("Datei");

        ablage.alte_fassungen("quiver-devops", &alt.sha256);
        assert!(ablage.pfad(&alt).exists(), "die geltende Kennung bleibt");
        assert!(!ablage.pfad(&andere).exists(), "die alte Fassung ist weg");

        // Nur noch quiver-devops ist zugewiesen: quiver-m365 verschwindet.
        ablage
            .aufraeumen(&BTreeSet::from(["quiver-devops".to_string()]))
            .await;
        assert!(ablage.ordner("quiver-devops").is_dir());
        assert!(!ablage.ordner("quiver-m365").exists());

        // Abmelden: alles weg, nichts anderes im Ordner wird angefasst.
        let fremd = wurzel.join("tools").join("meine-notizen");
        std::fs::create_dir_all(&fremd).expect("Ordner");
        ablage.alles_loeschen().await;
        assert!(!ablage.ordner("quiver-devops").exists());
        assert!(fremd.exists(), "fremde Ordner bleiben");
    }

    /// Nach dem Stoppen ist der Prozess nicht sofort tot: unter Windows hält er
    /// seine Datei fest. Das Löschen wartet deshalb und versucht es erneut –
    /// statt still zu scheitern und die alte Fassung liegen zu lassen.
    ///
    /// Nachgestellt mit einem Ordner, der für einen Augenblick nicht löschbar
    /// ist (`0o500`): ein einzelner Versuch scheiterte, das Warten kommt durch.
    #[cfg(unix)]
    #[tokio::test]
    async fn das_loeschen_versucht_es_bis_zum_ende_des_prozesses_erneut() {
        use std::os::unix::fs::PermissionsExt;

        let wurzel = baum("loeschen-warten");
        let ablage = Ablage::neu(&wurzel);
        let programm = programm("quiver-devops", b"hallo");
        let pfad = ablage.pfad(&programm);
        let ordner = pfad.parent().expect("Ordner").to_path_buf();
        std::fs::create_dir_all(&ordner).expect("Ordner");
        std::fs::write(&pfad, b"hallo").expect("Datei");

        // Solange der Ordner nur lesbar ist, lässt sich nichts darin löschen
        // (unter Windows: solange der Prozess die Datei hält).
        std::fs::set_permissions(&ordner, std::fs::Permissions::from_mode(0o500)).expect("Rechte");
        let frei = ordner.clone();
        let geber = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            std::fs::set_permissions(&frei, std::fs::Permissions::from_mode(0o700))
                .expect("Rechte");
        });
        let weg = ablage.loeschen_warten("quiver-devops").await;
        geber.join().expect("Geber");
        assert!(weg, "das Programm ist gelöscht");
        assert!(!ablage.ordner("quiver-devops").exists());

        // Und wenn es schon weg ist, ist das kein Fehler.
        assert!(ablage.loeschen_warten("quiver-devops").await);
        // Fremde Namen werden nicht angefasst.
        assert!(ablage.loeschen_warten("meine-notizen").await);
    }
}
