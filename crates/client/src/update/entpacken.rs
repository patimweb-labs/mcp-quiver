// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Pakete entpacken – ZIP und tar.gz, mit Bordmitteln dieses Programms
//! (Aufgabe 39).
//!
//! Kein fremdes Werkzeug: `unzip`, `tar` oder ein Helferprogramm wären auf dem
//! Gerät des Benutzers nicht garantiert vorhanden (Windows hat weder `unzip`
//! noch `tar`) und würden geprüft werden müssen. Hier wird gelesen, was die
//! Formate ohnehin offenlegen; entpackt wird nur, was quiver braucht:
//!
//! * nur **gewöhnliche Dateien und Ordner**, keine Verweise, keine Geräte,
//! * nur Namen **innerhalb** des Zielordners (kein `..`, kein `/` am Anfang,
//!   kein Laufwerksbuchstabe),
//! * höchstens [`EINTRAEGE_MAX`] Einträge und höchstens [`BYTES_MAX`] Bytes,
//! * deflate (ZIP-Methode 8) und gzip über `flate2`.
//!
//! Was hier entsteht, ist noch **nicht** getauscht: erst nach dem Selbsttest
//! der neuen Fassung wird es an seinen Platz gelegt.

use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::{DeflateDecoder, GzDecoder};

/// So viele Einträge darf ein Paket höchstens haben.
pub const EINTRAEGE_MAX: usize = 10_000;
/// So viele Bytes dürfen entpackt werden.
pub const BYTES_MAX: u64 = 1024 * 1024 * 1024;

/// Warum ein Paket nicht entpackt werden kann.
#[derive(Debug, thiserror::Error)]
pub enum EntpackFehler {
    #[error("das Paket ist beschädigt: {0}")]
    Beschaedigt(String),
    #[error("das Paket enthält einen unsicheren Pfad: {0}")]
    Unsicher(String),
    #[error("das Paket ist zu groß oder hat zu viele Einträge")]
    ZuGross,
    #[error("das Paket lässt sich nicht schreiben: {0}")]
    Schreiben(String),
}

fn beschädigt(text: impl Into<String>) -> EntpackFehler {
    EntpackFehler::Beschaedigt(text.into())
}

/// Einen Namen aus dem Paket zu einem Pfad im Zielordner machen.
///
/// `None` heißt: der Name taugt nicht (absolut, mit `..`, leer).
fn sicherer_pfad(ziel: &Path, name: &str) -> Result<PathBuf, EntpackFehler> {
    let name = name.replace('\\', "/");
    if name.is_empty() || name.starts_with('/') || name.contains('\0') {
        return Err(EntpackFehler::Unsicher(name));
    }
    let mut pfad = ziel.to_path_buf();
    for teil in name.split('/') {
        if teil.is_empty() || teil == "." {
            continue;
        }
        if teil == ".." || teil.contains(':') {
            return Err(EntpackFehler::Unsicher(name));
        }
        pfad.push(teil);
    }
    if !pfad.starts_with(ziel) || pfad == ziel {
        return Err(EntpackFehler::Unsicher(name));
    }
    Ok(pfad)
}

/// Einen Eintrag schreiben; Ordner werden angelegt.
fn schreiben(
    ziel: &Path,
    name: &str,
    ist_ordner: bool,
    daten: impl Read,
    anzahl: &mut usize,
    gesamt: &mut u64,
) -> Result<PathBuf, EntpackFehler> {
    *anzahl += 1;
    if *anzahl > EINTRAEGE_MAX {
        return Err(EntpackFehler::ZuGross);
    }
    let pfad = sicherer_pfad(ziel, name)?;
    // Ein Verweis im Ziel wäre ein Ausweg aus dem Ordner: was schon da ist,
    // wird nicht benutzt.
    if let Ok(meta) = std::fs::symlink_metadata(&pfad) {
        if meta.file_type().is_symlink() {
            let _ = std::fs::remove_file(&pfad);
        }
    }
    if ist_ordner {
        std::fs::create_dir_all(&pfad)
            .map_err(|fehler| EntpackFehler::Schreiben(fehler.to_string()))?;
        return Ok(pfad);
    }
    if let Some(ordner) = pfad.parent() {
        std::fs::create_dir_all(ordner)
            .map_err(|fehler| EntpackFehler::Schreiben(fehler.to_string()))?;
    }
    let mut datei = std::fs::File::create(&pfad)
        .map_err(|fehler| EntpackFehler::Schreiben(fehler.to_string()))?;
    let mut puffer = [0u8; 64 * 1024];
    let mut leser = daten;
    loop {
        let gelesen = leser
            .read(&mut puffer)
            .map_err(|fehler| beschädigt(fehler.to_string()))?;
        if gelesen == 0 {
            break;
        }
        *gesamt += gelesen as u64;
        if *gesamt > BYTES_MAX {
            return Err(EntpackFehler::ZuGross);
        }
        use std::io::Write as _;
        datei
            .write_all(&puffer[..gelesen])
            .map_err(|fehler| EntpackFehler::Schreiben(fehler.to_string()))?;
    }
    Ok(pfad)
}

// ------------------------------------------------------------------- ZIP

/// Ein Eintrag im zentralen Verzeichnis einer ZIP-Datei.
struct ZipEintrag {
    name: String,
    methode: u16,
    komprimiert: u64,
    unkomprimiert: u64,
    kopf: usize,
}

fn u16_aus(bytes: &[u8], ab: usize) -> u16 {
    u16::from_le_bytes([bytes[ab], bytes[ab + 1]])
}

fn u32_aus(bytes: &[u8], ab: usize) -> u32 {
    u32::from_le_bytes([bytes[ab], bytes[ab + 1], bytes[ab + 2], bytes[ab + 3]])
}

/// Das zentrale Verzeichnis finden (Ende- und Zentralverzeichnis-Satz).
fn zip_verzeichnis(bytes: &[u8]) -> Result<(usize, usize), EntpackFehler> {
    if bytes.len() < 22 {
        return Err(beschädigt("zu kurz für ein ZIP"));
    }
    // Das Ende-Satz steht am Schluss, hinter einem Kommentar von höchstens
    // 64 KiB. Rückwärts suchen.
    let anfang = bytes.len().saturating_sub(22 + 65_535);
    let mut ende = None;
    for i in (anfang..=bytes.len() - 22).rev() {
        if bytes[i..i + 4] == [0x50, 0x4b, 0x05, 0x06] {
            ende = Some(i);
            break;
        }
    }
    let ende = ende.ok_or_else(|| beschädigt("kein ZIP-Ende"))?;
    let anzahl = u16_aus(bytes, ende + 10) as usize;
    let groesse = u32_aus(bytes, ende + 12) as usize;
    let versatz = u32_aus(bytes, ende + 16) as usize;
    if versatz == 0xffff_ffff || versatz + groesse > bytes.len() {
        return Err(beschädigt("ZIP64 oder abgeschnitten"));
    }
    Ok((versatz, anzahl))
}

fn zip_eintraege(bytes: &[u8]) -> Result<Vec<ZipEintrag>, EntpackFehler> {
    let (mut pos, anzahl) = zip_verzeichnis(bytes)?;
    if anzahl > EINTRAEGE_MAX {
        return Err(EntpackFehler::ZuGross);
    }
    let mut liste = Vec::new();
    for _ in 0..anzahl {
        if pos + 46 > bytes.len() || bytes[pos..pos + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err(beschädigt("zentrales Verzeichnis"));
        }
        let methode = u16_aus(bytes, pos + 10);
        let komprimiert = u32_aus(bytes, pos + 20) as u64;
        let unkomprimiert = u32_aus(bytes, pos + 24) as u64;
        let name_laenge = u16_aus(bytes, pos + 28) as usize;
        let extra = u16_aus(bytes, pos + 30) as usize;
        let kommentar = u16_aus(bytes, pos + 32) as usize;
        let kopf = u32_aus(bytes, pos + 42) as usize;
        let name_anfang = pos + 46;
        if name_anfang + name_laenge > bytes.len() {
            return Err(beschädigt("Name im Verzeichnis"));
        }
        let name =
            String::from_utf8_lossy(&bytes[name_anfang..name_anfang + name_laenge]).to_string();
        liste.push(ZipEintrag {
            name,
            methode,
            komprimiert,
            unkomprimiert,
            kopf,
        });
        pos = name_anfang + name_laenge + extra + kommentar;
    }
    Ok(liste)
}

/// Ein ZIP-Paket entpacken. Gibt zurück, was geschrieben wurde.
pub fn zip_entpacken(bytes: &[u8], ziel: &Path) -> Result<Vec<PathBuf>, EntpackFehler> {
    std::fs::create_dir_all(ziel).map_err(|fehler| EntpackFehler::Schreiben(fehler.to_string()))?;
    let eintraege = zip_eintraege(bytes)?;
    let mut geschrieben = Vec::new();
    let mut gesamt = 0u64;
    let mut anzahl = 0usize;
    for eintrag in eintraege {
        if eintrag.unkomprimiert > BYTES_MAX {
            return Err(EntpackFehler::ZuGross);
        }
        // Lokaler Kopf: Name und Extra können dort anders lang sein.
        if eintrag.kopf + 30 > bytes.len()
            || bytes[eintrag.kopf..eintrag.kopf + 4] != [0x50, 0x4b, 0x03, 0x04]
        {
            return Err(beschädigt("lokaler Kopf"));
        }
        let name_laenge = u16_aus(bytes, eintrag.kopf + 26) as usize;
        let extra = u16_aus(bytes, eintrag.kopf + 28) as usize;
        let daten_anfang = eintrag.kopf + 30 + name_laenge + extra;
        let ende = daten_anfang
            .checked_add(eintrag.komprimiert as usize)
            .filter(|ende| *ende <= bytes.len())
            .ok_or_else(|| beschädigt("Daten abgeschnitten"))?;
        let daten = &bytes[daten_anfang..ende];
        let ist_ordner = eintrag.name.ends_with('/') || eintrag.name.ends_with('\\');
        let pfad = match eintrag.methode {
            0 => schreiben(
                ziel,
                &eintrag.name,
                ist_ordner,
                daten,
                &mut anzahl,
                &mut gesamt,
            )?,
            8 => schreiben(
                ziel,
                &eintrag.name,
                ist_ordner,
                DeflateDecoder::new(daten),
                &mut anzahl,
                &mut gesamt,
            )?,
            anderes => {
                return Err(beschädigt(format!(
                    "{} ist mit Methode {anderes} gepackt",
                    eintrag.name
                )))
            }
        };
        if !ist_ordner {
            geschrieben.push(pfad);
        }
    }
    Ok(geschrieben)
}

// ---------------------------------------------------------------- tar.gz

/// Ein tar-Paket (gzip) entpacken. Gibt zurück, was geschrieben wurde.
pub fn tar_gz_entpacken(bytes: &[u8], ziel: &Path) -> Result<Vec<PathBuf>, EntpackFehler> {
    std::fs::create_dir_all(ziel).map_err(|fehler| EntpackFehler::Schreiben(fehler.to_string()))?;
    let mut archiv = Vec::new();
    GzDecoder::new(bytes)
        .read_to_end(&mut archiv)
        .map_err(|fehler| beschädigt(format!("gzip: {fehler}")))?;
    if archiv.len() as u64 > BYTES_MAX {
        return Err(EntpackFehler::ZuGross);
    }
    let mut geschrieben = Vec::new();
    let mut gesamt = 0u64;
    let mut anzahl = 0usize;
    let mut pos = 0usize;
    while pos + 512 <= archiv.len() {
        let kopf = &archiv[pos..pos + 512];
        if kopf.iter().all(|byte| *byte == 0) {
            break;
        }
        let name = tar_text(&kopf[0..100]);
        let groesse = tar_zahl(&kopf[124..136])?;
        let art = kopf[156];
        // `ustar`-Präfix für lange Namen.
        let praefix = if kopf[257..262] == *b"ustar" {
            tar_text(&kopf[345..500])
        } else {
            String::new()
        };
        let name = if praefix.is_empty() {
            name
        } else {
            format!("{praefix}/{name}")
        };
        let daten_anfang = pos + 512;
        let daten_ende = daten_anfang
            .checked_add(groesse as usize)
            .filter(|ende| *ende <= archiv.len())
            .ok_or_else(|| beschädigt("tar-Eintrag abgeschnitten"))?;
        // Nur gewöhnliche Dateien (0, '0') und Ordner ('5'); Verweise und
        // Geräte werden übergangen.
        match art {
            b'5' => {
                schreiben(ziel, &name, true, &[][..], &mut anzahl, &mut gesamt)?;
            }
            0 | b'0' => {
                let daten = &archiv[daten_anfang..daten_ende];
                let pfad = schreiben(ziel, &name, false, daten, &mut anzahl, &mut gesamt)?;
                geschrieben.push(pfad);
            }
            _ => {}
        }
        pos = daten_anfang + groesse as usize;
        pos += (512 - pos % 512) % 512;
    }
    Ok(geschrieben)
}

/// Ein Textfeld aus einem tar-Kopf (bis zum ersten Null-Byte).
fn tar_text(feld: &[u8]) -> String {
    let ende = feld
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(feld.len());
    String::from_utf8_lossy(&feld[..ende]).trim().to_string()
}

/// Eine Zahl im tar-Kopf: acht Oktalziffern, sonst binär (GNU).
fn tar_zahl(feld: &[u8]) -> Result<u64, EntpackFehler> {
    if let Some(binär) = feld.first().filter(|byte| **byte & 0x80 != 0) {
        let _ = binär;
        let mut wert = 0u64;
        for byte in &feld[1..] {
            wert = (wert << 8) | *byte as u64;
        }
        return Ok(wert);
    }
    let text = tar_text(feld);
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(&text, 8).map_err(|_| beschädigt(format!("tar-Zahl {text:?}")))
}

/// Der Pfad einer Datei, die in einem entpackten Baum liegt (nach Namen).
pub fn finde_datei(wurzel: &Path, name: &str) -> Option<PathBuf> {
    let mut stapel = vec![wurzel.to_path_buf()];
    let mut gesehen = 0usize;
    while let Some(ordner) = stapel.pop() {
        let Ok(eintraege) = std::fs::read_dir(&ordner) else {
            continue;
        };
        for eintrag in eintraege.flatten() {
            gesehen += 1;
            if gesehen > EINTRAEGE_MAX {
                return None;
            }
            let pfad = eintrag.path();
            let art = eintrag.file_type().ok()?;
            if art.is_dir() {
                stapel.push(pfad);
            } else if art.is_file()
                && eintrag
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(name)
            {
                return Some(pfad);
            }
        }
    }
    None
}

/// Alle Pfadteile eines Eintrags sind harmlos? (für Tests und Prüfungen)
pub fn pfad_ist_sicher(name: &str) -> bool {
    sicherer_pfad(Path::new("ziel"), name).is_ok()
}

#[cfg(test)]
mod tests;
