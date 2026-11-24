// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Drossel gegen Rateversuche: 5 frei, danach wächst die Wartezeit bis 15
//! Minuten. Gilt je IP **und** je Konto, nie dauerhaft. Die Prüfung passiert
//! **vor** dem Vergleich von Passwort oder Code.

use std::collections::HashMap;
use std::sync::Mutex;

use time::OffsetDateTime;

/// So viele Versuche sind frei.
pub const FREI: u32 = 5;
/// Längste Sperre.
pub const HOECHSTENS_SEKUNDEN: i64 = 15 * 60;

#[derive(Debug, Clone, Copy)]
struct Eintrag {
    fehlversuche: u32,
    letzter: OffsetDateTime,
    gesperrt_bis: Option<OffsetDateTime>,
}

/// Zähler je Schlüssel (IP, Konto, Code).
#[derive(Debug, Default)]
pub struct Drossel {
    eintraege: Mutex<HashMap<String, Eintrag>>,
}

impl Drossel {
    pub fn neu() -> Self {
        Self::default()
    }

    /// Wie lange noch gesperrt? `None` = der Versuch darf laufen.
    pub fn wartezeit(&self, schluessel: &str, jetzt: OffsetDateTime) -> Option<i64> {
        let karte = crate::sperre(&self.eintraege);
        let eintrag = karte.get(schluessel)?;
        let bis = eintrag.gesperrt_bis?;
        (bis > jetzt).then(|| (bis - jetzt).whole_seconds().max(1))
    }

    /// Einen Fehlversuch zählen; gibt die neue Wartezeit zurück (0 = noch frei).
    pub fn fehlschlag(&self, schluessel: &str, jetzt: OffsetDateTime) -> i64 {
        let mut karte = crate::sperre(&self.eintraege);
        let eintrag = karte.entry(schluessel.to_string()).or_insert(Eintrag {
            fehlversuche: 0,
            letzter: jetzt,
            gesperrt_bis: None,
        });
        // Nach einer langen Pause wieder bei fünf freien Versuchen beginnen.
        if jetzt - eintrag.letzter > time::Duration::minutes(15)
            && eintrag.gesperrt_bis.is_none_or(|bis| bis <= jetzt)
        {
            eintrag.fehlversuche = 0;
        }
        eintrag.fehlversuche = eintrag.fehlversuche.saturating_add(1);
        eintrag.letzter = jetzt;
        if eintrag.fehlversuche <= FREI {
            return 0;
        }
        let stufe = eintrag.fehlversuche - FREI;
        // 1, 2, 4, 8, 15, 15 … Minuten.
        let sekunden = (60i64 << (stufe - 1).min(4)).min(HOECHSTENS_SEKUNDEN);
        eintrag.gesperrt_bis = Some(jetzt + time::Duration::seconds(sekunden));
        sekunden
    }

    /// Erfolg: beide Zähler wieder auf Anfang.
    pub fn erfolg(&self, schluessel: &str) {
        crate::sperre(&self.eintraege).remove(schluessel);
    }

    /// Alte Einträge entfernen (damit die Karte nicht wächst).
    pub fn aufraeumen(&self, jetzt: OffsetDateTime) {
        let mut karte = crate::sperre(&self.eintraege);
        karte.retain(|_, eintrag| {
            eintrag.gesperrt_bis.is_some_and(|bis| bis > jetzt)
                || jetzt - eintrag.letzter < time::Duration::hours(24)
        });
    }
}

/// Schlüssel für „IP unbekannt“ (bei `oneshot` ohne Peer-Adresse).
pub fn ip_schluessel(ip: Option<std::net::IpAddr>) -> String {
    match ip {
        Some(ip) => format!("ip:{ip}"),
        None => String::from("ip:unbekannt"),
    }
}

/// Konto-Schlüssel; die E-Mail wird normalisiert.
pub fn konto_schluessel(email: &str) -> String {
    format!("konto:{}", email_normalisieren(email))
}

/// E-Mail normalisieren: ohne Leerzeichen, klein.
pub fn email_normalisieren(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zeit(sekunden: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000 + sekunden).unwrap()
    }

    #[test]
    fn fuenf_frei_dann_wachsend_bis_15_minuten() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        for i in 0..FREI {
            assert_eq!(drossel.wartezeit("a", jetzt), None, "Versuch {i}");
            assert_eq!(drossel.fehlschlag("a", jetzt), 0);
        }
        assert_eq!(drossel.fehlschlag("a", jetzt), 60);
        assert_eq!(
            drossel.wartezeit("a", jetzt + time::Duration::seconds(30)),
            Some(30)
        );
        // Während der Sperre zählt kein weiterer Versuch als frei.
        assert!(drossel.wartezeit("a", jetzt).is_some());
        // Weitere Fehlversuche verdoppeln, bis zur Obergrenze.
        assert_eq!(drossel.fehlschlag("a", jetzt), 120);
        assert_eq!(drossel.fehlschlag("a", jetzt), 240);
        assert_eq!(drossel.fehlschlag("a", jetzt), 480);
        assert_eq!(drossel.fehlschlag("a", jetzt), 900);
        assert_eq!(
            drossel.fehlschlag("a", jetzt),
            HOECHSTENS_SEKUNDEN,
            "nie länger als 15 Minuten"
        );
    }

    #[test]
    fn erfolg_setzt_zurueck() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        for _ in 0..FREI + 1 {
            drossel.fehlschlag("a", jetzt);
        }
        drossel.erfolg("a");
        assert_eq!(drossel.wartezeit("a", jetzt), None);
        assert_eq!(drossel.fehlschlag("a", jetzt), 0);
    }

    #[test]
    fn lange_pause_beginnt_wieder_bei_frei() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        for _ in 0..FREI + 1 {
            drossel.fehlschlag("a", jetzt);
        }
        let spaeter = jetzt + time::Duration::minutes(16);
        assert_eq!(drossel.fehlschlag("a", spaeter), 0);
    }

    #[test]
    fn konten_werden_normalisiert() {
        assert_eq!(
            konto_schluessel("  Root@Example.Test "),
            "konto:root@example.test"
        );
        assert_eq!(email_normalisieren("A@B.Test"), "a@b.test");
    }
}
