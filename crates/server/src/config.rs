// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Konfiguration aus der Umgebung und die wenigen Regeln, die schon beim Start
//! gelten: Basis-URL, Entra, vertraute Proxies, Altgeräte, Uhr.
//!
//! Alles, was hier abgelehnt wird, führt zu einem Startfehler mit klarem Text –
//! nie zu einem Server, der halb konfiguriert läuft.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use time::OffsetDateTime;
use url::Url;

use crate::krypto;

/// Die Uhr des Servers. Im Betrieb die Systemzeit, in Tests einstellbar.
#[derive(Clone, Default)]
pub struct Uhr(Arc<Mutex<Option<OffsetDateTime>>>);

impl std::fmt::Debug for Uhr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Uhr")
    }
}

impl Uhr {
    pub fn jetzt(&self) -> OffsetDateTime {
        let fest = crate::sperre(&self.0);
        fest.unwrap_or_else(OffsetDateTime::now_utc)
    }

    /// Die Uhr auf einen festen Zeitpunkt stellen (nur für Tests gedacht).
    pub fn setze(&self, zeit: OffsetDateTime) {
        *crate::sperre(&self.0) = Some(zeit);
    }

    /// Wieder die Systemzeit nehmen.
    pub fn zuruecksetzen(&self) {
        *crate::sperre(&self.0) = None;
    }
}

/// Entra-Konfiguration. Fehlt sie ganz, gilt der alte Weg mit Einmal-Codes.
#[derive(Clone)]
pub struct EntraKonfig {
    /// Mandant (GUID, nie `common` oder `organizations`).
    pub tenant: String,
    pub client_id: String,
    /// Das Client-Geheimnis. Wird nie geloggt und nie ausgeliefert.
    pub secret: String,
    /// Authority ohne abschließenden Schrägstrich (Loopback nur im Test).
    pub authority: String,
    /// Der erwartete `iss`; muss exakt der Authority entsprechen.
    pub issuer: String,
    /// `Authorization Code` + PKCE; die Rückkehr landet hier.
    pub redirect_uri: String,
}

impl std::fmt::Debug for EntraKonfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Das Geheimnis gehört nicht in ein Debug-Protokoll.
        f.debug_struct("EntraKonfig")
            .field("tenant", &self.tenant)
            .field("client_id", &self.client_id)
            .field("authority", &self.authority)
            .field("issuer", &self.issuer)
            .field("redirect_uri", &self.redirect_uri)
            .field("secret", &"<verborgen>")
            .finish()
    }
}

/// Woher der Server seine Daten nimmt.
#[derive(Debug, Clone)]
pub struct Konfig {
    pub daten: PathBuf,
    /// Gebaute Admin-Oberfläche; fehlt sie, gibt es nur die API.
    pub admin_dist: Option<PathBuf>,
    /// Basis-URL, unter der der Server erreichbar ist (ohne Schrägstrich).
    pub oeffentlich: String,
    /// `None` heißt: Einmal-Codes statt Entra (MVP ohne Azure).
    pub entra: Option<EntraKonfig>,
    /// Nur von diesen Netzen wird `X-Forwarded-For` geglaubt.
    pub proxies: Vec<Netz>,
    /// Dürfen Geräte ohne Entra-Benutzer weiterlaufen?
    pub altgeraete_erlaubt: bool,
    pub uhr: Uhr,
}

impl Konfig {
    /// Eine Konfiguration für Tests und den alten Weg ohne Entra.
    pub fn neu(daten: impl Into<PathBuf>) -> Self {
        Self {
            daten: daten.into(),
            admin_dist: None,
            oeffentlich: String::from("http://127.0.0.1:8740"),
            entra: None,
            proxies: Vec::new(),
            altgeraete_erlaubt: true,
            uhr: Uhr::default(),
        }
    }

    /// Alles aus der Umgebung lesen und prüfen.
    pub fn aus_umgebung(daten: PathBuf) -> anyhow::Result<Self> {
        let oeffentlich = std::env::var("QUIVER_OEFFENTLICH").unwrap_or_default();
        if oeffentlich.trim().is_empty() {
            anyhow::bail!(
                "QUIVER_OEFFENTLICH fehlt (Beispiel: https://quiver.example.test). \
                 Ohne Basis-URL lässt sich weder der Origin prüfen noch die Entra-Rückkehr bauen."
            );
        }
        let oeffentlich = basis_url_pruefen(&oeffentlich)?;

        let tenant = std::env::var("QUIVER_ENTRA_TENANT").unwrap_or_default();
        let client_id = std::env::var("QUIVER_ENTRA_CLIENT_ID").unwrap_or_default();
        let secret_pfad = std::env::var_os("QUIVER_ENTRA_SECRET_DATEI")
            .map(PathBuf::from)
            .unwrap_or_else(|| daten.join("entra-secret"));

        let gesetzt = [
            !tenant.trim().is_empty(),
            !client_id.trim().is_empty(),
            secret_pfad.exists() || std::env::var_os("QUIVER_ENTRA_SECRET_DATEI").is_some(),
        ];
        let teilweise = gesetzt.iter().any(|x| *x) && !gesetzt.iter().all(|x| *x);
        if teilweise {
            anyhow::bail!(
                "Entra ist nur teilweise gesetzt: QUIVER_ENTRA_TENANT, QUIVER_ENTRA_CLIENT_ID \
                 und QUIVER_ENTRA_SECRET_DATEI gehören zusammen (alle oder keine)."
            );
        }

        let entra = if gesetzt.iter().all(|x| *x) {
            let tenant = tenant.trim().to_string();
            if !ist_guid(&tenant) {
                anyhow::bail!("QUIVER_ENTRA_TENANT ist keine GUID (und nie common/organizations)");
            }
            let secret = std::fs::read_to_string(&secret_pfad)
                .map_err(|fehler| {
                    anyhow::anyhow!(
                        "Entra-Geheimnis {} nicht lesbar: {fehler}",
                        secret_pfad.display()
                    )
                })?
                .trim()
                .to_string();
            if secret.is_empty() {
                anyhow::bail!("Entra-Geheimnis in {} ist leer", secret_pfad.display());
            }
            if krypto::rechte_zu_offen(&secret_pfad) {
                tracing::warn!(
                    "{} ist für Gruppe oder alle lesbar; bitte auf 0600 setzen",
                    secret_pfad.display()
                );
            }
            let authority = match std::env::var("QUIVER_ENTRA_AUTHORITY") {
                Ok(wert) if !wert.trim().is_empty() => {
                    let wert = basis_url_pruefen(&wert)?;
                    if !ist_loopback_url(&wert) {
                        anyhow::bail!(
                            "QUIVER_ENTRA_AUTHORITY ist nur für Tests auf Loopback erlaubt"
                        );
                    }
                    wert
                }
                _ => format!("https://login.microsoftonline.com/{tenant}/v2.0"),
            };
            Some(EntraKonfig {
                tenant,
                client_id: client_id.trim().to_string(),
                secret,
                issuer: authority.clone(),
                authority,
                redirect_uri: format!("{oeffentlich}/auth/callback"),
            })
        } else {
            None
        };

        let proxies = match std::env::var("QUIVER_PROXIES") {
            Ok(text) if !text.trim().is_empty() => text
                .split(',')
                .map(str::trim)
                .filter(|teil| !teil.is_empty())
                .map(Netz::lesen)
                .collect::<anyhow::Result<Vec<_>>>()?,
            _ => Vec::new(),
        };

        let altgeraete_erlaubt = match std::env::var("QUIVER_ALTGERAETE") {
            Ok(wert) => match wert.trim().to_ascii_lowercase().as_str() {
                "" | "erlaubt" => true,
                "gesperrt" => false,
                anderes => {
                    anyhow::bail!("QUIVER_ALTGERAETE kennt nur erlaubt|gesperrt, nicht {anderes}")
                }
            },
            Err(_) => true,
        };

        Ok(Self {
            daten,
            admin_dist: None,
            oeffentlich,
            entra,
            proxies,
            altgeraete_erlaubt,
            uhr: Uhr::default(),
        })
    }

    pub fn mit_admin_dist(mut self, dist: Option<PathBuf>) -> Self {
        self.admin_dist = dist;
        self
    }

    /// Ist die Basis-URL `https` (dann bekommen Cookies `Secure` und `__Host-`)?
    pub fn ist_https(&self) -> bool {
        self.oeffentlich.starts_with("https://")
    }
}

/// Eine Basis-URL prüfen und ohne abschließenden Schrägstrich zurückgeben.
///
/// `https` ist Pflicht; `http` nur auf Loopback, damit Tests und die Entwicklung
/// ohne Zertifikat laufen können.
pub fn basis_url_pruefen(wert: &str) -> anyhow::Result<String> {
    let wert = wert.trim().trim_end_matches('/');
    let url = Url::parse(wert)
        .map_err(|fehler| anyhow::anyhow!("{wert} ist keine gültige URL: {fehler}"))?;
    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("{wert} darf weder Abfrage noch Sprungmarke enthalten");
    }
    if !url.path().is_empty() && url.path() != "/" {
        anyhow::bail!("{wert} darf keinen Pfad enthalten");
    }
    match url.scheme() {
        "https" => {}
        "http" => {
            if !ist_loopback_url(wert) {
                anyhow::bail!(
                    "{wert} ist http, aber nicht Loopback – ohne TLS startet der Server nicht"
                );
            }
        }
        anderes => {
            anyhow::bail!("{wert}: nur https (oder http auf Loopback) ist erlaubt, nicht {anderes}")
        }
    }
    Ok(wert.to_string())
}

/// Loopback: `localhost`, `127.0.0.1`, `[::1]` (auch `127.x.x.x`).
pub fn ist_loopback_url(wert: &str) -> bool {
    let Ok(url) = Url::parse(wert) else {
        return false;
    };
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Eine GUID in der Form 8-4-4-4-12.
pub fn ist_guid(wert: &str) -> bool {
    let wert = wert.trim();
    let teile: Vec<&str> = wert.split('-').collect();
    [8, 4, 4, 4, 12]
        .iter()
        .zip(&teile)
        .all(|(laenge, teil)| teil.len() == *laenge && teil.chars().all(|c| c.is_ascii_hexdigit()))
        && teile.len() == 5
}

/// Ein Netz in CIDR-Schreibweise, z. B. `10.0.0.0/8` oder `2001:db8::/32`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Netz {
    netz: IpAddr,
    bits: u8,
}

impl Netz {
    pub fn lesen(text: &str) -> anyhow::Result<Self> {
        let (adresse, bits) = text
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("{text} ist kein CIDR (Beispiel: 10.0.0.0/8)"))?;
        let netz: IpAddr = adresse
            .parse()
            .map_err(|_| anyhow::anyhow!("{text}: keine gültige Adresse"))?;
        let bits: u8 = bits
            .parse()
            .map_err(|_| anyhow::anyhow!("{text}: keine gültige Bitzahl"))?;
        let max = if netz.is_ipv4() { 32 } else { 128 };
        if bits > max {
            anyhow::bail!("{text}: höchstens {max} Bit");
        }
        Ok(Self { netz, bits })
    }

    pub fn enthaelt(&self, ip: IpAddr) -> bool {
        match (self.netz, ip) {
            (IpAddr::V4(netz), IpAddr::V4(ip)) => {
                let netz = u32::from(netz);
                let ip = u32::from(ip);
                let maske = if self.bits == 0 {
                    0
                } else {
                    u32::MAX << (32 - u32::from(self.bits))
                };
                netz & maske == ip & maske
            }
            (IpAddr::V6(netz), IpAddr::V6(ip)) => {
                let netz = u128::from(netz);
                let ip = u128::from(ip);
                let maske = if self.bits == 0 {
                    0
                } else {
                    u128::MAX << (128 - u32::from(self.bits))
                };
                netz & maske == ip & maske
            }
            _ => false,
        }
    }
}

/// Die IP des Aufrufers: nur von vertrauten Proxies wird `X-Forwarded-For` gelesen.
///
/// Ohne Peer-Adresse (z. B. bei `tower::oneshot`) gibt es keine IP; dann wird
/// auch kein Kopf geglaubt.
pub fn aufrufer_ip(
    peer: Option<SocketAddr>,
    x_forwarded_for: Option<&str>,
    proxies: &[Netz],
) -> Option<IpAddr> {
    let peer = peer?;
    if proxies.iter().any(|netz| netz.enthaelt(peer.ip())) {
        if let Some(kopf) = x_forwarded_for {
            if let Some(erste) = kopf.split(',').next() {
                if let Ok(ip) = erste.trim().parse::<IpAddr>() {
                    return Some(ip);
                }
            }
        }
    }
    Some(peer.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_url_regeln() {
        assert!(basis_url_pruefen("https://quiver.example.test").is_ok());
        assert_eq!(
            basis_url_pruefen("https://quiver.example.test/").unwrap(),
            "https://quiver.example.test"
        );
        assert!(basis_url_pruefen("http://127.0.0.1:8740").is_ok());
        assert!(basis_url_pruefen("http://localhost:8740").is_ok());
        assert!(basis_url_pruefen("http://[::1]:8740").is_ok());
        assert!(basis_url_pruefen("http://quiver.example.test").is_err());
        assert!(basis_url_pruefen("http://10.0.0.5:8740").is_err());
        assert!(basis_url_pruefen("ftp://quiver.example.test").is_err());
        assert!(basis_url_pruefen("https://quiver.example.test/pfad").is_err());
        assert!(basis_url_pruefen("https://quiver.example.test/?x=1").is_err());
        assert!(basis_url_pruefen("keine url").is_err());
    }

    #[test]
    fn guids() {
        assert!(ist_guid("11111111-2222-3333-4444-555555555555"));
        assert!(ist_guid("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"));
        assert!(!ist_guid("common"));
        assert!(!ist_guid("organizations"));
        assert!(!ist_guid("11111111-2222-3333-4444"));
    }

    #[test]
    fn cidr_und_forwarded_for() {
        let netz = Netz::lesen("10.0.0.0/8").unwrap();
        assert!(netz.enthaelt("10.1.2.3".parse().unwrap()));
        assert!(!netz.enthaelt("11.0.0.1".parse().unwrap()));
        let v6 = Netz::lesen("2001:db8::/32").unwrap();
        assert!(v6.enthaelt("2001:db8::1".parse().unwrap()));
        assert!(!v6.enthaelt("2001:db9::1".parse().unwrap()));
        assert!(Netz::lesen("10.0.0.0/33").is_err());
        assert!(Netz::lesen("kein-netz").is_err());

        let peer: SocketAddr = "10.0.0.5:1234".parse().unwrap();
        let alle = vec![Netz::lesen("10.0.0.0/8").unwrap()];
        assert_eq!(
            aufrufer_ip(Some(peer), Some("203.0.113.9, 10.0.0.5"), &alle),
            Some("203.0.113.9".parse().unwrap())
        );
        // Nicht vertrauter Proxy: der Kopf wird ignoriert.
        assert_eq!(
            aufrufer_ip(Some(peer), Some("203.0.113.9"), &[]),
            Some("10.0.0.5".parse().unwrap())
        );
        assert_eq!(aufrufer_ip(None, Some("203.0.113.9"), &alle), None);
    }

    #[test]
    fn uhr_ist_einstellbar() {
        let uhr = Uhr::default();
        let fest = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        uhr.setze(fest);
        assert_eq!(uhr.jetzt().unix_timestamp(), fest.unix_timestamp());
        uhr.zuruecksetzen();
        assert_ne!(uhr.jetzt().unix_timestamp(), fest.unix_timestamp());
    }
}
