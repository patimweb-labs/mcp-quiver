// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kleinkram: Abbruch, Token, Hashes, Kindprozesse.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio::sync::Notify;

/// Ein Kindprozess ohne eigenes Konsolenfenster (Windows).
///
/// Der Client ist ein Fensterprogramm. Startet es ein Konsolenprogramm wie
/// `quiver-m365` oder ein anderes Werkzeug, legt Windows dafür ein schwarzes
/// Fenster an — es blitzt beim Anmelden auf, und beim Start eines Werkzeugs
/// bleibt es offen, bis das Werkzeug endet. `CREATE_NO_WINDOW` verhindert das:
/// angemeldet wird im Browser, und ein Werkzeug ist ein Hintergrundprozess, kein
/// zweites Programm mit eigener Konsole.
///
/// Auf allen anderen Systemen gibt es nichts zu tun. Das ist keine
/// Sonderbehandlung einer Ansicht (AGENTS.md §2.10), sondern die eine Stelle,
/// an der ein Prozess entsteht.
pub fn ohne_fenster(kommando: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Kein Konsolenfenster für den Kindprozess (`CREATE_NO_WINDOW`).
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        kommando.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        // Nichts zu tun: es gibt kein Fenster, das stören könnte.
        let _ = kommando;
    }
}

/// Ein Abbruchsignal, das sich teilen und abwarten lässt.
///
/// `tokio_util::sync::CancellationToken` wäre kürzer, ist aber keine
/// Abhängigkeit dieses Crates. Deshalb: Flag plus [`Notify`], und beim Warten
/// wird erst die Anmeldung erzeugt, dann das Flag geprüft – sonst geht ein
/// Abbruch zwischen Prüfung und Anmeldung verloren.
#[derive(Clone, Debug)]
pub struct Abbruch {
    innen: Arc<Innen>,
}

#[derive(Debug)]
struct Innen {
    zu: AtomicBool,
    meldung: Notify,
}

impl Abbruch {
    pub fn neu() -> Self {
        Self {
            innen: Arc::new(Innen {
                zu: AtomicBool::new(false),
                meldung: Notify::new(),
            }),
        }
    }

    pub fn abbrechen(&self) {
        if !self.innen.zu.swap(true, Ordering::SeqCst) {
            self.innen.meldung.notify_waiters();
        }
    }

    pub fn ist_abgebrochen(&self) -> bool {
        self.innen.zu.load(Ordering::SeqCst)
    }

    pub async fn warten(&self) {
        loop {
            let warten = self.innen.meldung.notified();
            tokio::pin!(warten);
            warten.as_mut().enable();
            if self.ist_abgebrochen() {
                return;
            }
            warten.await;
        }
    }
}

/// Zufällige Bytes.
fn zufall(anzahl: usize) -> Vec<u8> {
    use rand::RngCore;
    let mut puffer = vec![0u8; anzahl];
    rand::rng().fill_bytes(&mut puffer);
    puffer
}

/// Agent-Token: 32 zufällige Byte, hexadezimal.
pub fn token_erzeugen() -> String {
    hex::encode(zufall(32))
}

/// Kurze Kennung (acht Byte, hexadezimal).
pub fn kennung_erzeugen() -> String {
    hex::encode(zufall(8))
}

/// Zufälliger base64url-Wert ohne Auffüllzeichen (PKCE, `zustand`).
pub fn zufall_b64(anzahl: usize) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(zufall(anzahl))
}

/// PKCE-S256-Challenge zu einem Verifier (43 Zeichen base64url).
pub fn s256(verifier: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Was von einem Token gespeichert wird.
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
    fn hashes_sind_stabil() {
        assert_eq!(hash("abc").len(), 64);
        assert!(gleich_konstant(&hash("abc"), &hash("abc")));
        assert!(!gleich_konstant(&hash("abc"), &hash("abd")));
        assert!(!gleich_konstant("kurz", "laenger"));
    }

    #[test]
    fn token_sind_verschieden() {
        assert_ne!(token_erzeugen(), token_erzeugen());
        assert_eq!(token_erzeugen().len(), 64);
        assert_eq!(kennung_erzeugen().len(), 16);
    }

    /// PKCE: 32 Byte ergeben 43 Zeichen base64url; die Challenge ist S256.
    #[test]
    fn pkce_hat_die_verlangte_form() {
        let verifier = zufall_b64(32);
        assert_eq!(verifier.len(), 43);
        assert!(verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        let challenge = s256(&verifier);
        assert_eq!(challenge.len(), 43);
        assert_ne!(challenge, verifier);
        assert_eq!(challenge, s256(&verifier), "S256 ist stabil");
    }

    #[tokio::test]
    async fn abbruch_weckt_wartende() {
        let abbruch = Abbruch::neu();
        let warten = abbruch.clone();
        let aufgabe = tokio::spawn(async move { warten.warten().await });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        abbruch.abbrechen();
        tokio::time::timeout(std::time::Duration::from_secs(1), aufgabe)
            .await
            .expect("Abbruch weckt")
            .expect("Aufgabe läuft");
        assert!(abbruch.ist_abgebrochen());
    }

    #[tokio::test]
    async fn abbruch_vor_dem_warten_kommt_an() {
        let abbruch = Abbruch::neu();
        abbruch.abbrechen();
        tokio::time::timeout(std::time::Duration::from_secs(1), abbruch.warten())
            .await
            .expect("sofort fertig");
    }
}
