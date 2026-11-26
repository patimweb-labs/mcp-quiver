// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Updates aus Sicht des Clients (Aufgabe 34, erweitert in Aufgabe 39).
//!
//! Der Client fragt **nur den Server**, nie das Internet. Ist die neue Version
//! größer als die laufende, zeigt er einen ruhigen Hinweis in Fenster und Tray
//! – und, wenn es ein passendes Paket gibt, einen Knopf „Jetzt aktualisieren“.
//! Erst dieser Knopf lädt: das Paket kommt vom eigenen Server (mit dem
//! Geräte-Token), wird gegen Größe und SHA-256 aus der Antwort geprüft, mit
//! dem Selbsttest gestartet und erst dann getauscht.
//!
//! Kein Signaturschlüssel, kein fremdes Werkzeug: die Prüfsumme steht im
//! selben Release wie das Paket, und der Client prüft sie erneut. Was die
//! Firma nicht will, schaltet sie über die Vorgaben ab – dann bleibt es beim
//! Hinweis. Die Konfiguration des Benutzers (Profil, Tresor, Geräte-Token,
//! Angaben, Zugänge, Anmeldungen) wird dabei nicht angefasst.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use quiver_domain::updates::{
    self, version_lesen, version_neuer, PaketArt, TEXT_MIT_VERWALTUNG, TEXT_OHNE_VERWALTUNG,
};
use quiver_protocol::{UpdateAngebot, UpdatePaket, PRAEFIX};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::vorgaben::Vorgaben;
use crate::{netz, Innen};

pub mod art;
pub mod entpacken;
pub mod tausch;

pub use art::{Aufstellung, Installationsart};

/// So oft fragt der Client den Server (der Server selbst fragt höchstens alle
/// sechs Stunden die Veröffentlichung).
pub const ABSTAND: Duration = Duration::from_secs(6 * 60 * 60);
/// Streuung um [`ABSTAND`].
pub const STREUUNG: Duration = Duration::from_secs(30 * 60);
/// So groß darf die Antwort des Servers höchstens sein.
pub const ANGEBOT_BYTES: usize = 64 * 1024;
/// So groß darf ein Paket höchstens sein (dieselbe Grenze im Server).
pub const PAKET_BYTES_MAX: u64 = 512 * 1024 * 1024;
/// So lange darf ein Paket höchstens laden.
pub const PAKET_FRIST: Duration = Duration::from_secs(30 * 60);
/// So lange darf der Selbsttest der neuen Fassung höchstens dauern.
pub const SELBSTTEST_FRIST: Duration = Duration::from_secs(60);

/// Die Schritte, die das Fenster beim Aktualisieren zeigt (wie beim Laden der
/// Programme: Spinner und Text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateSchritt {
    /// Das Paket kommt vom Server.
    Laedt,
    /// Größe und Prüfsumme werden geprüft, die neue Fassung macht den
    /// Selbsttest.
    Prueft,
    /// Die alte Fassung wird getauscht.
    Tauscht,
}

/// Der Stand des Updates, wie ihn das Fenster zeigt (Summentyp, kein Fächer
/// aus Booleschen Werten).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum UpdateLage {
    /// Noch nicht gefragt.
    #[default]
    Unbekannt,
    /// Nichts zu tun.
    Keins,
    /// Eine neuere Version ist veröffentlicht; `hinweis` ist der fertige Satz
    /// aus der Vorgabe der Firma.
    Verfuegbar {
        version: String,
        hinweis: String,
        #[serde(default)]
        aenderungen: Option<String>,
        /// Darf der Client tauschen? Und wenn nicht, warum (Aufgabe 39)?
        #[serde(default)]
        angebot: Angebot,
    },
    /// Der Benutzer hat „Jetzt aktualisieren“ gedrückt: es läuft.
    Arbeit {
        schritt: UpdateSchritt,
        /// Anteil des Ladens (0–100), solange die Größe bekannt ist.
        #[serde(default)]
        anteil: Option<u8>,
    },
    /// Getauscht und geprüft: der Neustart läuft.
    Getauscht { version: String },
    /// Fragen oder Tauschen ist gescheitert.
    Fehler { text: String },
}

impl UpdateLage {
    /// Steht ein Update an (für Hinweis in Fenster und Tray)?
    pub fn anstehend(&self) -> Option<(&str, &str)> {
        match self {
            UpdateLage::Verfuegbar {
                version, hinweis, ..
            } => Some((version, hinweis)),
            _ => None,
        }
    }
}

/// Ob der Client das Update anbieten darf – und wenn nicht, warum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum Angebot {
    /// Es gibt einen Knopf „Jetzt aktualisieren“.
    Bereit { paket: UpdatePaket },
    /// Kein Knopf; `grund` kommt als Satz aus den Texten der Oberfläche.
    Keins { grund: AngebotGrund },
}

impl Default for Angebot {
    fn default() -> Self {
        Self::Keins {
            grund: AngebotGrund::default(),
        }
    }
}

/// Warum es keinen Knopf gibt. Die Ansicht macht daraus einen Satz.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AngebotGrund {
    /// Die Firma hat das Angebot per Vorgabe abgeschaltet.
    Vorgabe,
    /// Der Programmordner gehört root oder der Geräteverwaltung.
    KeinSchreibrecht,
    /// Die Installation braucht root (`deb`, `rpm`): Befehl statt Knopf.
    BrauchtRechte,
    /// Für diese Installation gibt es kein Paket in der Veröffentlichung.
    KeinPaket,
    /// Die Art der Installation ist unbekannt.
    #[default]
    Unbekannt,
}

/// Den Server fragen: welche Version ist veröffentlicht, und welche Pakete gibt
/// es für dieses System?
pub async fn angebot_holen(
    http: &reqwest::Client,
    server: &str,
    token: &str,
    version: &str,
    system: &quiver_domain::System,
) -> anyhow::Result<UpdateAngebot> {
    let adresse = format!(
        "{server}{PRAEFIX}/geraet/updates?version={}&system={}&arch={}",
        url_teil(version),
        url_teil(&system.system),
        url_teil(&system.arch)
    );
    let antwort = netz::frist(http.get(&adresse).bearer_auth(token))
        .send()
        .await
        .with_context(|| format!("{adresse} nicht erreichbar"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("{adresse} antwortet mit {}", antwort.status());
    }
    netz::json_begrenzt(antwort, ANGEBOT_BYTES).await
}

/// Nur Ziffern, Buchstaben, Punkt, Strich und Unterstrich in der Abfrage.
fn url_teil(wert: &str) -> String {
    wert.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .take(64)
        .collect()
}

/// Aus dem Angebot die Anzeige machen.
///
/// Kein Rückschritt: ist die neueste Version nicht größer als die laufende,
/// gibt es nichts zu zeigen – auch dann nicht, wenn der Server etwas anderes
/// behauptet. Der Text kommt aus der Vorgabe der Firma (oder aus dem
/// Standardtext, je nachdem ob das Gerät verwaltet ist).
///
/// Ob es einen Knopf gibt, entscheidet [`angebot_waehlen`]: nur mit einem
/// Paket, das zu **dieser** Installation passt und geprüft werden kann.
pub fn lage_aus(
    angebot: &UpdateAngebot,
    vorgaben: &Vorgaben,
    laufend: &str,
    aufstellung: &Aufstellung,
) -> UpdateLage {
    let Some(neueste) = angebot.neueste_version.as_deref() else {
        return UpdateLage::Keins;
    };
    if !version_neuer(neueste, laufend) {
        return UpdateLage::Keins;
    }
    let vorlage = match (&vorgaben.update_hinweis, vorgaben.verwaltet()) {
        (Some(text), _) => text.as_str(),
        (None, true) => TEXT_MIT_VERWALTUNG,
        (None, false) => TEXT_OHNE_VERWALTUNG,
    };
    // Für die Anzeige zählt die gelesene Form (`v0.3.0` wird `0.3.0`).
    let version = version_lesen(neueste).map_or_else(|| neueste.to_string(), |v| v.to_string());
    UpdateLage::Verfuegbar {
        version: version.clone(),
        hinweis: updates::hinweis_text(vorlage, &version),
        aenderungen: angebot
            .aenderungen
            .clone()
            .filter(|link| link.starts_with("https://")),
        angebot: angebot_waehlen(angebot, vorgaben, aufstellung),
    }
}

/// Darf das Update angeboten werden? Welches Paket gehört dazu?
///
/// Reihenfolge der Absage: die Vorgabe der Firma geht vor, dann die Rechte,
/// dann das Paket. Was nicht geprüft werden kann (keine Prüfsumme, keine
/// Größe), wird nicht angeboten – der Client tauscht nur, was er prüfen kann.
pub fn angebot_waehlen(
    angebot: &UpdateAngebot,
    vorgaben: &Vorgaben,
    aufstellung: &Aufstellung,
) -> Angebot {
    if vorgaben.update_angebot == Some(false) {
        return Angebot::Keins {
            grund: AngebotGrund::Vorgabe,
        };
    }
    if !aufstellung.art.tauschbar() {
        return Angebot::Keins {
            grund: match aufstellung.art {
                Installationsart::Deb | Installationsart::Rpm => AngebotGrund::BrauchtRechte,
                _ => AngebotGrund::Unbekannt,
            },
        };
    }
    if !aufstellung.schreibbar {
        return Angebot::Keins {
            grund: AngebotGrund::KeinSchreibrecht,
        };
    }
    let Some(art) = tausch::paketart_fuer(aufstellung.art) else {
        return Angebot::Keins {
            grund: AngebotGrund::Unbekannt,
        };
    };
    let paket = angebot
        .pakete
        .iter()
        .find(|paket| passt_zu(paket, art))
        .filter(|paket| {
            quiver_domain::updates::sha256_gueltig(&paket.sha256)
                && paket.groesse > 0
                && paket.groesse <= PAKET_BYTES_MAX
                && ist_dateiname(&paket.name)
        });
    match paket {
        Some(paket) => Angebot::Bereit {
            paket: paket.clone(),
        },
        None => Angebot::Keins {
            grund: AngebotGrund::KeinPaket,
        },
    }
}

/// Gehört dieses Paket zu dieser Art? (Endung **und** Angabe im Vertrag)
fn passt_zu(paket: &UpdatePaket, art: PaketArt) -> bool {
    PaketArt::aus_text(&paket.art) == Some(art) && PaketArt::aus_name(&paket.name) == Some(art)
}

/// Ist der Name ein reiner Dateiname?
fn ist_dateiname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.starts_with('.')
}

/// Das Paket vom Server laden und **vor** dem ersten Schritt prüfen.
///
/// Geprüft wird zweimal dasselbe: die angekündigte Größe (`Content-Length`)
/// und die tatsächlich geladene Menge, dazu SHA-256 über alles. Was nicht
/// stimmt, wird gelöscht – eine halbe Datei bleibt nie unter dem richtigen
/// Namen liegen.
pub async fn paket_holen(
    http: &reqwest::Client,
    server: &str,
    token: &str,
    system: &quiver_domain::System,
    paket: &UpdatePaket,
    ordner: &Path,
    mut fortschritt: impl FnMut(Option<u8>),
) -> anyhow::Result<PathBuf> {
    use futures_util::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    if !quiver_domain::updates::sha256_gueltig(&paket.sha256)
        || paket.groesse == 0
        || paket.groesse > PAKET_BYTES_MAX
        || !ist_dateiname(&paket.name)
    {
        anyhow::bail!("der Server nennt ein unbrauchbares Paket");
    }
    std::fs::create_dir_all(ordner)?;
    let ziel = ordner.join(&paket.name);
    // Was schon richtig daliegt, wird nicht erneut geladen.
    if let Ok(()) = datei_pruefen(&ziel, paket).await {
        fortschritt(Some(100));
        return Ok(ziel);
    }
    let adresse = format!(
        "{server}{PRAEFIX}/geraet/updates/paket/{}?system={}&arch={}",
        paket.name,
        url_teil(&system.system),
        url_teil(&system.arch)
    );
    let antwort = http
        .get(&adresse)
        .bearer_auth(token)
        .timeout(PAKET_FRIST)
        .send()
        .await
        .with_context(|| format!("{adresse} nicht erreichbar"))?;
    let antwort = crate::abgleich::pruefen(antwort).await?;
    if let Some(laenge) = antwort.content_length() {
        if laenge != paket.groesse {
            anyhow::bail!(
                "der Server schickt {laenge} Bytes, angekündigt waren {}",
                paket.groesse
            );
        }
    }
    let zwischen = ordner.join(format!(
        ".laden-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dauer| dauer.as_nanos())
            .unwrap_or(0)
    ));
    let mut datei = tokio::fs::File::create(&zwischen).await?;
    let mut hash = Sha256::new();
    let mut gesamt: u64 = 0;
    let mut letzter_anteil = 0u8;
    let mut strom = antwort.bytes_stream();
    let ergebnis = loop {
        let stueck = match strom.next().await {
            Some(Ok(stueck)) => stueck,
            Some(Err(fehler)) => break Err(anyhow::anyhow!("{adresse} bricht ab: {fehler}")),
            None => break Ok(()),
        };
        gesamt += stueck.len() as u64;
        if gesamt > paket.groesse.min(PAKET_BYTES_MAX) {
            break Err(anyhow::anyhow!(
                "der Server schickt mehr, als er angekündigt hat"
            ));
        }
        hash.update(&stueck);
        if let Err(fehler) = datei.write_all(&stueck).await {
            break Err(anyhow::anyhow!(
                "das Paket lässt sich nicht schreiben: {fehler}"
            ));
        }
        let anteil = (gesamt * 100 / paket.groesse.max(1)).min(100) as u8;
        if anteil != letzter_anteil {
            letzter_anteil = anteil;
            fortschritt(Some(anteil));
        }
    };
    let ergebnis = match ergebnis {
        Ok(()) => {
            if gesamt != paket.groesse {
                Err(anyhow::anyhow!(
                    "das Paket ist {gesamt} statt {} Bytes groß",
                    paket.groesse
                ))
            } else if hex::encode(hash.finalize()) != paket.sha256 {
                Err(anyhow::anyhow!("die Prüfsumme des Pakets stimmt nicht"))
            } else {
                let _ = datei.flush().await;
                let _ = datei.sync_all().await;
                Ok(())
            }
        }
        Err(fehler) => {
            let _ = tokio::fs::remove_file(&zwischen).await;
            return Err(fehler);
        }
    };
    drop(datei);
    if let Err(fehler) = ergebnis {
        let _ = tokio::fs::remove_file(&zwischen).await;
        return Err(fehler);
    }
    // Umbenennen: von hier an ist es das Paket.
    if ziel.exists() {
        let _ = tokio::fs::remove_file(&ziel).await;
    }
    tokio::fs::rename(&zwischen, &ziel).await?;
    fortschritt(Some(100));
    Ok(ziel)
}

/// Prüfen, ob eine Datei Größe und Prüfsumme des Pakets trägt.
pub async fn datei_pruefen(pfad: &Path, paket: &UpdatePaket) -> anyhow::Result<()> {
    use tokio::io::AsyncReadExt as _;
    let meta = tokio::fs::symlink_metadata(pfad).await?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        anyhow::bail!("kein Paket");
    }
    if meta.len() != paket.groesse {
        anyhow::bail!("falsche Größe");
    }
    let mut datei = tokio::fs::File::open(pfad).await?;
    let mut hash = Sha256::new();
    let mut puffer = vec![0u8; 64 * 1024];
    loop {
        let gelesen = datei.read(&mut puffer).await?;
        if gelesen == 0 {
            break;
        }
        hash.update(&puffer[..gelesen]);
    }
    if hex::encode(hash.finalize()) != paket.sha256 {
        anyhow::bail!("falsche Prüfsumme");
    }
    Ok(())
}

/// Der Selbsttest einer Fassung: `quiver --selbsttest`, Exit 0, ohne Fenster,
/// ohne Profil.
///
/// Er läuft in einem eigenen Prozess mit leerer Umgebung für die Anzeige und
/// mit einer Frist: ein hängendes Programm wird beendet und zählt als
/// gescheitert.
pub fn selbsttest(programm: &Path) -> bool {
    let mut befehl = std::process::Command::new(programm);
    befehl
        .arg("--selbsttest")
        // Kein fremdes Profil, kein fremder Endpunkt, keine Anzeige.
        .env_remove("QUIVER_PROFIL")
        .env_remove("QUIVER_MCP_ADRESSE")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        befehl.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(mut kind) = befehl.spawn() else {
        return false;
    };
    let ende = std::time::Instant::now() + SELBSTTEST_FRIST;
    loop {
        match kind.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {}
            Err(_) => {
                let _ = kind.kill();
                let _ = kind.wait();
                return false;
            }
        }
        if std::time::Instant::now() >= ende {
            let _ = kind.kill();
            let _ = kind.wait();
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

// ------------------------------------------------------------ Hintergrund

/// Fragt im Hintergrund, ob eine neuere Version veröffentlicht ist.
pub(crate) async fn lauf(innen: std::sync::Arc<Innen>) {
    let mut naechste = tokio::time::Instant::now();
    loop {
        if innen.ende.ist_abgebrochen() {
            return;
        }
        let jetzt = tokio::time::Instant::now();
        if jetzt >= naechste {
            if let Err(fehler) = pruefen(&innen).await {
                tracing::debug!("Update-Prüfung: {fehler:#}");
            }
            let streuung = if STREUUNG.is_zero() {
                Duration::ZERO
            } else {
                Duration::from_secs(rand::random::<u64>() % STREUUNG.as_secs().max(1))
            };
            naechste = jetzt + ABSTAND + streuung;
        }
        tokio::select! {
            _ = innen.ende.warten() => return,
            _ = innen.update_erneut.notified() => {
                // Neue Anmeldung: gleich fragen, aber nicht in einer Schleife.
                naechste = tokio::time::Instant::now();
            }
            _ = tokio::time::sleep(ABSTAND) => {}
        }
    }
}

/// Einmal fragen und den Stand setzen. Ohne Verbindung passiert nichts.
///
/// Ein laufender Tausch oder ein Fehler, den der Benutzer gerade liest, wird
/// nicht von einer Hintergrundfrage überschrieben: nur ein neues Angebot,
/// „nichts zu tun“ und ein neues Angebot nach dem Ablehnen ersetzen ihn.
pub(crate) async fn pruefen(innen: &std::sync::Arc<Innen>) -> anyhow::Result<()> {
    let (server, token) = innen.zugang();
    let (Some(server), Some(token)) = (server, token) else {
        return Ok(());
    };
    let angebot = angebot_holen(
        &innen.http,
        &server,
        &token,
        crate::CLIENT_VERSION,
        &innen.system,
    )
    .await?;
    let lage = lage_aus(
        &angebot,
        &innen.konfig.vorgaben,
        crate::CLIENT_VERSION,
        &innen.aufstellung,
    );
    // Ein laufender Tausch und sein Ergebnis werden nicht von einer
    // Hintergrundfrage überschrieben: das Fenster zeigt gerade „lädt“ oder
    // „Neustart“, und die Antwort wäre von vorhin.
    if matches!(
        innen.stand().update,
        UpdateLage::Arbeit { .. } | UpdateLage::Getauscht { .. }
    ) {
        return Ok(());
    }
    innen.update_setzen(lage);
    Ok(())
}

#[cfg(test)]
mod tests;
