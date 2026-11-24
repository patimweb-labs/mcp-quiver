// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Drossel gegen Rateversuche: 5 frei, danach wächst die Wartezeit bis 15
//! Minuten. Gilt je IP **und** je Konto, nie dauerhaft. Die Prüfung passiert
//! **vor** dem Vergleich von Passwort oder Code.
//!
//! Die Karte ist begrenzt ([`MAX_EINTRAEGE`]) und wird in Schritten aufgeräumt:
//! sie darf weder unbegrenzt wachsen noch bei jedem Aufruf die ganze Karte
//! durchgehen.

use std::collections::HashMap;
use std::sync::Mutex;

use time::OffsetDateTime;

/// So viele Versuche sind frei.
pub const FREI: u32 = 5;
/// Längste Sperre.
pub const HOECHSTENS_SEKUNDEN: i64 = 15 * 60;
/// So viele Einträge darf die Karte höchstens halten.
pub const MAX_EINTRAEGE: usize = 50_000;
/// So viele Einträge räumt ein Durchgang höchstens weg.
pub const AUFRAEUM_SCHRITT: usize = 1_000;
/// So viele Startversuche sind je Adresse frei (Anmelde-Start, Punkt 1).
pub const FREI_START: u32 = 30;

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
        self.fehlschlag_bis(schluessel, jetzt, None)
    }

    /// Wie [`Drossel::fehlschlag`], aber mit einer Obergrenze für die Wartezeit.
    ///
    /// Gebraucht für die Konto-Drossel: wenn die Adresse des Aufrufers nicht
    /// unterscheidbar ist (etwa hinter der Docker-Brücke), darf eine
    /// Kontosperre den Besitzer nicht für eine Viertelstunde aussperren.
    pub fn fehlschlag_bis(
        &self,
        schluessel: &str,
        jetzt: OffsetDateTime,
        hoechstens: Option<i64>,
    ) -> i64 {
        let mut karte = crate::sperre(&self.eintraege);
        if !karte.contains_key(schluessel) && karte.len() >= MAX_EINTRAEGE {
            verdraengen(&mut karte, jetzt);
        }
        let eintrag = karte.entry(schluessel.to_string()).or_insert(Eintrag {
            fehlversuche: 0,
            letzter: jetzt,
            gesperrt_bis: None,
        });
        zaehlen(eintrag, jetzt, FREI, hoechstens)
    }

    /// Einen Zugriff zählen (Anmelde-Start). `Some(sekunden)` heißt: warten.
    ///
    /// Anders als [`Drossel::fehlschlag`] ist der erste Zugriff kein Fehler:
    /// bis [`FREI_START`] passiert nichts, danach wächst die Wartezeit wie bei
    /// den Fehlversuchen.
    pub fn treffer(&self, schluessel: &str, jetzt: OffsetDateTime) -> Option<i64> {
        let mut karte = crate::sperre(&self.eintraege);
        if !karte.contains_key(schluessel) && karte.len() >= MAX_EINTRAEGE {
            verdraengen(&mut karte, jetzt);
        }
        let eintrag = karte.entry(schluessel.to_string()).or_insert(Eintrag {
            fehlversuche: 0,
            letzter: jetzt,
            gesperrt_bis: None,
        });
        let sekunden = zaehlen(eintrag, jetzt, FREI_START, None);
        (sekunden > 0).then_some(sekunden)
    }

    /// Erfolg: beide Zähler wieder auf Anfang.
    pub fn erfolg(&self, schluessel: &str) {
        crate::sperre(&self.eintraege).remove(schluessel);
    }

    /// Alte Einträge entfernen (damit die Karte nicht wächst).
    ///
    /// Höchstens [`AUFRAEUM_SCHRITT`] je Aufruf: die Karte liegt hinter einer
    /// Sperre, ein voller Durchgang bei 50 000 Einträgen würde alles andere
    /// aufhalten. Der Hintergrundlauf ruft so lange, bis nichts mehr weggeht.
    /// Gibt zurück, wie viele Einträge entfernt wurden.
    pub fn aufraeumen(&self, jetzt: OffsetDateTime) -> usize {
        let mut karte = crate::sperre(&self.eintraege);
        let mut weg = Vec::new();
        for (schluessel, eintrag) in karte.iter() {
            if eintrag.gesperrt_bis.is_some_and(|bis| bis > jetzt)
                || jetzt - eintrag.letzter < time::Duration::hours(24)
            {
                continue;
            }
            weg.push(schluessel.clone());
            if weg.len() >= AUFRAEUM_SCHRITT {
                break;
            }
        }
        for schluessel in &weg {
            karte.remove(schluessel);
        }
        weg.len()
    }

    /// Wie viele Einträge stehen gerade in der Karte?
    #[cfg(test)]
    pub fn anzahl(&self) -> usize {
        crate::sperre(&self.eintraege).len()
    }
}

/// Einen Zähler erhöhen und die Wartezeit bestimmen. `max` begrenzt die Sperre.
fn zaehlen(eintrag: &mut Eintrag, jetzt: OffsetDateTime, frei: u32, max: Option<i64>) -> i64 {
    // Nach einer langen Pause wieder bei den freien Versuchen beginnen.
    if jetzt - eintrag.letzter > time::Duration::minutes(15)
        && eintrag.gesperrt_bis.is_none_or(|bis| bis <= jetzt)
    {
        eintrag.fehlversuche = 0;
    }
    eintrag.fehlversuche = eintrag.fehlversuche.saturating_add(1);
    eintrag.letzter = jetzt;
    if eintrag.fehlversuche <= frei {
        return 0;
    }
    let stufe = eintrag.fehlversuche - frei;
    // 1, 2, 4, 8, 15, 15 … Minuten.
    let mut sekunden = (60i64 << (stufe - 1).min(4)).min(HOECHSTENS_SEKUNDEN);
    if let Some(max) = max {
        sekunden = sekunden.min(max);
    }
    eintrag.gesperrt_bis = Some(jetzt + time::Duration::seconds(sekunden));
    sekunden
}

/// Den ältesten Eintrag verdrängen, der nicht gesperrt ist; ist jeder gesperrt,
/// den ältesten überhaupt (die Grenze ist wichtiger als die Sperre).
fn verdraengen(karte: &mut HashMap<String, Eintrag>, jetzt: OffsetDateTime) {
    let ohne_sperre = karte
        .iter()
        .filter(|(_, eintrag)| eintrag.gesperrt_bis.is_none_or(|bis| bis <= jetzt))
        .min_by_key(|(_, eintrag)| eintrag.letzter)
        .map(|(schluessel, _)| schluessel.clone());
    let opfer = ohne_sperre.or_else(|| {
        karte
            .iter()
            .min_by_key(|(_, eintrag)| eintrag.letzter)
            .map(|(schluessel, _)| schluessel.clone())
    });
    if let Some(opfer) = opfer {
        karte.remove(&opfer);
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

    /// Eine Obergrenze für die Wartezeit: die Konto-Drossel bleibt bei 60 s.
    #[test]
    fn wartezeit_laesst_sich_begrenzen() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        for _ in 0..FREI {
            assert_eq!(drossel.fehlschlag_bis("k", jetzt, Some(60)), 0);
        }
        assert_eq!(drossel.fehlschlag_bis("k", jetzt, Some(60)), 60);
        assert_eq!(drossel.fehlschlag_bis("k", jetzt, Some(60)), 60);
        // Ohne Grenze wüchse sie weiter.
        assert_eq!(drossel.fehlschlag("k2", jetzt), 0);
    }

    /// Der Anmelde-Start hat mehr freie Versuche als ein Passwortversuch.
    #[test]
    fn treffer_zaehlt_zugriffe() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        for _ in 0..FREI_START {
            assert_eq!(drossel.treffer("start", jetzt), None);
        }
        assert_eq!(drossel.treffer("start", jetzt), Some(60));
        assert_eq!(drossel.wartezeit("start", jetzt), Some(60));
    }

    /// Die Karte ist begrenzt und verdrängt den ältesten Eintrag.
    #[test]
    fn karte_ist_begrenzt() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        // Ohne Sperre: die ältesten Plätze werden frei.
        for i in 0..MAX_EINTRAEGE {
            drossel.fehlschlag(&format!("k{i}"), jetzt);
        }
        assert_eq!(drossel.anzahl(), MAX_EINTRAEGE);
        drossel.fehlschlag("neu", jetzt);
        assert_eq!(drossel.anzahl(), MAX_EINTRAEGE, "die Grenze hält");
        assert_eq!(drossel.wartezeit("k0", jetzt), None, "der älteste ist weg");
        assert!(drossel.anzahl() <= MAX_EINTRAEGE);
    }

    /// Aufgeräumt wird in Schritten, nicht alles auf einmal.
    #[test]
    fn aufraeumen_geht_in_schritten() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        for i in 0..(AUFRAEUM_SCHRITT + 10) {
            drossel.fehlschlag(&format!("k{i}"), jetzt);
        }
        let spaeter = jetzt + time::Duration::hours(25);
        assert_eq!(drossel.aufraeumen(spaeter), AUFRAEUM_SCHRITT);
        assert_eq!(drossel.anzahl(), 10);
        assert_eq!(drossel.aufraeumen(spaeter), 10);
        assert_eq!(drossel.anzahl(), 0);
    }

    /// Ein gesperrter Eintrag überlebt das Aufräumen.
    #[test]
    fn gesperrtes_bleibt_beim_aufraeumen() {
        let drossel = Drossel::neu();
        let jetzt = zeit(0);
        for _ in 0..FREI + 1 {
            drossel.fehlschlag("gesperrt", jetzt);
        }
        let spaeter = jetzt + time::Duration::seconds(30);
        assert_eq!(drossel.aufraeumen(spaeter), 0);
        assert!(drossel.wartezeit("gesperrt", spaeter).is_some());
    }
}
