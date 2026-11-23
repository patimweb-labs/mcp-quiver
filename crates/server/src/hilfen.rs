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
}
