// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Kleine Kryptobausteine des Servers: der Schlüssel für die Refresh-Token,
//! Rechteprüfung und Zufall. Keine eigene Kryptografie – XChaCha20-Poly1305
//! kommt aus `chacha20poly1305`, Argon2id aus `argon2`.

use std::path::Path;

use anyhow::Context;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;

/// Länge des Schlüssels in Byte.
pub const SCHLUESSEL_LAENGE: usize = 32;
/// Länge des Nonce in Byte (XChaCha20).
pub const NONCE_LAENGE: usize = 24;

/// Zufällige Bytes.
pub fn zufall(anzahl: usize) -> Vec<u8> {
    let mut puffer = vec![0u8; anzahl];
    rand::rng().fill_bytes(&mut puffer);
    puffer
}

/// Zufall als base64url ohne Auffüllung (URL-tauglich, 32 Byte = 43 Zeichen).
pub fn zufall_b64(anzahl: usize) -> String {
    URL_SAFE_NO_PAD.encode(zufall(anzahl))
}

/// Ein Schlüssel für die verschlüsselte Ablage.
pub struct Schluessel([u8; SCHLUESSEL_LAENGE]);

impl Schluessel {
    pub fn aus_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() != SCHLUESSEL_LAENGE {
            anyhow::bail!(
                "Schlüssel hat {} Byte, erwartet werden {SCHLUESSEL_LAENGE}",
                bytes.len()
            );
        }
        let mut innen = [0u8; SCHLUESSEL_LAENGE];
        innen.copy_from_slice(bytes);
        Ok(Self(innen))
    }

    /// `<daten>/schluessel` lesen oder beim ersten Start erzeugen (0600).
    pub fn laden(daten: &Path) -> anyhow::Result<Self> {
        let pfad = daten.join("schluessel");
        if pfad.exists() {
            let text = std::fs::read_to_string(&pfad)
                .with_context(|| format!("Schlüssel {} lesen", pfad.display()))?;
            let text = text.trim();
            let bytes = URL_SAFE_NO_PAD
                .decode(text)
                .or_else(|_| hex::decode(text))
                .with_context(|| format!("Schlüssel {} ist nicht lesbar", pfad.display()))?;
            return Self::aus_bytes(&bytes);
        }
        std::fs::create_dir_all(daten)?;
        let schluessel = Self::aus_bytes(&zufall(SCHLUESSEL_LAENGE))?;
        std::fs::write(&pfad, URL_SAFE_NO_PAD.encode(schluessel.0))
            .with_context(|| format!("Schlüssel {} schreiben", pfad.display()))?;
        nur_eigentuemer(&pfad)?;
        Ok(schluessel)
    }
}

impl std::fmt::Debug for Schluessel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Schluessel(<verborgen>)")
    }
}

/// Verschlüsseln: `nonce || ciphertext`, base64url.
pub fn verschluesseln(schluessel: &Schluessel, aad: &str, klar: &[u8]) -> anyhow::Result<String> {
    let chiffre = XChaCha20Poly1305::new((&schluessel.0).into());
    let nonce = zufall(NONCE_LAENGE);
    let geheim = chiffre
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: klar,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("Verschlüsseln gescheitert"))?;
    let mut aus = nonce;
    aus.extend_from_slice(&geheim);
    Ok(URL_SAFE_NO_PAD.encode(aus))
}

/// Entschlüsseln; die AAD muss dieselbe sein, sonst scheitert es.
pub fn entschluesseln(schluessel: &Schluessel, aad: &str, text: &str) -> anyhow::Result<Vec<u8>> {
    let roh = URL_SAFE_NO_PAD
        .decode(text)
        .context("Geheimnis ist kein base64url")?;
    if roh.len() <= NONCE_LAENGE {
        anyhow::bail!("Geheimnis ist zu kurz");
    }
    let (nonce, geheim) = roh.split_at(NONCE_LAENGE);
    let chiffre = XChaCha20Poly1305::new((&schluessel.0).into());
    chiffre
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: geheim,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("Entschlüsseln gescheitert (falscher Schlüssel oder AAD)"))
}

/// Schlüssel für die AAD einer Identität: `(tid, oid)`.
pub fn aad(tid: &str, oid: &str) -> String {
    format!("quiver|{tid}|{oid}")
}

/// Sind die Rechte zu offen (Gruppe oder alle dürfen lesen)?
///
/// Unter Unix geprüft, sonst `false` – Windows kennt diese Bits nicht.
pub fn rechte_zu_offen(pfad: &Path) -> bool {
    rechte_zu_offen_impl(pfad)
}

#[cfg(unix)]
fn rechte_zu_offen_impl(pfad: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(pfad) {
        Ok(meta) => meta.permissions().mode() & 0o077 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn rechte_zu_offen_impl(_pfad: &Path) -> bool {
    false
}

/// 0600 setzen, wo es das gibt.
pub fn nur_eigentuemer(pfad: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(pfad, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    let _ = pfad;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundlauf_mit_aad() {
        let schluessel = Schluessel::aus_bytes(&[7u8; SCHLUESSEL_LAENGE]).unwrap();
        let kontext = aad("tid-1", "oid-1");
        let geheim = verschluesseln(&schluessel, &kontext, b"refresh-token").unwrap();
        assert_eq!(
            entschluesseln(&schluessel, &kontext, &geheim).unwrap(),
            b"refresh-token"
        );
        // Andere Identität: die AAD passt nicht.
        assert!(entschluesseln(&schluessel, &aad("tid-1", "oid-2"), &geheim).is_err());
        // Anderer Schlüssel: scheitert.
        let anderer = Schluessel::aus_bytes(&[8u8; SCHLUESSEL_LAENGE]).unwrap();
        assert!(entschluesseln(&anderer, &kontext, &geheim).is_err());
        // Zufall: zweimal verschlüsseln ergibt zwei Texte.
        let zweiter = verschluesseln(&schluessel, &kontext, b"refresh-token").unwrap();
        assert_ne!(geheim, zweiter);
    }

    #[test]
    fn schluesseldatei_wird_erzeugt() {
        let ordner = std::env::temp_dir().join(format!(
            "quiver-schluessel-{}-{}",
            std::process::id(),
            crate::hilfen::token_erzeugen()
        ));
        std::fs::create_dir_all(&ordner).unwrap();
        let a = Schluessel::laden(&ordner).unwrap();
        let b = Schluessel::laden(&ordner).unwrap();
        let text = verschluesseln(&a, "aad", b"x").unwrap();
        assert_eq!(entschluesseln(&b, "aad", &text).unwrap(), b"x");
        assert!(ordner.join("schluessel").exists());
        assert!(!rechte_zu_offen(&ordner.join("schluessel")));
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[cfg(unix)]
    #[test]
    fn offene_rechte_werden_erkannt() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = std::env::temp_dir().join(format!(
            "quiver-rechte-{}-{}",
            std::process::id(),
            zufall_b64(8)
        ));
        std::fs::create_dir_all(&ordner).unwrap();
        let pfad = ordner.join("entra-secret");
        std::fs::write(&pfad, "kein-echtes-geheimnis").unwrap();
        std::fs::set_permissions(&pfad, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(rechte_zu_offen(&pfad), "0644 ist zu offen");
        nur_eigentuemer(&pfad).unwrap();
        assert!(!rechte_zu_offen(&pfad), "0600 ist in Ordnung");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn zufall_b64_hat_43_zeichen() {
        let wert = zufall_b64(32);
        assert_eq!(wert.len(), 43);
        assert!(wert
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
