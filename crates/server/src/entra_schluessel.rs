// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Zertifikat statt Geheimnis (Aufgabe 23): der Server weist sich bei Entra
//! mit `private_key_jwt` aus (PS256, Kopf `x5t#S256`).
//!
//! Der private Schlüssel entsteht auf dem Server (RSA 3072) und verlässt ihn
//! nie; nach außen geht **nur** das selbst signierte Zertifikat als PEM, mit
//! Fingerabdruck und Ablauf. Erzeugen, Erneuern und Umschalten sind
//! Unterbefehle; vorhandene Schlüssel werden nie überschrieben.
//!
//! Keine eigene Kryptografie: Schlüssel und Signatur kommen aus `rsa`, das
//! X.509-Zertifikat aus `x509-cert`, der JWT aus `jsonwebtoken`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::Context;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::{Generate, PrivateKey};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use x509_cert::builder::{profile::cabf::Root as RootProfil, Builder, CertificateBuilder};
use x509_cert::der::{DecodePem, Encode, EncodePem};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::time::{Time, Validity};

/// Dateiname des aktiven privaten Schlüssels in `<daten>`.
pub const SCHLUESSEL_DATEI: &str = "entra-schluessel.pem";
/// So lange gilt ein neu erzeugtes Zertifikat.
pub const GUELTIG_TAGE: i64 = 365;
/// Ab so vielen Tagen vor dem Ablauf wird gewarnt.
pub const WARNUNG_TAGE: i64 = 30;
/// Gültigkeit einer Client-Assertion; die Aufgabe erlaubt höchstens 5 Minuten.
pub const ASSERTION_SEKUNDEN: i64 = 240;
/// Name im Zertifikat (allgemein, keine Firmendaten).
const ZERTIFIKAT_NAME: &str = "CN=quiver-server,O=mcp-quiver,C=DE";

/// Der private Schlüssel und sein öffentliches Zertifikat.
///
/// `Debug` gibt **nie** Schlüsselmaterial aus, auch nicht gekürzt.
pub struct EntraSchluessel {
    /// PKCS#8-PEM des privaten Schlüssels.
    schluessel_pem: String,
    /// PEM des selbst signierten Zertifikats (öffentlich).
    zertifikat_pem: String,
    /// DER des Zertifikats (für den Fingerabdruck).
    zertifikat_der: Vec<u8>,
    /// `x5t#S256`: base64url(SHA-256(DER)), wie Entra es erwartet.
    x5t_s256: String,
    /// Fingerabdruck in Hex mit Doppelpunkten (zum Vergleichen im Portal).
    fingerabdruck: String,
    nicht_vor: OffsetDateTime,
    nicht_nach: OffsetDateTime,
    /// Wo der private Schlüssel liegt.
    pfad: PathBuf,
}

impl std::fmt::Debug for EntraSchluessel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntraSchluessel")
            .field("pfad", &self.pfad)
            .field("nicht_nach", &self.nicht_nach)
            .field("fingerabdruck", &self.fingerabdruck)
            .field("schluessel", &"<verborgen>")
            .finish()
    }
}

impl EntraSchluessel {
    /// Schlüssel und Zertifikat von der Platte lesen und zusammen prüfen.
    pub fn laden(pfad: &Path) -> anyhow::Result<Self> {
        let schluessel_pem = std::fs::read_to_string(pfad)
            .with_context(|| format!("Entra-Schlüssel {} lesen", pfad.display()))?;
        Self::aus_pem(pfad, &schluessel_pem)
    }

    /// Wie [`EntraSchluessel::laden`], mit dem Schlüssel schon im Speicher.
    fn aus_pem(pfad: &Path, schluessel_pem: &str) -> anyhow::Result<Self> {
        let zertifikat_pfad = zertifikat_pfad(pfad);
        let zertifikat_pem = std::fs::read_to_string(&zertifikat_pfad).with_context(|| {
            format!(
                "Entra-Zertifikat {} lesen (gehört zum Schlüssel {})",
                zertifikat_pfad.display(),
                pfad.display()
            )
        })?;
        let zertifikat =
            x509_cert::Certificate::from_pem(zertifikat_pem.as_bytes()).with_context(|| {
                format!(
                    "Entra-Zertifikat {} ist kein PEM",
                    zertifikat_pfad.display()
                )
            })?;
        let zertifikat_der = zertifikat
            .to_der()
            .context("Entra-Zertifikat nicht als DER darstellbar")?;

        let privat = privat_aus_pem(schluessel_pem)?;
        // Der Schlüssel muss zum Zertifikat gehören – sonst scheitert jede
        // Anmeldung erst bei Entra und niemand weiß warum.
        let aus_zertifikat = zertifikat
            .tbs_certificate()
            .subject_public_key_info()
            .to_der()
            .context("öffentlicher Schlüssel des Zertifikats")?;
        let aus_privat = privat
            .to_public_key()
            .to_public_key_der()
            .context("öffentlicher Schlüssel des privaten Schlüssels")?;
        if aus_zertifikat != aus_privat.as_bytes() {
            anyhow::bail!(
                "Entra-Schlüssel {} und Zertifikat {} passen nicht zusammen",
                pfad.display(),
                zertifikat_pfad.display()
            );
        }

        let nicht_vor = zeit_aus_x509(&zertifikat.tbs_certificate().validity().not_before)?;
        let nicht_nach = zeit_aus_x509(&zertifikat.tbs_certificate().validity().not_after)?;
        let x5t_s256 = URL_SAFE_NO_PAD.encode(Sha256::digest(&zertifikat_der));
        let fingerabdruck = fingerabdruck_hex(&zertifikat_der);

        Ok(Self {
            schluessel_pem: schluessel_pem.to_string(),
            zertifikat_pem,
            zertifikat_der,
            x5t_s256,
            fingerabdruck,
            nicht_vor,
            nicht_nach,
            pfad: pfad.to_path_buf(),
        })
    }

    /// Das öffentliche Zertifikat als PEM – das Einzige, was den Server verlässt.
    pub fn zertifikat_pem(&self) -> &str {
        &self.zertifikat_pem
    }

    /// Das Zertifikat als DER (öffentlich, z. B. für den Fingerabdruck).
    pub fn zertifikat_der(&self) -> &[u8] {
        &self.zertifikat_der
    }

    /// `x5t#S256`, wie es im Assertion-Kopf steht.
    pub fn x5t_s256(&self) -> &str {
        &self.x5t_s256
    }

    /// SHA-256 des Zertifikats in Hex, mit Doppelpunkten.
    pub fn fingerabdruck(&self) -> &str {
        &self.fingerabdruck
    }

    pub fn nicht_vor(&self) -> OffsetDateTime {
        self.nicht_vor
    }

    pub fn nicht_nach(&self) -> OffsetDateTime {
        self.nicht_nach
    }

    pub fn pfad(&self) -> &Path {
        &self.pfad
    }

    /// Wie viele Tage das Zertifikat noch gilt (negativ: schon abgelaufen).
    pub fn tage_uebrig(&self, jetzt: OffsetDateTime) -> i64 {
        (self.nicht_nach - jetzt).whole_days()
    }

    /// Der Stand für Verwaltung und Prüfspur.
    pub fn stand(&self, jetzt: OffsetDateTime) -> ZertifikatStand {
        let tage_uebrig = self.tage_uebrig(jetzt);
        let ablauf = self
            .nicht_nach
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| String::from("unbekannt"));
        ZertifikatStand {
            ablauf,
            tage_uebrig,
            warnung: tage_uebrig <= WARNUNG_TAGE,
            fingerabdruck: self.fingerabdruck.clone(),
        }
    }

    /// Text der Warnung, wenn das Zertifikat bald abläuft (oder abgelaufen ist).
    pub fn warnungstext(&self, jetzt: OffsetDateTime) -> Option<String> {
        let tage = self.tage_uebrig(jetzt);
        if tage > WARNUNG_TAGE {
            return None;
        }
        Some(if tage < 0 {
            format!(
                "Das Entra-Zertifikat ist seit {} Tagen abgelaufen ({}). \
                 Bitte `quiver-server entra-schluessel --erneuern` und dann `--umschalten`.",
                -tage, self.fingerabdruck
            )
        } else {
            format!(
                "Das Entra-Zertifikat gilt noch {tage} Tage (bis {}, {}). \
                 Bitte `quiver-server entra-schluessel --erneuern` und dann `--umschalten`.",
                self.nicht_nach.date(),
                self.fingerabdruck
            )
        })
    }

    /// Eine Client-Assertion für den Token-Endpunkt (`private_key_jwt`).
    ///
    /// Je Anfrage neu: `jti` ist zufällig, `nbf`/`iat` sind jetzt, `exp`
    /// höchstens fünf Minuten später, `aud` ist der Token-Endpunkt aus der
    /// Discovery, `iss` = `sub` = Client-Id.
    pub fn assertion(
        &self,
        client_id: &str,
        token_endpoint: &str,
        jetzt: OffsetDateTime,
    ) -> anyhow::Result<String> {
        let mut kopf = Header::new(Algorithm::PS256);
        kopf.typ = Some(String::from("JWT"));
        kopf.x5t_s256 = Some(self.x5t_s256.clone());
        let iat = jetzt.unix_timestamp();
        let claims = serde_json::json!({
            "aud": token_endpoint,
            "iss": client_id,
            "sub": client_id,
            "jti": crate::krypto::zufall_b64(16),
            "nbf": iat,
            "iat": iat,
            "exp": iat + ASSERTION_SEKUNDEN,
        });
        let schluessel = EncodingKey::from_rsa_pem(self.schluessel_pem.as_bytes())
            .context("Entra-Schlüssel nicht als Signierschlüssel lesbar")?;
        encode(&kopf, &claims, &schluessel).context("Client-Assertion nicht signierbar")
    }
}

/// Schreibt die Warnung höchstens einmal je Tag in Log und Prüfspur
/// (Aufgabe 23). Ohne Schlüssel oder ohne Warnung passiert nichts.
pub(crate) async fn tageshinweis(zustand: &Arc<crate::zustand::Zustand>) {
    let Some(oidc) = zustand.oidc.as_ref() else {
        return;
    };
    let crate::config::Ausweis::Schluessel(halter) = &oidc.konfig.ausweis else {
        return;
    };
    let schluessel = halter.jetzt();
    let jetzt = zustand.uhr.jetzt();
    let Some(text) = schluessel.warnungstext(jetzt) else {
        return;
    };
    let tag = jetzt.date().to_string();
    let schreiben = crate::db(zustand, {
        let tag = tag.clone();
        move |conn| {
            Ok(!crate::speicher::pruefspur_hat_am_tag(
                conn,
                "hinweis",
                &tag,
                "zertifikat",
            )?)
        }
    })
    .await;
    if !matches!(schreiben, Ok(true)) {
        return;
    }
    let eintrag = format!("zertifikat: {text}");
    let _ = crate::db(zustand, move |conn| {
        crate::speicher::pruefspur_schreiben(conn, jetzt, "hinweis", "server", &eintrag)
    })
    .await;
    tracing::warn!("{text}");
}

/// Der Stand des Zertifikats, wie ihn die Verwaltung zeigt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZertifikatStand {
    /// RFC 3339.
    pub ablauf: String,
    pub tage_uebrig: i64,
    pub warnung: bool,
    pub fingerabdruck: String,
}

/// Hält den aktiven Schlüssel und lädt ihn neu, wenn die Datei sich ändert.
///
/// So wirkt `entra-schluessel --umschalten` ohne Neustart und ohne Ausfall:
/// beide Schlüssel sind bei Entra hinterlegt, der neue gilt ab dem nächsten
/// Aufruf.
pub struct SchluesselHalter {
    pfad: PathBuf,
    innen: Mutex<HalterStand>,
}

struct HalterStand {
    geaendert: Option<SystemTime>,
    schluessel: Arc<EntraSchluessel>,
}

impl std::fmt::Debug for SchluesselHalter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchluesselHalter")
            .field("pfad", &self.pfad)
            .finish()
    }
}

impl SchluesselHalter {
    pub fn laden(pfad: PathBuf) -> anyhow::Result<Self> {
        let schluessel = Arc::new(EntraSchluessel::laden(&pfad)?);
        let geaendert = geaendert(&pfad);
        Ok(Self {
            pfad,
            innen: Mutex::new(HalterStand {
                geaendert,
                schluessel,
            }),
        })
    }

    pub fn pfad(&self) -> &Path {
        &self.pfad
    }

    /// Der aktuelle Schlüssel; nach einem Wechsel der neue.
    pub fn jetzt(&self) -> Arc<EntraSchluessel> {
        let neu = geaendert(&self.pfad);
        let mut stand = crate::sperre(&self.innen);
        if neu != stand.geaendert {
            match EntraSchluessel::laden(&self.pfad) {
                Ok(frisch) => {
                    stand.geaendert = neu;
                    stand.schluessel = Arc::new(frisch);
                }
                Err(fehler) => {
                    // Der alte Schlüssel gilt weiter; ein halb geschriebener
                    // Wechsel darf den Server nicht lahmlegen.
                    tracing::warn!(
                        "Entra-Schlüssel {} nicht neu lesbar ({fehler}); der bisherige gilt weiter",
                        self.pfad.display()
                    );
                }
            }
        }
        stand.schluessel.clone()
    }
}

fn geaendert(pfad: &Path) -> Option<SystemTime> {
    std::fs::metadata(pfad)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

/// Wo das aktive Zertifikat zu einem Schlüssel liegt.
pub fn zertifikat_pfad(schluessel: &Path) -> PathBuf {
    let stamm = schluessel
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    schluessel.with_file_name(format!("{stamm}.crt.pem"))
}

/// Der Pfad des aktiven Schlüssels: `QUIVER_ENTRA_SCHLUESSEL_DATEI` oder
/// `<daten>/entra-schluessel.pem`.
pub fn pfad_aus_umgebung(daten: &Path) -> PathBuf {
    std::env::var_os("QUIVER_ENTRA_SCHLUESSEL_DATEI")
        .map(PathBuf::from)
        .unwrap_or_else(|| daten.join(SCHLUESSEL_DATEI))
}

fn privat_aus_pem(pem: &str) -> anyhow::Result<PrivateKey> {
    PrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| PrivateKey::from_pkcs1_pem(pem))
        .context("privater Entra-Schlüssel ist kein PEM (PKCS#8 oder PKCS#1)")
}

fn zeit_aus_x509(zeit: &Time) -> anyhow::Result<OffsetDateTime> {
    let sekunden = zeit.to_unix_duration().as_secs();
    OffsetDateTime::from_unix_timestamp(i64::try_from(sekunden).unwrap_or(i64::MAX))
        .context("Zeitpunkt im Zertifikat liegt außerhalb des darstellbaren Bereichs")
}

fn x509_zeit(zeit: OffsetDateTime) -> anyhow::Result<Time> {
    let ohne_nanos = zeit
        .replace_nanosecond(0)
        .context("Zeitpunkt nicht darstellbar")?;
    let text = ohne_nanos
        .format(&time::format_description::well_known::Rfc3339)
        .context("Zeitpunkt nicht als RFC 3339 darstellbar")?;
    text.parse::<Time>()
        .with_context(|| format!("{text} ist keine X.509-Zeit"))
}

fn fingerabdruck_hex(der: &[u8]) -> String {
    let roh = hex::encode_upper(Sha256::digest(der));
    roh.as_bytes()
        .chunks(2)
        .map(|paar| String::from_utf8_lossy(paar).to_string())
        .collect::<Vec<_>>()
        .join(":")
}

/// Ein neues Schlüsselpaar erzeugen und ablegen. Vorhandenes wird **nie**
/// überschrieben: die Dateien entstehen mit `create_new`.
pub fn erzeugen(pfad: &Path, jetzt: OffsetDateTime, tage: i64) -> anyhow::Result<EntraSchluessel> {
    let zertifikat = zertifikat_pfad(pfad);
    for datei in [pfad, zertifikat.as_path()] {
        if datei.exists() {
            anyhow::bail!(
                "{} gibt es schon; vorhandene Schlüssel werden nicht überschrieben \
                 (`entra-schluessel --erneuern` legt einen zweiten an, \
                 `--umschalten` macht ihn aktiv)",
                datei.display()
            );
        }
    }
    if let Some(ordner) = pfad.parent() {
        std::fs::create_dir_all(ordner)
            .with_context(|| format!("Ordner {} anlegen", ordner.display()))?;
    }

    let privat = PrivateKey::generate();
    let schluessel_pem = privat
        .to_pkcs8_pem(LineEnding::LF)
        .context("privaten Schlüssel als PEM schreiben")?;
    let subject = Name::from_str(ZERTIFIKAT_NAME)
        .with_context(|| format!("{ZERTIFIKAT_NAME} ist kein gültiger X.509-Name"))?;
    let profil = RootProfil::new(false, subject).context("Zertifikatsprofil")?;
    let nicht_vor = x509_zeit(jetzt)?;
    let nicht_nach = x509_zeit(jetzt + time::Duration::days(tage))?;
    let spki = SubjectPublicKeyInfoOwned::from_key(&privat.to_public_key())
        .context("öffentlichen Schlüssel einbetten")?;
    let serial = SerialNumber::from(serial_zahl());
    let bauer = CertificateBuilder::new(profil, serial, Validity::new(nicht_vor, nicht_nach), spki)
        .context("Zertifikat vorbereiten")?;
    let signierer = rsa::pkcs1v15::SigningKey::<rsa::sha2::Sha256>::new(privat);
    let zertifikat_der = bauer
        .build(&signierer)
        .context("selbst signiertes Zertifikat bauen")?;
    let zertifikat_pem = zertifikat_der
        .to_pem(LineEnding::LF)
        .context("Zertifikat als PEM schreiben")?;

    // Zuerst der private Schlüssel (0600, ohne Zeitfenster mit offenen
    // Rechten), dann das öffentliche Zertifikat.
    private_schreiben(pfad, schluessel_pem.as_bytes())?;
    if let Err(fehler) = std::fs::write(&zertifikat, &zertifikat_pem) {
        let _ = std::fs::remove_file(pfad);
        return Err(fehler)
            .with_context(|| format!("Zertifikat {} schreiben", zertifikat.display()));
    }
    EntraSchluessel::laden(pfad)
}

/// Schreibt den privaten Schlüssel mit 0600 und `create_new` (nie überschreiben).
fn private_schreiben(pfad: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let mut optionen = std::fs::OpenOptions::new();
    optionen.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        optionen.mode(0o600);
    }
    let mut datei = optionen
        .open(pfad)
        .with_context(|| format!("privaten Schlüssel {} anlegen", pfad.display()))?;
    datei
        .write_all(bytes)
        .with_context(|| format!("privaten Schlüssel {} schreiben", pfad.display()))?;
    datei.flush()?;
    Ok(())
}

/// Eine zufällige, positive Seriennummer (<= 20 Byte, kein gesetztes MSB).
fn serial_zahl() -> u64 {
    let mut roh = [0u8; 8];
    roh.copy_from_slice(&crate::krypto::zufall(8));
    (u64::from_be_bytes(roh) >> 1).max(1)
}

/// Die belegten Zusatzplätze (`entra-schluessel-<n>.pem`) zu einem aktiven Pfad.
pub fn vorhandene_plaetze(aktiv: &Path) -> Vec<(u32, PathBuf)> {
    let Some(ordner) = aktiv.parent() else {
        return Vec::new();
    };
    let stamm = aktiv
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut plaetze = BTreeSet::new();
    if let Ok(eintraege) = std::fs::read_dir(ordner) {
        for eintrag in eintraege.flatten() {
            let name = eintrag.file_name().to_string_lossy().to_string();
            let rest = name.strip_prefix(&format!("{stamm}-"));
            let Some(rest) = rest else { continue };
            let Some(nummer) = rest.strip_suffix(".pem") else {
                continue;
            };
            if nummer.contains('.') {
                continue;
            }
            if let Ok(n) = nummer.parse::<u32>() {
                if n > 0 {
                    plaetze.insert(n);
                }
            }
        }
    }
    plaetze
        .into_iter()
        .map(|n| (n, pfad_fuer_platz(aktiv, n)))
        .collect()
}

/// `<stamm>-<n>.pem` neben dem aktiven Schlüssel.
pub fn pfad_fuer_platz(aktiv: &Path, nummer: u32) -> PathBuf {
    let stamm = aktiv
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    aktiv.with_file_name(format!("{stamm}-{nummer}.pem"))
}

/// Den nächsten freien Zusatzplatz wählen.
pub fn naechster_platz(aktiv: &Path) -> PathBuf {
    let belegt: BTreeSet<u32> = vorhandene_plaetze(aktiv)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let mut nummer = 1;
    while belegt.contains(&nummer) {
        nummer += 1;
    }
    pfad_fuer_platz(aktiv, nummer)
}

/// Einen zweiten Schlüssel anlegen (wird nicht aktiv).
pub fn erneuern(aktiv: &Path, jetzt: OffsetDateTime) -> anyhow::Result<(PathBuf, EntraSchluessel)> {
    let ziel = naechster_platz(aktiv);
    let schluessel = erzeugen(&ziel, jetzt, GUELTIG_TAGE)?;
    Ok((ziel, schluessel))
}

/// Den neuesten zweiten Schlüssel aktiv machen – ohne Ausfall und ohne
/// Überschreiben: der bisherige aktive wandert auf einen freien Platz.
pub fn umschalten(aktiv: &Path) -> anyhow::Result<EntraSchluessel> {
    let plaetze = vorhandene_plaetze(aktiv);
    let Some((_, neu)) = plaetze.last() else {
        anyhow::bail!(
            "kein zweiter Schlüssel da; erst `quiver-server entra-schluessel --erneuern`"
        );
    };
    if !neu.exists() {
        anyhow::bail!("{} fehlt", neu.display());
    }
    let neu_zertifikat = zertifikat_pfad(neu);
    if !neu_zertifikat.exists() {
        anyhow::bail!("{} fehlt", neu_zertifikat.display());
    }
    if aktiv.exists() {
        let ziel = naechster_platz(aktiv);
        let alt_zertifikat = zertifikat_pfad(aktiv);
        std::fs::rename(aktiv, &ziel)
            .with_context(|| format!("{} nach {} verschieben", aktiv.display(), ziel.display()))?;
        // Das Zertifikat gehört zum alten Schlüssel und wandert mit.
        if alt_zertifikat.exists() {
            std::fs::rename(&alt_zertifikat, zertifikat_pfad(&ziel)).with_context(|| {
                format!(
                    "{} nach {} verschieben",
                    alt_zertifikat.display(),
                    zertifikat_pfad(&ziel).display()
                )
            })?;
        }
    }
    std::fs::rename(neu, aktiv)
        .with_context(|| format!("{} nach {} verschieben", neu.display(), aktiv.display()))?;
    std::fs::rename(&neu_zertifikat, zertifikat_pfad(aktiv)).with_context(|| {
        format!(
            "{} nach {} verschieben",
            neu_zertifikat.display(),
            zertifikat_pfad(aktiv).display()
        )
    })?;
    EntraSchluessel::laden(aktiv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode_header, DecodingKey, Validation};

    fn ordner(name: &str) -> PathBuf {
        let pfad = std::env::temp_dir().join(format!(
            "quiver-entra-schluessel-{name}-{}-{}",
            std::process::id(),
            crate::krypto::zufall_b64(8)
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    fn jetzt() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
    }

    #[test]
    fn schluessel_und_zertifikat_entstehen_und_passen_zusammen() {
        let ordner = ordner("erzeugen");
        let pfad = ordner.join(SCHLUESSEL_DATEI);
        let schluessel = erzeugen(&pfad, jetzt(), GUELTIG_TAGE).expect("erzeugt");
        assert!(pfad.exists());
        assert!(zertifikat_pfad(&pfad).exists());
        assert!(schluessel
            .zertifikat_pem()
            .starts_with("-----BEGIN CERTIFICATE-----"));
        assert_eq!(schluessel.x5t_s256().len(), 43);
        assert_eq!(
            schluessel.x5t_s256(),
            URL_SAFE_NO_PAD.encode(Sha256::digest(schluessel.zertifikat_der()))
        );
        assert!(schluessel.fingerabdruck().contains(':'));
        assert_eq!(
            schluessel.nicht_nach(),
            jetzt() + time::Duration::days(GUELTIG_TAGE)
        );
        // Das Debug enthält kein Schlüsselmaterial.
        let debug = format!("{schluessel:?}");
        assert!(!debug.contains("PRIVATE KEY"), "{debug}");
        assert!(!debug.contains(&schluessel.schluessel_pem), "{debug}");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[cfg(unix)]
    #[test]
    fn privater_schluessel_hat_0600_und_wird_nie_ueberschrieben() {
        use std::os::unix::fs::PermissionsExt;

        let ordner = ordner("rechte");
        let pfad = ordner.join(SCHLUESSEL_DATEI);
        let erst = erzeugen(&pfad, jetzt(), GUELTIG_TAGE).expect("erzeugt");
        let mode = std::fs::metadata(&pfad).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "private key is too open: {mode:o}");
        let vorher = std::fs::read_to_string(&pfad).unwrap();
        // Zweiter Versuch: klare Meldung, keine Änderung.
        let fehler = erzeugen(&pfad, jetzt(), GUELTIG_TAGE).expect_err("nicht überschreiben");
        assert!(format!("{fehler}").contains("gibt es schon"), "{fehler}");
        assert_eq!(std::fs::read_to_string(&pfad).unwrap(), vorher);
        assert_eq!(
            erst.fingerabdruck(),
            EntraSchluessel::laden(&pfad).unwrap().fingerabdruck()
        );
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn assertion_hat_alle_felder_und_ist_ps256() {
        let ordner = ordner("assertion");
        let pfad = ordner.join(SCHLUESSEL_DATEI);
        let schluessel = erzeugen(&pfad, jetzt(), GUELTIG_TAGE).expect("erzeugt");
        let token = schluessel
            .assertion("client-1", "https://login.example.test/token", jetzt())
            .expect("Assertion");
        let kopf = decode_header(&token).unwrap();
        assert_eq!(kopf.alg, Algorithm::PS256);
        assert_eq!(kopf.x5t_s256.as_deref(), Some(schluessel.x5t_s256()));
        // Signatur gegen das Zertifikat prüfen.
        let zert =
            x509_cert::Certificate::from_pem(schluessel.zertifikat_pem().as_bytes()).unwrap();
        let spki = zert.tbs_certificate().subject_public_key_info();
        let oeffentlich = spki
            .subject_public_key
            .as_bytes()
            .ok_or("kein Schlüssel")
            .unwrap();
        let public =
            <rsa::PublicKey as rsa::pkcs1::DecodeRsaPublicKey>::from_pkcs1_der(oeffentlich)
                .unwrap();
        let pem = public
            .to_public_key_pem(LineEnding::LF)
            .expect("öffentlich als PEM");
        let mut pruefung = Validation::new(Algorithm::PS256);
        pruefung.set_audience(&["https://login.example.test/token"]);
        pruefung.set_issuer(&["client-1"]);
        let daten = jsonwebtoken::decode::<serde_json::Value>(
            &token,
            &DecodingKey::from_rsa_pem(pem.as_bytes()).unwrap(),
            &pruefung,
        )
        .expect("Signatur gilt");
        let claims = daten.claims;
        assert_eq!(claims["sub"], "client-1");
        assert_eq!(claims["iss"], "client-1");
        assert!(claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap() <= 300);
        // Je Anfrage eine neue `jti`.
        let zweite = schluessel
            .assertion("client-1", "https://login.example.test/token", jetzt())
            .unwrap();
        let andere: serde_json::Value =
            jsonwebtoken::dangerous::insecure_decode::<serde_json::Value>(&zweite)
                .unwrap()
                .claims;
        assert_ne!(claims["jti"], andere["jti"]);
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn warnung_ab_dreissig_tagen_und_ablauf() {
        let ordner = ordner("warnung");
        let pfad = ordner.join(SCHLUESSEL_DATEI);
        let schluessel = erzeugen(&pfad, jetzt(), GUELTIG_TAGE).expect("erzeugt");
        assert!(schluessel.warnungstext(jetzt()).is_none());
        let knapp = jetzt() + time::Duration::days(GUELTIG_TAGE - WARNUNG_TAGE);
        let text = schluessel.warnungstext(knapp).expect("Warnung");
        assert!(text.contains("30 Tage"), "{text}");
        let abgelaufen = jetzt() + time::Duration::days(GUELTIG_TAGE + 3);
        let text = schluessel.warnungstext(abgelaufen).expect("Warnung");
        assert!(text.contains("abgelaufen"), "{text}");
        assert!(schluessel.stand(abgelaufen).warnung);
        assert!(schluessel.stand(abgelaufen).tage_uebrig < 0);
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn erneuern_und_umschalten_ohne_ueberschreiben() {
        let ordner = ordner("wechsel");
        let aktiv = ordner.join(SCHLUESSEL_DATEI);
        let erst = erzeugen(&aktiv, jetzt(), GUELTIG_TAGE).expect("erzeugt");
        let (zweiter_pfad, zweiter) = erneuern(&aktiv, jetzt()).expect("erneuert");
        assert!(zweiter_pfad.exists());
        assert_ne!(erst.fingerabdruck(), zweiter.fingerabdruck());
        // Der aktive Schlüssel ist unverändert.
        assert_eq!(
            EntraSchluessel::laden(&aktiv).unwrap().fingerabdruck(),
            erst.fingerabdruck()
        );
        let umgeschaltet = umschalten(&aktiv).expect("umgeschaltet");
        assert_eq!(umgeschaltet.fingerabdruck(), zweiter.fingerabdruck());
        // Der alte Schlüssel liegt weiter (auf einem freien Platz), nichts ist weg.
        let alle = vorhandene_plaetze(&aktiv);
        assert!(alle.iter().any(|(_, pfad)| pfad.exists()));
        assert_eq!(
            EntraSchluessel::laden(&alle[0].1).unwrap().fingerabdruck(),
            erst.fingerabdruck()
        );
        // Zurückschalten geht, solange der alte Schlüssel da ist.
        let zurueck = umschalten(&aktiv).expect("zurückgeschaltet");
        assert_eq!(zurueck.fingerabdruck(), erst.fingerabdruck());
        // Ohne zweiten Schlüssel ist Umschalten ein klarer Fehler.
        for (_, pfad) in vorhandene_plaetze(&aktiv) {
            let _ = std::fs::remove_file(&pfad);
            let _ = std::fs::remove_file(zertifikat_pfad(&pfad));
        }
        assert!(umschalten(&aktiv).is_err());
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn halter_laedt_nach_dem_wechsel_neu() {
        let ordner = ordner("halter");
        let aktiv = ordner.join(SCHLUESSEL_DATEI);
        let erst = erzeugen(&aktiv, jetzt(), GUELTIG_TAGE).expect("erzeugt");
        let halter = SchluesselHalter::laden(aktiv.clone()).expect("Halter");
        assert_eq!(halter.jetzt().fingerabdruck(), erst.fingerabdruck());
        let (_, zweiter) = erneuern(&aktiv, jetzt()).expect("erneuert");
        umschalten(&aktiv).expect("umgeschaltet");
        // mtime kann in derselben Sekunde liegen; deshalb den Wechsel erzwingen.
        assert_eq!(halter.jetzt().fingerabdruck(), zweiter.fingerabdruck());
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn schluessel_und_zertifikat_muessen_zusammenpassen() {
        let ordner = ordner("fremd");
        let pfad = ordner.join(SCHLUESSEL_DATEI);
        let schluessel = erzeugen(&pfad, jetzt(), GUELTIG_TAGE).expect("erzeugt");
        let (zweiter, _) = erneuern(&pfad, jetzt()).expect("erneuert");
        // Schlüssel und Zertifikat vertauschen: passt nicht zusammen.
        let fremd = ordner.join("fremd.pem");
        std::fs::copy(&zweiter, &fremd).unwrap();
        std::fs::copy(zertifikat_pfad(&pfad), zertifikat_pfad(&fremd)).unwrap();
        let fehler = EntraSchluessel::laden(&fremd).expect_err("passt nicht");
        assert!(
            format!("{fehler}").contains("passen nicht zusammen"),
            "{fehler}"
        );
        assert_eq!(
            EntraSchluessel::laden(&pfad).unwrap().fingerabdruck(),
            schluessel.fingerabdruck()
        );
        let _ = std::fs::remove_dir_all(&ordner);
    }
}
