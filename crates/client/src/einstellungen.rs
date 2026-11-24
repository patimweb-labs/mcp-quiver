// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Einstellungen der Tools sichern und auf einem anderen Gerät einspielen
//! (Aufgabe 15).
//!
//! Gesichert werden die Werte je Eintrag (`werte`), die Zugangslisten
//! (`zugaenge`, Aufgabe 14) und – **nur mit Passwort** – die Geheimnisse aus
//! dem Tresor. Nicht dabei sind das Geräte-Token, Agent-Tokens und die
//! Anmeldungen der Tools selbst (Refresh-Tokens von m365/zep): die gehören zum
//! Gerät bzw. zur Anmeldung.
//!
//! Aufbau der Datei `quiver-einstellungen-<datum>.json`:
//!
//! ```json
//! { "format": "mcp-quiver/einstellungen", "version": 1, "erstellt": "…",
//!   "eintraege": { "<id>": { "werte": {…}, "zugaenge": { "<name>": {…} } } },
//!   "geheim": null }
//! ```
//!
//! Mit Passwort steht in `geheim` statt `null` ein Umschlag
//! (`kdf`, `salz`, `nonce`, `daten`). Der Schlüssel kommt aus Argon2id
//! (m = 64 MiB, t = 3, p = 1) mit einem zufälligen Salz von 16 Byte;
//! verschlüsselt wird mit XChaCha20-Poly1305 – wie in
//! [`crate::dateitresor`]. Klartext-Geheimnisse stehen nie in der Datei, im
//! Log oder in einer Fehlermeldung.
//!
//! Was hier steht, ist **reine Datenarbeit**: die Datei lesen und schreiben,
//! mischen und vorhersagen, was ein Import täte. Der Kern
//! ([`crate::Kern`](crate::Kern)) holt die Werte aus seinem Stand und schreibt
//! sie zurück.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use quiver_domain::{EintragId, Soll, Werte};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Kennung des Formats in der Datei.
pub const FORMAT: &str = "mcp-quiver/einstellungen";
/// Die Fassung dieses Formats.
pub const VERSION: u32 = 1;
/// So kurz darf das Passwort nicht sein.
pub const PASSWORT_MIN: usize = 8;
/// Salz des Schlüssels (Byte).
const SALZ_LEN: usize = 16;
/// Nonce von XChaCha20-Poly1305 (Byte).
const NONCE_LEN: usize = 24;
/// Länge des abgeleiteten Schlüssels (Byte).
const SCHLUESSEL_LEN: usize = 32;
/// Speicher des Schlüssel-Ableiters in KiB (64 MiB).
const ARGON2_MEMORIE: u32 = 64 * 1024;
/// Durchläufe des Schlüssel-Ableiters.
const ARGON2_RUNDEN: u32 = 3;

// ---------------------------------------------------------------- Ergebnis

/// Was ein Export oder Import getan hat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Zusammenfassung {
    /// Wie viele Einträge geschrieben wurden.
    pub eintraege: usize,
    /// Wie viele offene Werte.
    pub werte: usize,
    /// Wie viele Zugänge.
    pub zugaenge: usize,
    /// Wie viele Geheimnisse (nur mit Passwort).
    pub geheimnisse: usize,
    /// Einträge, die es auf diesem Gerät nicht (mehr) gibt.
    pub uebersprungen: Vec<String>,
}

impl Zusammenfassung {
    /// Sind Geheimnisse mitgekommen (oder mitgegangen)?
    pub fn mit_geheimnissen(&self) -> bool {
        self.geheimnisse > 0
    }
}

/// Was ein Import mit einem Eintrag täte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VorschauArt {
    /// Auf dem Gerät steht zu diesem Eintrag noch nichts.
    Neu,
    /// Der Eintrag ist da; es kommt etwas hinzu oder ändert sich.
    Aendert,
    /// Diesen Eintrag gibt es hier nicht (nicht im Soll) – er wird
    /// übersprungen und bleibt in der Datei.
    Uebersprungen,
}

/// Eine Zeile der Vorschau.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VorschauEintrag {
    /// Die Kennung des Eintrags (so steht sie in der Datei).
    pub id: String,
    /// Der Titel, wenn das Gerät den Eintrag kennt – sonst die Kennung.
    pub titel: String,
    pub art: VorschauArt,
    /// Werte, die es hier noch nicht gibt.
    pub neue: usize,
    /// Werte, die hier anders stehen.
    pub geaenderte: usize,
    /// Zugänge in der Datei.
    pub zugaenge: usize,
    /// Ist der Eintrag danach auf diesem Gerät abgeschaltet (Aufgabe 19)?
    /// `true`, wenn die Datei „aus“ sagt oder das Gerät ihn schon aus hat.
    pub am_geraet_aus: bool,
}

/// Was ein Import täte – je Eintrag und insgesamt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vorschau {
    pub eintraege: Vec<VorschauEintrag>,
    /// Die Datei führt Geheimnisse: ohne Passwort geht der Import nicht.
    pub hat_geheimnisse: bool,
}

impl Vorschau {
    /// Wie viele Einträge übernommen würden.
    pub fn uebernommen(&self) -> usize {
        self.eintraege
            .iter()
            .filter(|eintrag| eintrag.art != VorschauArt::Uebersprungen)
            .count()
    }

    /// Wie viele Einträge übersprungen würden.
    pub fn uebersprungen(&self) -> usize {
        self.eintraege
            .iter()
            .filter(|eintrag| eintrag.art == VorschauArt::Uebersprungen)
            .count()
    }
}

// -------------------------------------------------------------- Die Datei

/// Die offenen Angaben eines Eintrags in der Datei.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EintragTeil {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub werte: Werte,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zugaenge: BTreeMap<String, Werte>,
    /// War der Eintrag auf dem sichernden Gerät abgeschaltet (Aufgabe 19)?
    /// Die Wahl geht mit und wird beim Einspielen **nicht** aufgehoben: aus
    /// bleibt aus.
    #[serde(default, skip_serializing_if = "ist_falsch")]
    pub am_geraet_aus: bool,
}

/// Nur schreiben, wenn die Wahl „abgeschaltet“ gilt.
fn ist_falsch(wert: &bool) -> bool {
    !*wert
}

/// Die Geheimnisse, wie sie (verschlüsselt) in der Datei stehen: die Werte der
/// Einträge und die ihrer Zugänge – dieselben Schlüssel wie im Tresor, nur ohne
/// das Präfix.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geheimnisse {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub eintraege: BTreeMap<String, Werte>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zugaenge: BTreeMap<String, BTreeMap<String, Werte>>,
}

impl Geheimnisse {
    /// Steckt etwas drin?
    pub fn ist_leer(&self) -> bool {
        self.eintraege.values().all(Werte::is_empty)
            && self
                .zugaenge
                .values()
                .all(|liste| liste.values().all(Werte::is_empty))
    }

    /// Wie viele Geheimnisse es sind.
    pub fn anzahl(&self) -> usize {
        self.eintraege.values().map(Werte::len).sum::<usize>()
            + self
                .zugaenge
                .values()
                .flat_map(|liste| liste.values())
                .map(Werte::len)
                .sum::<usize>()
    }
}

/// Der Umschlag um die Geheimnisse.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Umschlag {
    kdf: String,
    salz: String,
    nonce: String,
    daten: String,
}

/// Die Datei, wie sie auf der Platte steht.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Huelle {
    format: String,
    version: u32,
    erstellt: String,
    #[serde(default)]
    eintraege: BTreeMap<String, EintragTeil>,
    #[serde(default)]
    geheim: Option<Umschlag>,
}

/// Eine gelesene Datei. `geheimnisse` ist nur gefüllt, wenn ein Passwort
/// mitgegeben wurde (und die Datei eines führt).
#[derive(Debug, Clone)]
pub struct Gelesen {
    pub eintraege: BTreeMap<String, EintragTeil>,
    pub geheimnisse: Option<Geheimnisse>,
    /// Die Datei führt Geheimnisse – auch wenn sie (noch) nicht entschlüsselt
    /// sind.
    pub hat_geheimnisse: bool,
}

// ------------------------------------------------------- Schreiben, Lesen

/// Die Angaben als Datei schreiben (Rechte 0600).
///
/// `geheimnisse` landet nur mit `passwort` in der Datei; ohne Passwort steht
/// `"geheim": null` und es ist keine Spur eines Geheimnisses im Text.
pub fn schreiben(
    pfad: &Path,
    eintraege: BTreeMap<String, EintragTeil>,
    geheimnisse: Option<&Geheimnisse>,
    passwort: Option<&str>,
) -> anyhow::Result<Zusammenfassung> {
    let passwort = passwort.map(str::trim).filter(|wert| !wert.is_empty());
    let mut zusammenfassung = Zusammenfassung {
        eintraege: eintraege.len(),
        ..Zusammenfassung::default()
    };
    for teil in eintraege.values() {
        zusammenfassung.werte += teil.werte.len();
        zusammenfassung.zugaenge += teil.zugaenge.len();
    }
    let geheim = match (passwort, geheimnisse) {
        (Some(passwort), Some(geheimnisse)) if !geheimnisse.ist_leer() => {
            passwort_pruefen(passwort)?;
            zusammenfassung.geheimnisse = geheimnisse.anzahl();
            Some(verschliessen(geheimnisse, passwort)?)
        }
        _ => None,
    };
    let huelle = Huelle {
        format: FORMAT.to_string(),
        version: VERSION,
        erstellt: jetzt_rfc3339(),
        eintraege,
        geheim,
    };
    let text = serde_json::to_vec_pretty(&huelle)?;
    crate::dateitresor::schreiben_atomar(pfad, &text)?;
    Ok(zusammenfassung)
}

/// Eine Datei lesen und prüfen. Ein fremdes Format und ein falsches Passwort
/// sind Fehler **ohne** Inhalt: es wird nichts übernommen.
pub fn lesen(pfad: &Path, passwort: Option<&str>) -> anyhow::Result<Gelesen> {
    let text = std::fs::read_to_string(pfad)
        .map_err(|fehler| anyhow::anyhow!("Die Datei lässt sich nicht lesen: {fehler}"))?;
    let huelle: Huelle = serde_json::from_str(&text)
        .map_err(|_| anyhow::anyhow!("Das ist keine mcp-quiver-Sicherung"))?;
    if huelle.format != FORMAT {
        anyhow::bail!("Das ist keine mcp-quiver-Sicherung");
    }
    if huelle.version != VERSION {
        anyhow::bail!(
            "Die Sicherung hat Version {}, diese Fassung kennt nur Version {VERSION}",
            huelle.version
        );
    }
    let hat_geheimnisse = huelle.geheim.is_some();
    let geheimnisse = match (&huelle.geheim, passwort.map(str::trim)) {
        (Some(umschlag), Some(passwort)) if !passwort.is_empty() => {
            passwort_pruefen(passwort)?;
            Some(entschluesseln(umschlag, passwort)?)
        }
        _ => None,
    };
    Ok(Gelesen {
        eintraege: huelle.eintraege,
        geheimnisse,
        hat_geheimnisse,
    })
}

/// Der Vorschlag für den Dateinamen: `quiver-einstellungen-2026-11-23.json`.
pub fn dateiname() -> String {
    let (jahr, monat, tag) = zivildatum(tage_seit_1970());
    format!("quiver-einstellungen-{jahr:04}-{monat:02}-{tag:02}.json")
}

// ---------------------------------------------------- Vorschau und Mischen

/// Was ein Import täte: je Eintrag neu, geändert oder übersprungen.
///
/// Übersprungen wird, was es auf diesem Gerät nicht gibt (nicht im Soll);
/// der Eintrag bleibt trotzdem in der Datei vermerkt.
pub fn vorschau(
    gelesen: &Gelesen,
    soll: &Soll,
    werte: &BTreeMap<EintragId, Werte>,
    zugaenge: &BTreeMap<EintragId, BTreeMap<String, Werte>>,
    aus: &BTreeSet<EintragId>,
) -> Vorschau {
    let mut vorschau = Vorschau {
        eintraege: Vec::new(),
        hat_geheimnisse: gelesen.hat_geheimnisse,
    };
    for (id, teil) in &gelesen.eintraege {
        let Some(soll_eintrag) = soll
            .eintraege
            .iter()
            .find(|eintrag| eintrag.eintrag.id.als_str() == id)
        else {
            vorschau.eintraege.push(VorschauEintrag {
                id: id.clone(),
                titel: id.clone(),
                art: VorschauArt::Uebersprungen,
                neue: 0,
                geaenderte: 0,
                zugaenge: 0,
                am_geraet_aus: false,
            });
            continue;
        };
        let eintrag_id = &soll_eintrag.eintrag.id;
        let vorhanden = werte.get(eintrag_id);
        let mut neue = 0;
        let mut geaenderte = 0;
        for (name, wert) in &teil.werte {
            if wert.trim().is_empty() {
                continue;
            }
            match vorhanden.and_then(|werte| werte.get(name)) {
                None => neue += 1,
                Some(alt) if alt != wert => geaenderte += 1,
                _ => {}
            }
        }
        let art = if werte.contains_key(eintrag_id) || zugaenge.contains_key(eintrag_id) {
            VorschauArt::Aendert
        } else {
            VorschauArt::Neu
        };
        vorschau.eintraege.push(VorschauEintrag {
            id: id.clone(),
            titel: soll_eintrag.eintrag.titel.clone(),
            art,
            neue,
            geaenderte,
            zugaenge: teil.zugaenge.len(),
            am_geraet_aus: aus_mischen(aus.contains(eintrag_id), teil.am_geraet_aus),
        });
    }
    vorschau
}

/// Die Wahl „auf diesem Gerät abgeschaltet“ mischen: **aus bleibt aus**.
/// Kein Weg – auch kein Import – schaltet einen abgeschalteten Eintrag wieder
/// ein (Aufgabe 19).
///
pub fn aus_mischen(geraet: bool, datei: bool) -> bool {
    geraet || datei
}

/// Werte mischen: was aus der Datei kommt, überschreibt gleichnamige Werte;
/// was nur am Gerät steht, bleibt. Leere Werte der Datei zählen nicht.
pub fn werte_mischen(geraet: &Werte, datei: &Werte) -> Werte {
    let mut gemischt = geraet.clone();
    for (name, wert) in datei {
        if !wert.trim().is_empty() {
            gemischt.insert(name.clone(), wert.clone());
        }
    }
    gemischt
}

/// Zugänge über den Namen zusammenführen; je Zugang gewinnen die Felder der
/// Datei, die übrigen bleiben.
pub fn zugaenge_mischen(
    geraet: &BTreeMap<String, Werte>,
    datei: &BTreeMap<String, Werte>,
) -> BTreeMap<String, Werte> {
    let mut gemischt = geraet.clone();
    for (name, werte) in datei {
        let zugang = gemischt.entry(name.clone()).or_default();
        for (feld, wert) in werte {
            if !wert.trim().is_empty() {
                zugang.insert(feld.clone(), wert.clone());
            }
        }
    }
    gemischt
}

// ------------------------------------------------------------ Verschließen

fn passwort_pruefen(passwort: &str) -> anyhow::Result<()> {
    if passwort.chars().count() < PASSWORT_MIN {
        anyhow::bail!("Das Passwort muss mindestens {PASSWORT_MIN} Zeichen haben");
    }
    Ok(())
}

fn verschliessen(geheimnisse: &Geheimnisse, passwort: &str) -> anyhow::Result<Umschlag> {
    let mut salz = [0u8; SALZ_LEN];
    rand::rng().fill_bytes(&mut salz);
    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let schluessel = schluessel_ableiten(passwort, &salz)?;
    let klar = serde_json::to_vec(geheimnisse)?;
    let daten = XChaCha20Poly1305::new(Key::from_slice(&schluessel))
        .encrypt(XNonce::from_slice(&nonce), klar.as_ref())
        .map_err(|_| anyhow::anyhow!("Die Geheimnisse lassen sich nicht verschlüsseln"))?;
    Ok(Umschlag {
        kdf: "argon2id".to_string(),
        salz: B64.encode(salz),
        nonce: B64.encode(nonce),
        daten: B64.encode(daten),
    })
}

fn entschluesseln(umschlag: &Umschlag, passwort: &str) -> anyhow::Result<Geheimnisse> {
    // Nur, was diese Fassung kennt – alles andere ist kein Fehler des Users,
    // sondern eine fremde Datei.
    if umschlag.kdf != "argon2id" {
        anyhow::bail!("Die Sicherung nutzt ein unbekanntes Verfahren");
    }
    let salz = B64
        .decode(&umschlag.salz)
        .ok()
        .filter(|salz| salz.len() == SALZ_LEN)
        .ok_or_else(|| anyhow::anyhow!("Die Sicherung ist beschädigt"))?;
    let nonce = B64
        .decode(&umschlag.nonce)
        .ok()
        .filter(|nonce| nonce.len() == NONCE_LEN)
        .ok_or_else(|| anyhow::anyhow!("Die Sicherung ist beschädigt"))?;
    let daten = B64
        .decode(&umschlag.daten)
        .map_err(|_| anyhow::anyhow!("Die Sicherung ist beschädigt"))?;
    let schluessel = schluessel_ableiten(passwort, &salz)?;
    let klar = XChaCha20Poly1305::new(Key::from_slice(&schluessel))
        .decrypt(XNonce::from_slice(&nonce), daten.as_ref())
        .map_err(|_| anyhow::anyhow!("Das Passwort ist falsch"))?;
    serde_json::from_slice(&klar).map_err(|_| anyhow::anyhow!("Die Sicherung ist beschädigt"))
}

/// Der Schlüssel aus dem Passwort (Argon2id, m = 64 MiB, t = 3, p = 1).
fn schluessel_ableiten(passwort: &str, salz: &[u8]) -> anyhow::Result<[u8; SCHLUESSEL_LEN]> {
    let params = argon2::Params::new(ARGON2_MEMORIE, ARGON2_RUNDEN, 1, Some(SCHLUESSEL_LEN))
        .map_err(|fehler| anyhow::anyhow!("Argon2-Parameter: {fehler}"))?;
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut schluessel = [0u8; SCHLUESSEL_LEN];
    argon2
        .hash_password_into(passwort.as_bytes(), salz, &mut schluessel)
        .map_err(|_| anyhow::anyhow!("Der Schlüssel lässt sich nicht ableiten"))?;
    Ok(schluessel)
}

// ---------------------------------------------------------------- Die Zeit

/// Sekunden seit 1970 – ohne Uhr, die es nicht überall gibt.
fn tage_seit_1970() -> i64 {
    let sekunden = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dauer| dauer.as_secs())
        .unwrap_or_default();
    i64::try_from(sekunden / 86_400).unwrap_or_default()
}

/// Die aktuelle Zeit als RFC 3339 in UTC (`2026-11-23T10:11:12Z`).
fn jetzt_rfc3339() -> String {
    let sekunden = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dauer| dauer.as_secs())
        .unwrap_or_default();
    rfc3339(sekunden)
}

fn rfc3339(sekunden: u64) -> String {
    let tage = i64::try_from(sekunden / 86_400).unwrap_or_default();
    let rest = sekunden % 86_400;
    let (jahr, monat, tag) = zivildatum(tage);
    format!(
        "{jahr:04}-{monat:02}-{tag:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Tage seit 1970-01-01 → Jahr, Monat, Tag (Howard Hinnant, `civil_from_days`).
fn zivildatum(tage: i64) -> (i64, u32, u32) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_domain::{Eintrag, Herkunft, Laufzeit, SollEintrag};

    fn werte(paare: &[(&str, &str)]) -> Werte {
        paare
            .iter()
            .map(|(name, wert)| ((*name).to_string(), (*wert).to_string()))
            .collect()
    }

    fn eintrag(id: &str, titel: &str) -> Eintrag {
        Eintrag {
            id: EintragId::neu(id),
            version: "1.0.0".into(),
            titel: titel.into(),
            beschreibung: String::new(),
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
            mehrfach: None,
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: "gibt-es-nicht".into(),
                argumente: Vec::new(),
                umgebung: Werte::new(),
            },
            felder: Vec::new(),
        }
    }

    fn soll(ids: &[&str]) -> Soll {
        Soll::neu(
            ids.iter()
                .map(|id| SollEintrag {
                    eintrag: eintrag(id, &format!("Titel {id}")),
                    werte: Werte::new(),
                })
                .collect(),
        )
    }

    fn geleert() -> Gelesen {
        Gelesen {
            eintraege: BTreeMap::new(),
            geheimnisse: None,
            hat_geheimnisse: false,
        }
    }

    #[test]
    fn ohne_passwort_steht_kein_geheimnis_in_der_datei() {
        let ordner = tempfile::tempdir().unwrap();
        let pfad = ordner.path().join("einstellungen.json");
        let eintraege = BTreeMap::from([(
            "ssh".to_string(),
            EintragTeil {
                werte: werte(&[("host", "web-01.example")]),
                zugaenge: BTreeMap::from([("web-01".to_string(), werte(&[("port", "22")]))]),
                ..EintragTeil::default()
            },
        )]);
        let geheimnisse = Geheimnisse {
            eintraege: BTreeMap::from([(
                "ssh".to_string(),
                werte(&[("passwort", "streng-geheim")]),
            )]),
            zugaenge: BTreeMap::new(),
        };
        let zusammenfassung =
            schreiben(&pfad, eintraege, Some(&geheimnisse), None).expect("schreibbar");
        assert_eq!(zusammenfassung.eintraege, 1);
        assert_eq!(zusammenfassung.werte, 1);
        assert_eq!(zusammenfassung.zugaenge, 1);
        assert_eq!(
            zusammenfassung.geheimnisse, 0,
            "ohne Passwort keine Geheimnisse"
        );

        let text = std::fs::read_to_string(&pfad).unwrap();
        assert!(!text.contains("streng-geheim"), "Klartext in der Datei");
        assert!(!text.contains("passwort"), "kein Hinweis auf Geheimnisse");
        assert!(text.contains("\"geheim\": null"), "{text}");

        let gelesen = lesen(&pfad, None).expect("lesbar");
        assert!(!gelesen.hat_geheimnisse);
        assert_eq!(gelesen.eintraege["ssh"].werte["host"], "web-01.example");
    }

    #[test]
    fn mit_passwort_rundlauf_der_geheimnisse() {
        let ordner = tempfile::tempdir().unwrap();
        let pfad = ordner.path().join("einstellungen.json");
        let geheimnisse = Geheimnisse {
            eintraege: BTreeMap::from([(
                "ssh".to_string(),
                werte(&[("passwort", "streng-geheim")]),
            )]),
            zugaenge: BTreeMap::from([(
                "ssh".to_string(),
                BTreeMap::from([("web-01".to_string(), werte(&[("sudo", "auch-geheim")]))]),
            )]),
        };
        let zusammenfassung = schreiben(
            &pfad,
            BTreeMap::from([("ssh".to_string(), EintragTeil::default())]),
            Some(&geheimnisse),
            Some("ein-langes-passwort"),
        )
        .expect("schreibbar");
        assert_eq!(zusammenfassung.geheimnisse, 2);
        assert!(zusammenfassung.mit_geheimnissen());

        let text = std::fs::read_to_string(&pfad).unwrap();
        assert!(!text.contains("streng-geheim"), "Klartext in der Datei");
        assert!(text.contains("\"kdf\": \"argon2id\""), "{text}");

        let gelesen = lesen(&pfad, Some("ein-langes-passwort")).expect("lesbar");
        assert!(gelesen.hat_geheimnisse);
        assert_eq!(gelesen.geheimnisse, Some(geheimnisse));

        // Ein falsches Passwort ist ein Fehler ohne Inhalt.
        let fehler = lesen(&pfad, Some("falsches-passwort")).expect_err("muss scheitern");
        let text = format!("{fehler:#}");
        assert!(text.contains("Passwort ist falsch"), "{text}");
        // Ohne Passwort bleibt der Umschlag zu – die offenen Werte sind lesbar.
        let gelesen = lesen(&pfad, None).expect("lesbar");
        assert!(gelesen.hat_geheimnisse);
        assert_eq!(gelesen.geheimnisse, None);
    }

    #[test]
    fn zu_kurzes_passwort_wird_abgelehnt() {
        let ordner = tempfile::tempdir().unwrap();
        let pfad = ordner.path().join("einstellungen.json");
        let geheimnisse = Geheimnisse {
            eintraege: BTreeMap::from([("ssh".to_string(), werte(&[("passwort", "x")]))]),
            zugaenge: BTreeMap::new(),
        };
        let fehler = schreiben(&pfad, BTreeMap::new(), Some(&geheimnisse), Some("kurz"))
            .expect_err("zu kurz");
        assert!(format!("{fehler:#}").contains("mindestens 8 Zeichen"));
        assert!(!pfad.exists(), "es entsteht keine Datei");
    }

    #[test]
    fn fremdes_und_kaputtes_format_ist_ein_fehler_ohne_inhalt() {
        let ordner = tempfile::tempdir().unwrap();
        for (name, text) in [
            ("kaputt.json", "kein json".to_string()),
            (
                "fremd.json",
                serde_json::json!({
                    "format": "etwas-anderes",
                    "version": 1,
                    "eintraege": { "ssh": { "werte": { "host": "streng-geheim" } } }
                })
                .to_string(),
            ),
            (
                "neuere.json",
                serde_json::json!({
                    "format": FORMAT,
                    "version": 99,
                    "eintraege": {}
                })
                .to_string(),
            ),
        ] {
            let pfad = ordner.path().join(name);
            std::fs::write(&pfad, text).unwrap();
            let fehler = format!("{:#}", lesen(&pfad, None).expect_err("muss scheitern"));
            assert!(!fehler.contains("streng-geheim"), "{name}: {fehler}");
            assert!(
                !fehler.contains("host"),
                "{name}: der Fehler trägt keinen Inhalt: {fehler}"
            );
        }
        // Eine fehlende Datei ist auch ein Fehler ohne Inhalt.
        let fehler = format!(
            "{:#}",
            lesen(&ordner.path().join("gibt-es-nicht.json"), None).expect_err("fehlt")
        );
        assert!(fehler.contains("nicht lesen"), "{fehler}");
    }

    #[cfg(unix)]
    #[test]
    fn die_datei_gehoert_nur_dem_benutzer() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = tempfile::tempdir().unwrap();
        let pfad = ordner.path().join("einstellungen.json");
        schreiben(&pfad, BTreeMap::new(), None, None).expect("schreibbar");
        let modus = std::fs::metadata(&pfad).unwrap().permissions().mode() & 0o777;
        assert_eq!(modus, 0o600, "Rechte der Datei");
    }

    #[test]
    fn mischen_behaelt_was_nur_am_geraet_steht() {
        let geraet_werte = werte(&[("host", "alt"), ("pfad", "/x")]);
        let geraet = BTreeMap::from([(EintragId::neu("ssh"), geraet_werte.clone())]);
        let datei = BTreeMap::from([(
            "ssh".to_string(),
            EintragTeil {
                werte: werte(&[("host", "neu"), ("leer", "  ")]),
                zugaenge: BTreeMap::from([("web-01".to_string(), werte(&[("port", "22")]))]),
                ..EintragTeil::default()
            },
        )]);
        let gelesen = Gelesen {
            eintraege: datei.clone(),
            ..geleert()
        };
        let vorschau = vorschau(
            &gelesen,
            &soll(&["ssh"]),
            &geraet,
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        assert_eq!(vorschau.eintraege.len(), 1);
        let zeile = &vorschau.eintraege[0];
        assert_eq!(zeile.art, VorschauArt::Aendert);
        assert_eq!(zeile.neue, 0, "leere Werte zählen nicht");
        assert_eq!(zeile.geaenderte, 1);
        assert_eq!(zeile.zugaenge, 1);

        let gemischt = werte_mischen(&geraet_werte, &datei["ssh"].werte);
        assert_eq!(gemischt.get("host").map(String::as_str), Some("neu"));
        assert_eq!(
            gemischt.get("pfad").map(String::as_str),
            Some("/x"),
            "was nur am Gerät steht, bleibt"
        );
        assert!(!gemischt.contains_key("leer"), "leere Werte ändern nichts");

        let zugaenge = zugaenge_mischen(&BTreeMap::new(), &datei["ssh"].zugaenge);
        assert_eq!(
            zugaenge["web-01"].get("port").map(String::as_str),
            Some("22")
        );
    }

    #[test]
    fn unbekannte_eintraege_werden_uebersprungen_und_gemeldet() {
        let datei = BTreeMap::from([
            (
                "ssh".to_string(),
                EintragTeil {
                    werte: werte(&[("host", "web-01")]),
                    zugaenge: BTreeMap::new(),
                    ..EintragTeil::default()
                },
            ),
            (
                "gibt-es-hier-nicht".to_string(),
                EintragTeil {
                    werte: werte(&[("host", "weg")]),
                    zugaenge: BTreeMap::new(),
                    ..EintragTeil::default()
                },
            ),
        ]);
        let gelesen = Gelesen {
            eintraege: datei,
            ..geleert()
        };
        let vorschau = vorschau(
            &gelesen,
            &soll(&["ssh"]),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        assert_eq!(vorschau.uebernommen(), 1);
        assert_eq!(vorschau.uebersprungen(), 1);
        let weg = vorschau
            .eintraege
            .iter()
            .find(|eintrag| eintrag.id == "gibt-es-hier-nicht")
            .expect("der unbekannte Eintrag steht in der Vorschau");
        assert_eq!(weg.art, VorschauArt::Uebersprungen);
        assert_eq!(weg.titel, "gibt-es-hier-nicht", "ohne Titel die Kennung");
        // Ein Eintrag ohne lokale Werte ist neu.
        let ssh = vorschau
            .eintraege
            .iter()
            .find(|eintrag| eintrag.id == "ssh")
            .expect("ssh");
        assert_eq!(ssh.art, VorschauArt::Neu);
        assert_eq!(ssh.titel, "Titel ssh");
        assert_eq!(ssh.neue, 1);
    }

    #[test]
    fn die_zeit_ist_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(zivildatum(0), (1970, 1, 1));
        // Ein Schaltjahr und der Tag danach.
        assert_eq!(zivildatum(19_782), (2024, 2, 29));
        assert_eq!(zivildatum(19_783), (2024, 3, 1));
        let name = dateiname();
        assert!(name.starts_with("quiver-einstellungen-"), "{name}");
        assert!(name.ends_with(".json"), "{name}");
    }
}
