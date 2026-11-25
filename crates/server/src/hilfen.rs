// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kleinkram: Codes, Token, Hashes, Zeit.

use rand::RngCore;
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Zeichen der Einmal-Codes: ohne `I`, `O`, `0` und `1`, damit man sie abtippen kann.
pub const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Länge eines Codes ohne Bindestrich.
pub const CODE_LAENGE: usize = 8;

pub fn jetzt() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// Zeit als RFC 3339 (so steht sie im Vertrag).
pub fn als_text(zeit: OffsetDateTime) -> String {
    zeit.format(&Rfc3339).unwrap_or_default()
}

pub fn zeit_lesen(text: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(text, &Rfc3339).ok()
}

fn zufall(anzahl: usize) -> Vec<u8> {
    let mut puffer = vec![0u8; anzahl];
    rand::rng().fill_bytes(&mut puffer);
    puffer
}

/// Einmal-Code in der Anzeigeform `XXXX-XXXX`.
///
/// 32 Zeichen im Alphabet teilen 256 ohne Rest, deshalb ist das Modulo ohne Vorliebe.
pub fn code_erzeugen() -> String {
    let roh = zufall(CODE_LAENGE);
    let mut aus = String::with_capacity(CODE_LAENGE + 1);
    for (i, byte) in roh.iter().enumerate() {
        if i == 4 {
            aus.push('-');
        }
        aus.push(CODE_ALPHABET[usize::from(byte % CODE_ALPHABET.len() as u8)] as char);
    }
    aus
}

/// Eingabe des Nutzers → gespeicherte Form (acht Zeichen, Großbuchstaben, ohne Bindestrich).
///
/// `None`, wenn Zeichen vorkommen, die kein Code enthält.
pub fn code_normalisieren(eingabe: &str) -> Option<String> {
    let mut aus = String::with_capacity(CODE_LAENGE);
    for zeichen in eingabe.chars() {
        if zeichen == '-' || zeichen.is_whitespace() {
            continue;
        }
        if !zeichen.is_ascii() {
            return None;
        }
        let zeichen = zeichen.to_ascii_uppercase();
        if !CODE_ALPHABET.contains(&(zeichen as u8)) {
            return None;
        }
        aus.push(zeichen);
    }
    (aus.len() == CODE_LAENGE).then_some(aus)
}

/// Geräte-Token: 32 zufällige Byte, hexadezimal.
pub fn token_erzeugen() -> String {
    hex::encode(zufall(32))
}

/// Kennung eines Geräts: 6 zufällige Byte, hexadezimal (zwölf Zeichen).
pub fn geraet_id_erzeugen() -> String {
    hex::encode(zufall(6))
}

/// Kurze Kennung für eine Fehlermeldung (acht Zeichen hexadezimal).
///
/// Sie verbindet die Antwort mit der Zeile im Log, ohne etwas über den Fehler
/// zu verraten.
pub fn kennung_erzeugen() -> String {
    hex::encode(zufall(4))
}

/// Was in der Datenbank vom Token steht.
pub fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Vergleich in konstanter Zeit (die Länge darf durchsickern).
pub fn gleich_konstant(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |rest, (x, y)| rest | (x ^ y)) == 0
}

/// Ein ETag zu einem Wert: [`kanonisch`] plus FNV-1a (64 Bit), als Hex.
///
/// Die Verwaltung berechnet denselben Wert aus dem geladenen Stand (Vue:
/// `logik/etag.ts`); deshalb steht das Verfahren hier **einmal** und ist
/// getestet. Kein Geheimnis, keine Signature – nur ein Vergleichswert dafür,
/// ob sich etwas geändert hat.
pub fn etag<T: serde::Serialize>(wert: &T) -> String {
    let wert = serde_json::to_value(wert).unwrap_or(serde_json::Value::Null);
    let text = kanonisch(&wert);
    format!("\"{:016x}\"", fnv1a_64(text.as_bytes()))
}

/// JSON mit sortierten Schlüsseln und ohne Leerraum.
///
/// Beide Seiten müssen auf dieselbe Zeichenkette kommen; Objektschlüssel in
/// fester Reihenfolge sind der einfachste Weg dorthin. Zahlen sind im Modell
/// ganzzahlig, deshalb gibt es keine Abweichung bei der Darstellung.
pub fn kanonisch(wert: &serde_json::Value) -> String {
    use serde_json::Value;
    match wert {
        Value::Object(karte) => {
            let mut namen: Vec<&String> = karte.keys().collect();
            namen.sort();
            let teile: Vec<String> = namen
                .into_iter()
                .map(|name| {
                    format!(
                        "{}:{}",
                        serde_json::Value::String(name.clone()),
                        kanonisch(&karte[name])
                    )
                })
                .collect();
            format!("{{{}}}", teile.join(","))
        }
        Value::Array(liste) => {
            let teile: Vec<String> = liste.iter().map(kanonisch).collect();
            format!("[{}]", teile.join(","))
        }
        anderes => anderes.to_string(),
    }
}

/// FNV-1a (64 Bit) – dieselbe Rechnung wie in `logik/etag.ts`.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_hat_anzeigeform() {
        let code = code_erzeugen();
        assert_eq!(code.len(), 9);
        assert_eq!(code.as_bytes()[4], b'-');
        assert!(code
            .chars()
            .filter(|c| *c != '-')
            .all(|c| { CODE_ALPHABET.contains(&(c.to_ascii_uppercase() as u8)) }));
        let normal = code_normalisieren(&code).expect("eigener Code ist gültig");
        assert_eq!(normal.len(), CODE_LAENGE);
        assert_eq!(
            code_normalisieren(&normal.to_lowercase()),
            Some(normal.clone())
        );
        assert_eq!(
            code_normalisieren(&format!("{}-{}", &normal[..4], &normal[4..])),
            Some(normal)
        );
    }

    #[test]
    fn falsche_codes_werden_abgelehnt() {
        assert_eq!(code_normalisieren(""), None);
        assert_eq!(code_normalisieren("ABC"), None);
        assert_eq!(code_normalisieren("ABCD-EFGH-IJKL"), None);
        assert_eq!(code_normalisieren("ABCD-EFG0"), None, "0 gehört nicht dazu");
        assert_eq!(code_normalisieren("ABCD-EFGÄ"), None);
    }

    #[test]
    fn hashes_sind_stabil() {
        assert_eq!(hash("abc").len(), 64);
        assert!(gleich_konstant(&hash("abc"), &hash("abc")));
        assert!(!gleich_konstant(&hash("abc"), &hash("abd")));
        assert!(!gleich_konstant("kurz", "laenger"));
    }

    #[test]
    fn zeit_rundlauf() {
        let jetzt = jetzt();
        let text = als_text(jetzt);
        let zurueck = zeit_lesen(&text).expect("RFC 3339");
        assert_eq!(zurueck.unix_timestamp(), jetzt.unix_timestamp());
        assert_eq!(zeit_lesen("keine Zeit"), None);
    }

    /// Das ETag hängt am Inhalt und nicht an der Reihenfolge der Schlüssel.
    /// Dieselbe Rechnung steht in `admin/src/logik/etag.ts`.
    #[test]
    fn etag_ist_inhaltsgleich() {
        let a: serde_json::Value = serde_json::from_str(r#"{"b":1,"a":[1,2]}"#).unwrap();
        let b: serde_json::Value = serde_json::from_str(r#"{"a":[1,2],"b":1}"#).unwrap();
        assert_eq!(etag(&a), etag(&b));
        assert_ne!(etag(&a), etag(&serde_json::json!({"a": [1, 3], "b": 1})));
        // Der Wert steht in Anführungszeichen (Entity-Tag) und ist stabil.
        assert!(etag(&a).starts_with('"') && etag(&a).ends_with('"'));
        assert_eq!(etag(&a), etag(&a));
        // FNV-1a, feste Werte: wer die Rechnung ändert, merkt es hier.
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        // Dieselben Werte bildet `admin/src/logik/etag.ts`.
        assert_eq!(etag(&a), "\"30077cb0d8fc07be\"");
        assert_eq!(etag(&serde_json::json!({})), "\"08f44b07b5901a25\"");
        assert_eq!(etag(&serde_json::json!([])), "\"09612b07b5ecb5a5\"");
    }
}
