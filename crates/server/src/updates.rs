// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Updates auf dem Server (Aufgabe 34): **nur anzeigen**.
//!
//! Der Server fragt höchstens alle sechs Stunden die Veröffentlichung ab
//! (GitHub-API, ohne Kennung, ohne Nutzdaten; abschaltbar mit
//! `QUIVER_UPDATES=aus`) und merkt sich das Ergebnis in `update_stand`. Aus
//! diesem Stand beantwortet er zwei Fragen:
//!
//! * **Gerät** (`GET /api/v1/geraet/updates`): ist eine neuere Version
//!   veröffentlicht als die, die du meldest? Der Client zeigt dann einen
//!   ruhigen Hinweis – er lädt und installiert nichts.
//! * **Verwaltung** (`GET /api/v1/admin/updates`): welche Version ist
//!   veröffentlicht, ist der Server selbst älter, und welche Geräte melden
//!   einen älteren Client?
//!
//! Eingespielt wird durch die Geräteverwaltung (Intune) oder von Hand aus den
//! Paketen des Releases – siehe `docs/intune.md`.
//!
//! **Aufgabe 39:** Der Server holt das Paket auch selbst. Er lädt es einmal von
//! der Veröffentlichung (`https`, in Tests Loopback), prüft es gegen
//! `SHA256SUMS`, legt es unter `<daten>/updates/<version>/` ab und liefert es
//! dem Gerät mit dessen Token aus – gestreamt, mit Größe, `ETag` und Drossel,
//! wie die Programme in `geraet.rs`. Ein Signaturschlüssel ist dafür nicht
//! nötig: die Prüfsumme steht im selben Release wie das Paket, und der Client
//! prüft sie erneut.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Extension, Path as Pfad, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use quiver_domain::updates::{
    self, Paketwahl, Veroeffentlichung, ANTWORT_BYTES_MAX, PAKET_BYTES_MAX, PRUEFSUMMEN_NAME,
};
use quiver_domain::{GeraetId, System};
use quiver_protocol::{UpdateAngebot, UpdatePaket, UpdateStand, VeralteterClient, VERALTETE_MAX};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::fehler::{Ergebnis, FehlerAntwort};
use crate::speicher::{self, UpdateStandZeile};
use crate::zustand::Zustand;
use crate::{db, hilfen};

/// So lange darf eine Antwort der Veröffentlichung höchstens dauern.
const ABRUF_FRIST: Duration = Duration::from_secs(30);
/// So lange darf ein Paket höchstens laden (große Dateien, langsame Leitung).
const PAKET_FRIST: Duration = Duration::from_secs(30 * 60);
/// So groß darf `SHA256SUMS` höchstens sein.
const PRUEFSUMMEN_BYTES_MAX: usize = 256 * 1024;
/// So viele Zeilenstücke liefert ein Strom höchstens (64 KiB je Stück).
const STROM_STUECK: u64 = 64 * 1024;

/// Was der Server über die Veröffentlichung weiß.
#[derive(Default)]
pub(crate) struct Lage {
    /// Die zuletzt gelesene Veröffentlichung.
    pub(crate) veroeffentlichung: Option<Veroeffentlichung>,
    /// Wann zuletzt erfolgreich gefragt wurde.
    pub(crate) zuletzt: Option<OffsetDateTime>,
    /// Warum die letzte Abfrage scheiterte (ohne Geheimnisse).
    pub(crate) fehler: Option<String>,
    /// Die Prüfsummen der Veröffentlichung (`SHA256SUMS`), gelesen beim
    /// Prüfen und beim Ausliefern. Ohne sie gibt es kein Paket.
    pub(crate) pruefsummen: BTreeMap<String, String>,
}

impl Lage {
    pub(crate) fn neu() -> Self {
        Self::default()
    }

    /// Den gemerkten Stand übernehmen (nach einem Neustart).
    pub(crate) fn aus_zeile(&mut self, zeile: UpdateStandZeile) {
        self.veroeffentlichung = zeile.veroeffentlichung;
        self.zuletzt = zeile.zuletzt.as_deref().and_then(|text| {
            OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()
        });
    }

    fn als_zeile(&self) -> UpdateStandZeile {
        UpdateStandZeile {
            veroeffentlichung: self.veroeffentlichung.clone(),
            zuletzt: self.zuletzt.map(hilfen::als_text),
        }
    }
}

// ------------------------------------------------------------- Hintergrund

/// Fragt die Veröffentlichung im Hintergrund ab (höchstens alle sechs Stunden,
/// mit Streuung). Mit `QUIVER_UPDATES=aus` wird **nicht** gefragt.
pub(crate) async fn hintergrund(zustand: Arc<Zustand>) {
    let konfig = zustand.konfig.updates.clone();
    if !konfig.an {
        tracing::info!("Update-Prüfung ist aus (QUIVER_UPDATES=aus)");
        return;
    }
    // Den gemerkten Stand übernehmen und den Abstand ab der letzten Frage
    // rechnen: ein Neustart fragt nicht sofort wieder.
    let gemerkt = db(&zustand, |conn| speicher::update_stand(conn))
        .await
        .ok()
        .flatten();
    if let Some(zeile) = gemerkt {
        crate::sperre(&zustand.updates).aus_zeile(zeile);
    }
    loop {
        if let Err(fehler) = pruefen(&zustand).await {
            tracing::warn!("Update-Prüfung: {fehler:#}");
            let mut lage = crate::sperre(&zustand.updates);
            lage.fehler = Some(format!("{fehler:#}"));
            lage.zuletzt = Some(zustand.uhr.jetzt());
        }
        let streuung = if konfig.streuung.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_secs(rand::random::<u64>() % konfig.streuung.as_secs().max(1))
        };
        let warte = konfig.abstand + streuung;
        let mut ende = zustand.ende_abo();
        tokio::select! {
            _ = ende.recv() => return,
            _ = tokio::time::sleep(warte) => {}
        }
    }
}

/// Die Adresse der Veröffentlichung, aus der Basis der Quelle.
///
/// Für GitHub wird die API gefragt (JSON statt HTML): `github.com/<owner>/<repo>`
/// wird zu `api.github.com/repos/<owner>/<repo>/releases/latest`. Eine
/// Loopback-Quelle (Tests) wird unverändert genommen, damit der Test einen
/// eigenen Server stellen kann.
pub fn api_adresse(quelle: &str) -> String {
    let basis = quelle.trim_end_matches('/');
    if let Some(rest) = basis.strip_prefix("https://github.com/") {
        return format!("https://api.github.com/repos/{rest}/releases/latest");
    }
    format!("{basis}/releases/latest")
}

/// Einmal fragen: Antwort holen, lesen, merken (im Speicher und in der Datenbank).
async fn pruefen(zustand: &Arc<Zustand>) -> anyhow::Result<()> {
    let url = api_adresse(&zustand.konfig.updates.quelle);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        // Ohne Kennung: kein Gerät, kein Benutzer, keine Instanz.
        .user_agent("quiver")
        .timeout(ABRUF_FRIST)
        .build()
        .unwrap_or_default();
    let antwort = client
        .get(&url)
        .send()
        .await
        .map_err(|fehler| anyhow::anyhow!("{url} nicht erreichbar: {fehler}"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("{url} antwortet mit {}", antwort.status());
    }
    let text = antwort
        .text()
        .await
        .map_err(|fehler| anyhow::anyhow!("{url} nicht lesbar: {fehler}"))?;
    if text.len() > ANTWORT_BYTES_MAX {
        anyhow::bail!("{url} ist zu groß ({} Bytes)", text.len());
    }
    let veroeffentlichung =
        updates::veroeffentlichung_lesen(&text).map_err(|fehler| anyhow::anyhow!("{fehler}"))?;
    let jetzt = zustand.uhr.jetzt();
    let zeile = {
        let mut lage = crate::sperre(&zustand.updates);
        tracing::info!(
            "Veröffentlichte Version gelesen: {}",
            veroeffentlichung.version
        );
        // Eine neue Veröffentlichung: die alten Prüfsummen gelten nicht mehr.
        if lage
            .veroeffentlichung
            .as_ref()
            .map(|alt| alt.version != veroeffentlichung.version)
            .unwrap_or(true)
        {
            lage.pruefsummen.clear();
        }
        lage.veroeffentlichung = Some(veroeffentlichung);
        lage.zuletzt = Some(jetzt);
        lage.fehler = None;
        lage.als_zeile()
    };
    let gemerkt = zeile.clone();
    db(zustand, move |conn| {
        speicher::update_stand_setzen(conn, &gemerkt)
    })
    .await
    .map_err(|_| anyhow::anyhow!("der Stand der Veröffentlichung ließ sich nicht ablegen"))?;
    Ok(())
}

// ---------------------------------------------------------- Prüfsummen
//
// Ohne `SHA256SUMS` gibt es kein Paket: was der Client nicht prüfen kann,
// tauscht er nicht. Die Datei wird einmal geholt, geprüft (64 Hex-Zeichen je
// Zeile), unter `<daten>/updates/<version>/SHA256SUMS` abgelegt und von dort
// beim nächsten Start wieder gelesen.

/// Der Ordner der Pakete einer Version: `<daten>/updates/<version>`.
fn paket_ordner(zustand: &Zustand, version: &str) -> PathBuf {
    zustand.konfig.daten.join("updates").join(version)
}

/// Eine URL als Bytes holen, mit Grenze und Frist (kein Weiterleiten über
/// eine andere Adresse: was der Server nennt, wird geholt).
async fn holen_bytes(url: &str, hoechstens: u64, frist: Duration) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .user_agent("quiver")
        .timeout(frist)
        .build()
        .unwrap_or_default();
    let antwort = client
        .get(url)
        .send()
        .await
        .map_err(|fehler| anyhow::anyhow!("{url} nicht erreichbar: {fehler}"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("{url} antwortet mit {}", antwort.status());
    }
    if let Some(laenge) = antwort.content_length() {
        if laenge > hoechstens {
            anyhow::bail!("{url} ist zu groß ({laenge} Bytes)");
        }
    }
    let bytes = antwort
        .bytes()
        .await
        .map_err(|fehler| anyhow::anyhow!("{url} nicht lesbar: {fehler}"))?;
    if bytes.len() as u64 > hoechstens {
        anyhow::bail!("{url} ist zu groß ({} Bytes)", bytes.len());
    }
    Ok(bytes.to_vec())
}

/// Die Prüfsummen der Veröffentlichung bereitstellen: aus dem Speicher, aus
/// der Ablage oder von der Veröffentlichung. `false` heißt: es gibt keine.
pub(crate) async fn pruefsummen_sicherstellen(zustand: &Arc<Zustand>) -> bool {
    let (version, adresse) = {
        let lage = crate::sperre(&zustand.updates);
        if !lage.pruefsummen.is_empty() {
            return true;
        }
        let Some(veroeffentlichung) = lage.veroeffentlichung.as_ref() else {
            return false;
        };
        (
            veroeffentlichung.version.clone(),
            veroeffentlichung
                .pruefsummen_datei()
                .and_then(|datei| datei.adresse.clone()),
        )
    };
    let datei = paket_ordner(zustand, &version).join(PRUEFSUMMEN_NAME);
    let text = match tokio::fs::read_to_string(&datei).await {
        Ok(text) => Some(text),
        Err(_) => match adresse {
            // Kein Netz im Test: die Attrappe antwortet über Loopback, die
            // Adresse ist geprüft (`adresse_erlaubt`).
            Some(adresse) => {
                match holen_bytes(&adresse, PRUEFSUMMEN_BYTES_MAX as u64, ABRUF_FRIST).await {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        // Ablage: nur wenn der Ordner entstehen darf. Scheitert
                        // das Schreiben, gilt der Stand trotzdem für diesen Lauf.
                        if tokio::fs::create_dir_all(paket_ordner(zustand, &version))
                            .await
                            .is_ok()
                        {
                            let _ = tokio::fs::write(&datei, &text).await;
                        }
                        Some(text)
                    }
                    Err(fehler) => {
                        tracing::warn!("Prüfsummen nicht abrufbar: {fehler:#}");
                        None
                    }
                }
            }
            None => None,
        },
    };
    let Some(text) = text else {
        return false;
    };
    let pruefsummen = updates::pruefsummen_lesen(&text);
    if pruefsummen.is_empty() {
        tracing::warn!("Prüfsummen der Veröffentlichung sind leer oder unbrauchbar");
        return false;
    }
    let wieder = {
        let mut lage = crate::sperre(&zustand.updates);
        if lage.veroeffentlichung.as_ref().map(|v| &v.version) == Some(&version) {
            lage.pruefsummen = pruefsummen;
            true
        } else {
            // Inzwischen eine andere Veröffentlichung: dieser Lauf hat nichts
            // mehr zu sagen.
            false
        }
    };
    wieder
}

/// Der Stand der Veröffentlichung, notfalls frisch geholt.
///
/// Der Client fragt nach einem Paket; hat der Server noch nichts gehört (etwa
/// gleich nach dem Start), fragt er selbst einmal. Die Sperre hält mehrere
/// gleichzeitige Fragen auseinander: ein Paket, ein Abruf.
pub(crate) async fn sicherstellen(zustand: &Arc<Zustand>, frisch: bool) -> anyhow::Result<()> {
    if !zustand.konfig.updates.an {
        anyhow::bail!("die Update-Abfrage ist aus (QUIVER_UPDATES=aus)");
    }
    let _wache = zustand.update_wache.lock().await;
    let bekannt = crate::sperre(&zustand.updates).veroeffentlichung.is_some();
    if frisch || !bekannt {
        let gemerkt = db(zustand, |conn| speicher::update_stand(conn))
            .await
            .ok()
            .flatten();
        if let Some(zeile) = gemerkt {
            crate::sperre(&zustand.updates).aus_zeile(zeile);
        }
    }
    if !crate::sperre(&zustand.updates).veroeffentlichung.is_some() {
        pruefen(zustand).await?;
    }
    if !pruefsummen_sicherstellen(zustand).await {
        anyhow::bail!("die Prüfsummen der Veröffentlichung fehlen");
    }
    Ok(())
}

/// Die Pakete der Veröffentlichung für ein System – mit Prüfsummen.
pub(crate) fn pakete_fuer_system(zustand: &Zustand, system: &System) -> Vec<Paketwahl> {
    let lage = crate::sperre(&zustand.updates);
    let Some(veroeffentlichung) = lage.veroeffentlichung.as_ref() else {
        return Vec::new();
    };
    updates::pakete_fuer(
        veroeffentlichung,
        &system.system,
        &system.arch,
        &lage.pruefsummen,
    )
}

/// Aus einer Wahl das machen, was im Vertrag steht.
fn als_vertrag(wahl: &Paketwahl) -> UpdatePaket {
    UpdatePaket {
        name: wahl.name.clone(),
        groesse: wahl.groesse,
        sha256: wahl.sha256.clone(),
        art: wahl.art.als_text().to_string(),
    }
}

/// Ein Paket bereitstellen: aus der Ablage oder von der Veröffentlichung.
///
/// Geladen wird in eine Zwischendatei im Zielordner (dasselbe Dateisystem,
/// ein Umbenennen ohne Kopie); dabei werden Größe **und** SHA-256 mitgezählt.
/// Erst wenn beides stimmt, entsteht der endgültige Name. Was nicht stimmt,
/// wird gelöscht – ein halbes Paket liegt nie unter dem richtigen Namen.
async fn paket_bereitstellen(
    zustand: &Arc<Zustand>,
    version: &str,
    wahl: &Paketwahl,
) -> anyhow::Result<PathBuf> {
    let ordner = paket_ordner(zustand, version);
    let ziel = ordner.join(&wahl.name);
    if paket_pruefen(&ziel, wahl).await.is_ok() {
        return Ok(ziel);
    }
    let adresse = wahl.adresse.clone().ok_or_else(|| {
        anyhow::anyhow!("die Veröffentlichung nennt keine Adresse für {}", wahl.name)
    })?;
    tokio::fs::create_dir_all(&ordner)
        .await
        .map_err(|fehler| anyhow::anyhow!("die Ablage {} fehlt: {fehler}", ordner.display()))?;
    let zwischen = ordner.join(format!(
        ".laden-{}-{}-{}",
        &wahl.sha256[..16.min(wahl.sha256.len())],
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|dauer| dauer.as_nanos())
            .unwrap_or(0)
    ));
    let ergebnis = laden(&adresse, wahl, &zwischen).await;
    if let Err(fehler) = ergebnis {
        let _ = tokio::fs::remove_file(&zwischen).await;
        return Err(fehler);
    }
    if let Err(fehler) = tokio::fs::rename(&zwischen, &ziel).await {
        let _ = tokio::fs::remove_file(&zwischen).await;
        anyhow::bail!("das Paket lässt sich nicht ablegen: {fehler}");
    }
    tracing::info!(paket = %wahl.name, "Paket der Veröffentlichung abgelegt");
    Ok(ziel)
}

/// Ein Paket laden und dabei Größe und Prüfsumme zählen.
async fn laden(adresse: &str, wahl: &Paketwahl, zwischen: &Path) -> anyhow::Result<()> {
    use futures_util::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .user_agent("quiver")
        .timeout(PAKET_FRIST)
        .build()
        .unwrap_or_default();
    let antwort = client
        .get(adresse)
        .send()
        .await
        .map_err(|fehler| anyhow::anyhow!("{adresse} nicht erreichbar: {fehler}"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("{adresse} antwortet mit {}", antwort.status());
    }
    if let Some(laenge) = antwort.content_length() {
        if laenge > PAKET_BYTES_MAX {
            anyhow::bail!("{adresse} ist zu groß ({laenge} Bytes)");
        }
    }
    let mut datei = tokio::fs::File::create(zwischen).await?;
    let mut hash = Sha256::new();
    let mut gesamt: u64 = 0;
    let mut strom = antwort.bytes_stream();
    while let Some(stueck) = strom.next().await {
        let stueck = stueck.map_err(|fehler| anyhow::anyhow!("{adresse} bricht ab: {fehler}"))?;
        gesamt += stueck.len() as u64;
        if gesamt > PAKET_BYTES_MAX {
            anyhow::bail!("{adresse} ist größer als erlaubt");
        }
        hash.update(&stueck);
        datei.write_all(&stueck).await?;
    }
    datei.flush().await?;
    datei.sync_all().await?;
    drop(datei);
    if wahl.groesse > 0 && gesamt != wahl.groesse {
        anyhow::bail!(
            "{} ist {gesamt} statt {} Bytes groß",
            wahl.name,
            wahl.groesse
        );
    }
    let sha256 = hex::encode(hash.finalize());
    if sha256 != wahl.sha256 {
        anyhow::bail!("die Prüfsumme von {} stimmt nicht", wahl.name);
    }
    Ok(())
}

/// Liegt das Paket da und stimmt es?
async fn paket_pruefen(pfad: &Path, wahl: &Paketwahl) -> anyhow::Result<()> {
    use tokio::io::AsyncReadExt as _;
    let meta = tokio::fs::symlink_metadata(pfad).await?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        anyhow::bail!("kein Paket");
    }
    if wahl.groesse > 0 && meta.len() != wahl.groesse {
        anyhow::bail!("falsche Größe");
    }
    let mut datei = tokio::fs::File::open(pfad).await?;
    let mut hash = Sha256::new();
    let mut puffer = vec![0u8; STROM_STUECK as usize];
    loop {
        let gelesen = datei.read(&mut puffer).await?;
        if gelesen == 0 {
            break;
        }
        hash.update(&puffer[..gelesen]);
    }
    if hex::encode(hash.finalize()) != wahl.sha256 {
        anyhow::bail!("falsche Prüfsumme");
    }
    Ok(())
}

// ------------------------------------------------------------ Geräteseite

/// Abfrage des Geräts: die laufende Version (aus der Abfrage oder der letzten
/// Meldung) und das System, für das Pakete gefragt sind (Aufgabe 39).
#[derive(Debug, Deserialize)]
pub(crate) struct AngebotAbfrage {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    arch: Option<String>,
}

impl AngebotAbfrage {
    /// `Ok(None)`: keine Angabe. `Err`: eine Angabe, die es nicht gibt.
    fn system(&self) -> Result<Option<System>, FehlerAntwort> {
        crate::geraet::SystemAbfrage::neu(self.system.as_deref(), self.arch.as_deref()).lesen()
    }
}

/// `GET /api/v1/geraet/updates`: was der Server über die Veröffentlichung weiß.
///
/// Nennt das Gerät sein System, kommen die Pakete mit, die zu ihm gehören –
/// mit Größe und Prüfsumme, damit der Client vor jedem Schritt prüfen kann.
pub(crate) async fn angebot(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Query(abfrage): Query<AngebotAbfrage>,
) -> Ergebnis<Json<UpdateAngebot>> {
    let system = abfrage.system()?;
    let kennung = id.clone();
    let gemeldet = db(&zustand, move |conn| {
        Ok(speicher::hole_geraet(conn, kennung.als_str())?.and_then(|zeile| zeile.meldung))
    })
    .await?;
    let aktuelle = abfrage_version(&abfrage.version, gemeldet.as_ref());
    // Pakete nur, wenn der Server die Veröffentlichung und ihre Prüfsummen
    // kennt (aus dem Speicher, der Ablage oder – einmal – von der
    // Veröffentlichung). Fehlt etwas, ist das kein Fehler: dann bleibt es beim
    // Hinweis, und der Client bietet nichts an.
    let pakete = match system {
        Some(system) if zustand.konfig.updates.an => {
            let _ = sicherstellen(&zustand, false).await;
            pakete_fuer_system(&zustand, &system)
                .iter()
                .map(als_vertrag)
                .collect()
        }
        _ => Vec::new(),
    };
    let lage = crate::sperre(&zustand.updates);
    let (neueste_version, aenderungen) = lage
        .veroeffentlichung
        .as_ref()
        .map(|veroeffentlichung| {
            (
                Some(veroeffentlichung.version.clone()),
                veroeffentlichung.aenderungen.clone(),
            )
        })
        .unwrap_or((None, None));
    Ok(Json(UpdateAngebot {
        aktuelle_version: aktuelle,
        neueste_version,
        aenderungen,
        pakete,
    }))
}

/// Drossel je Gerät und Stunde für Paket-Abrufe (wie bei den Programmen).
fn gedrosselt(zustand: &Arc<Zustand>, id: &GeraetId, name: &str) -> Option<FehlerAntwort> {
    let jetzt = zustand.uhr.jetzt();
    let stunde = jetzt.unix_timestamp() / 3600;
    zustand
        .drossel
        .treffer(&format!("update:{id}:{name}:{stunde}"), jetzt)
        .map(|sekunden| {
            FehlerAntwort::neu(
                StatusCode::TOO_MANY_REQUESTS,
                format!("zu viele Abrufe; bitte {sekunden} Sekunden warten"),
            )
        })
}

/// `GET /api/v1/geraet/updates/paket/{name}?system=…&arch=…` (Aufgabe 39).
///
/// Nur mit Geräte-Token, nur für ein Paket, das zu **diesem** System gehört
/// und dessen Prüfsumme die Veröffentlichung nennt. Ausgeliefert wird
/// gestreamt, größenbegrenzt und gedrosselt; `ETag` und `If-None-Match` sparen
/// den zweiten Durchlauf.
pub(crate) async fn paket(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Pfad(name): Pfad<String>,
    Query(abfrage): Query<crate::geraet::SystemAbfrage>,
    kopf: HeaderMap,
) -> Ergebnis<Response> {
    let Some(system) = abfrage.lesen()? else {
        return Err(FehlerAntwort::ungueltig(
            "system und arch fehlen (windows|macos|linux, x64|arm64)",
        ));
    };
    if !ist_paketname(&name) {
        return Err(FehlerAntwort::nicht_gefunden(
            "so ein Paket verteilt dieser Server nicht",
        ));
    }
    sicherstellen(&zustand, false).await.map_err(|fehler| {
        FehlerAntwort::nicht_verfuegbar(format!("noch kein Paket zum Ausliefern: {fehler:#}"))
    })?;
    let Some(wahl) = pakete_fuer_system(&zustand, &system)
        .into_iter()
        .find(|wahl| wahl.name == name)
    else {
        // Ein Paket für ein anderes System ist keine Auskunft über dieses
        // Gerät: es wird nicht ausgeliefert und nicht verraten.
        return Err(FehlerAntwort::nicht_gefunden(
            "für dieses System gibt es das Paket nicht",
        ));
    };
    if let Some(fehler) = gedrosselt(&zustand, &id, &name) {
        return Err(fehler);
    }
    let etag = format!("\"{}\"", wahl.sha256);
    if kopf
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|wert| wert.to_str().ok())
        .is_some_and(|wert| wert.split(',').any(|teil| teil.trim() == etag))
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(axum::http::header::ETAG, etag)],
            "",
        )
            .into_response());
    }
    let version = {
        let lage = crate::sperre(&zustand.updates);
        lage.veroeffentlichung
            .as_ref()
            .map(|veroeffentlichung| veroeffentlichung.version.clone())
            .unwrap_or_default()
    };
    let datei = paket_bereitstellen(&zustand, &version, &wahl)
        .await
        .map_err(|fehler| {
            tracing::warn!(paket = %name, "Paket nicht abrufbar: {fehler:#}");
            FehlerAntwort::nicht_verfuegbar("das Paket ist gerade nicht abrufbar")
        })?;
    ausliefern(&datei, &wahl, etag).await
}

/// Ist der Name ein reiner Dateiname, den quiver verteilt?
fn ist_paketname(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.starts_with('.')
        && updates::PaketArt::aus_name(name).is_some()
}

/// Ein geprüftes Paket streamen.
async fn ausliefern(datei: &Path, wahl: &Paketwahl, etag: String) -> Ergebnis<Response> {
    let datei = tokio::fs::File::open(datei).await.map_err(|fehler| {
        tracing::warn!(paket = %wahl.name, "Paket nicht lesbar: {fehler}");
        FehlerAntwort::nicht_verfuegbar("das Paket ist nicht lesbar")
    })?;
    let strom = crate::geraet::datei_strom(datei, wahl.groesse);
    let antwort = Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
        .header(axum::http::header::CONTENT_LENGTH, wahl.groesse)
        .header(axum::http::header::ETAG, etag)
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from_stream(strom))
        .map_err(|fehler| FehlerAntwort::intern_mit_kennung(&fehler))?;
    Ok(antwort)
}

/// Die Version aus der Abfrage, sonst die zuletzt gemeldete.
fn abfrage_version(aus_abfrage: &str, gemeldet: Option<&quiver_protocol::Meldung>) -> String {
    let aus_abfrage = aus_abfrage.trim();
    if !aus_abfrage.is_empty() {
        return aus_abfrage.chars().take(64).collect();
    }
    gemeldet
        .map(|meldung| meldung.client_version.clone())
        .unwrap_or_default()
}

// ----------------------------------------------------------- Verwaltung

/// Die Geräte, deren gemeldeter Client älter ist als `neueste`.
///
/// Reine Rechnung über dem, was die Geräte gemeldet haben: eine unlesbare
/// Version ist nie älter (sie erscheint also nicht als veraltet), und die Liste
/// ist bei [`VERALTETE_MAX`] gedeckelt.
pub fn veraltete_aus(
    neueste: &str,
    gemeldet: impl IntoIterator<Item = (GeraetId, String, String)>,
) -> Vec<VeralteterClient> {
    gemeldet
        .into_iter()
        .filter(|(_, _, version)| updates::version_neuer(neueste, version))
        .take(VERALTETE_MAX)
        .map(|(geraet, name, version)| VeralteterClient {
            geraet,
            name,
            version,
        })
        .collect()
}

/// `GET /api/v1/admin/updates`: Stand der Abfrage und was daraus folgt.
pub(crate) async fn stand(State(zustand): State<Arc<Zustand>>) -> Ergebnis<Json<UpdateStand>> {
    let server_version = env!("CARGO_PKG_VERSION").to_string();
    let konfig = &zustand.konfig.updates;
    let (veroeffentlichung, zuletzt, mut fehler) = {
        let lage = crate::sperre(&zustand.updates);
        (
            lage.veroeffentlichung.clone(),
            lage.zuletzt.map(hilfen::als_text),
            lage.fehler.clone(),
        )
    };
    if !konfig.an {
        fehler = Some(String::from(
            "Die Update-Prüfung ist aus (QUIVER_UPDATES=aus).",
        ));
    }
    let neueste_version = veroeffentlichung.as_ref().map(|v| v.version.clone());
    let server_veraltet = neueste_version
        .as_deref()
        .is_some_and(|neueste| updates::version_neuer(neueste, &server_version));
    let veraltete_clients = match neueste_version.clone() {
        Some(neueste) => {
            let gemeldet = db(&zustand, |conn| {
                Ok(speicher::alle_geraete(conn)?
                    .into_iter()
                    .filter_map(|zeile| {
                        let meldung = zeile.meldung?;
                        Some((zeile.geraet.id, zeile.geraet.name, meldung.client_version))
                    })
                    .collect::<Vec<_>>())
            })
            .await?;
            veraltete_aus(&neueste, gemeldet)
        }
        None => Vec::new(),
    };
    Ok(Json(UpdateStand {
        server_version,
        abfrage_an: konfig.an,
        zuletzt_geprueft: zuletzt,
        neueste_version,
        aenderungen: veroeffentlichung
            .as_ref()
            .and_then(|v| v.aenderungen.clone()),
        server_veraltet,
        veraltete_clients,
        fehler,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::UPDATE_QUELLE;

    #[test]
    fn die_github_quelle_wird_zur_api() {
        assert_eq!(
            api_adresse(UPDATE_QUELLE),
            "https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest"
        );
        // Loopback (Tests) bleibt, wie es ist.
        assert_eq!(
            api_adresse("http://127.0.0.1:8080/veroeffentlichung"),
            "http://127.0.0.1:8080/veroeffentlichung/releases/latest"
        );
        assert_eq!(
            api_adresse("https://github.com/patimweb-labs/mcp-quiver/"),
            "https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest"
        );
    }

    #[test]
    fn die_abfrage_geht_vor_der_gemeldeten_version() {
        let meldung = quiver_protocol::Meldung {
            revision: String::from("r"),
            client_version: String::from("0.1.0"),
            eintraege: Vec::new(),
            programme: Vec::new(),
        };
        assert_eq!(abfrage_version(" 0.2.0 ", Some(&meldung)), "0.2.0");
        assert_eq!(abfrage_version("", Some(&meldung)), "0.1.0");
        assert_eq!(abfrage_version("", None), "");
    }

    #[test]
    fn nur_aeltere_clients_zaehlen_als_veraltet() {
        let geraete = vec![
            (
                GeraetId::neu("alt"),
                String::from("Rechner"),
                String::from("0.2.0"),
            ),
            (
                GeraetId::neu("neu"),
                String::from("Notebook"),
                String::from("0.3.0"),
            ),
            (
                GeraetId::neu("alt2"),
                String::from("Tablet"),
                String::from("0.2.9"),
            ),
            (GeraetId::neu("leer"), String::from("Kiosk"), String::new()),
        ];
        let veraltet = veraltete_aus("v0.3.0", geraete);
        let namen: Vec<&str> = veraltet.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(namen, vec!["Rechner", "Tablet"]);
        // Gleiche Version ist nicht veraltet.
        assert!(veraltete_aus(
            "0.3.0",
            vec![(
                GeraetId::neu("gleich"),
                String::from("Rechner"),
                String::from("0.3.0")
            )]
        )
        .is_empty());
    }

    #[test]
    fn die_liste_der_veralteten_ist_gedeckelt() {
        let viele = (0..VERALTETE_MAX + 10).map(|i| {
            (
                GeraetId::neu(format!("g{i}")),
                String::from("Rechner"),
                String::from("0.1.0"),
            )
        });
        assert_eq!(veraltete_aus("0.3.0", viele).len(), VERALTETE_MAX);
    }

    #[test]
    fn der_gemerkte_stand_ueberlebt_einen_neustart() {
        let veroeffentlichung = Veroeffentlichung {
            version: String::from("0.3.0"),
            veroeffentlicht: None,
            aenderungen: Some(String::from("https://example.test/changes")),
            dateien: Vec::new(),
        };
        let mut lage = Lage::neu();
        lage.aus_zeile(UpdateStandZeile {
            veroeffentlichung: Some(veroeffentlichung.clone()),
            zuletzt: Some(String::from("2026-03-01T10:00:00Z")),
        });
        assert_eq!(lage.veroeffentlichung, Some(veroeffentlichung));
        assert!(lage.zuletzt.is_some());
        // Und er lässt sich wieder als Zeile schreiben.
        let zeile = lage.als_zeile();
        assert_eq!(zeile.zuletzt.as_deref(), Some("2026-03-01T10:00:00Z"));
    }
}
