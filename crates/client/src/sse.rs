// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Server-sent Events (SSE) selbst lesen.
//!
//! Der Server schickt auf `/geraet/ereignisse` Zeilen der Form `event: soll`
//! und `data: <revision>`; eine Leerzeile beendet ein Ereignis. `:`-Zeilen
//! sind Kommentare (Keep-Alive) und werden übergangen.
//!
//! Der Leser nimmt beliebig zerteilte Stücke an, weil ein HTTP-Rumpf an jeder
//! Byte-Grenze ankommen darf. Eine Zeile darf höchstens [`ZEILE_MAX`] Byte
//! lang sein: ein Server, der eine riesige Zeile ohne Zeilenende schickt, soll
//! den Speicher des Clients nicht füllen. Längere Zeilen werden verworfen und
//! gezählt.

/// So lang darf eine Zeile eines Ereignis-Stroms höchstens sein.
pub const ZEILE_MAX: usize = 64 * 1024;

/// Ein Ereignis aus dem Strom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ereignis {
    pub name: String,
    pub daten: String,
}

/// Der Zustand des Lesers: was schon gepuffert ist und was zum laufenden
/// Ereignis gehört.
#[derive(Debug, Default)]
pub struct Leser {
    puffer: String,
    name: String,
    daten: Vec<String>,
    /// Wie viele Zeilen die Längengrenze überschritten haben.
    verworfen: u64,
    /// Wird die zu lange Zeile gerade verworfen (bis zum Zeilenende)?
    im_verwerfen: bool,
}

impl Leser {
    /// Wie viele Zeilen die Längengrenze überschritten haben.
    pub fn verworfene_zeilen(&self) -> u64 {
        self.verworfen
    }

    /// Ein Stück Text einspeisen; gibt die Ereignisse zurück, die damit fertig sind.
    pub fn schieben(&mut self, stueck: &str) -> Vec<Ereignis> {
        self.puffer.push_str(stueck);
        let mut fertig = Vec::new();
        while let Some(ende) = self.puffer.find('\n') {
            let mut zeile = self.puffer[..ende].to_string();
            self.puffer.drain(..=ende);
            if self.im_verwerfen {
                // Der Rest einer zu langen Zeile: bis zum Zeilenende vergessen.
                self.im_verwerfen = false;
                continue;
            }
            if zeile.len() > ZEILE_MAX {
                // Eine Zeile, die in einem Stück kam und zu lang ist.
                self.verworfen = self.verworfen.saturating_add(1);
                continue;
            }
            if zeile.ends_with('\r') {
                zeile.pop();
            }
            self.zeile(&zeile, &mut fertig);
        }
        if self.puffer.len() > ZEILE_MAX {
            // Kein Zeilenende in Sicht: wegwerfen, nicht weiter sammeln.
            self.puffer.clear();
            self.im_verwerfen = true;
            self.verworfen = self.verworfen.saturating_add(1);
        }
        fertig
    }

    fn zeile(&mut self, zeile: &str, fertig: &mut Vec<Ereignis>) {
        if zeile.is_empty() {
            // Leerzeile: Ereignis abschließen, wenn eins angefangen wurde.
            if self.daten.is_empty() && self.name.is_empty() {
                return;
            }
            let name = if self.name.is_empty() {
                "message".to_string()
            } else {
                std::mem::take(&mut self.name)
            };
            let daten = self.daten.join("\n");
            self.daten.clear();
            self.name.clear();
            fertig.push(Ereignis { name, daten });
            return;
        }
        if zeile.starts_with(':') {
            return;
        }
        let (feld, wert) = match zeile.split_once(':') {
            Some((feld, wert)) => (feld, wert.strip_prefix(' ').unwrap_or(wert)),
            None => (zeile, ""),
        };
        match feld {
            "event" => self.name = wert.to_string(),
            "data" => self.daten.push(wert.to_string()),
            // `id`, `retry` und Unbekanntes brauchen wir nicht.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn einfaches_ereignis() {
        let mut leser = Leser::default();
        assert_eq!(leser.schieben("event: soll\ndata: abc\n\n").len(), 1);
        // Der zweite Aufruf ist leer, weil das Ereignis schon fertig ist.
        assert!(leser.schieben("").is_empty());
    }

    #[test]
    fn zerteilte_stuecke() {
        let mut leser = Leser::default();
        assert!(leser.schieben("event: so").is_empty());
        assert!(leser.schieben("ll\ndata: ").is_empty());
        assert!(leser.schieben("rev-1\n").is_empty());
        let fertig = leser.schieben("\n");
        assert_eq!(fertig.len(), 1);
        assert_eq!(fertig[0].name, "soll");
        assert_eq!(fertig[0].daten, "rev-1");
    }

    #[test]
    fn crlf_und_keepalive() {
        let mut leser = Leser::default();
        let fertig = leser.schieben(": keep-alive\r\nevent: soll\r\ndata: r\r\n\r\n");
        assert_eq!(fertig.len(), 1);
        assert_eq!(fertig[0].name, "soll");
        assert_eq!(fertig[0].daten, "r");
    }

    #[test]
    fn mehrzeilige_daten() {
        let mut leser = Leser::default();
        let fertig = leser.schieben("event: soll\ndata: a\ndata: b\n\n");
        assert_eq!(fertig.len(), 1);
        assert_eq!(fertig[0].daten, "a\nb");
    }

    #[test]
    fn zwei_ereignisse_in_einem_stueck() {
        let mut leser = Leser::default();
        let fertig = leser.schieben("event: soll\ndata: 1\n\nevent: soll\ndata: 2\n\n");
        assert_eq!(fertig.len(), 2);
        assert_eq!(
            (fertig[0].name.as_str(), fertig[0].daten.as_str()),
            ("soll", "1")
        );
        assert_eq!(
            (fertig[1].name.as_str(), fertig[1].daten.as_str()),
            ("soll", "2")
        );
    }

    #[test]
    fn leere_abschnitte_zaehlen_nicht() {
        let mut leser = Leser::default();
        assert!(leser.schieben("\n\n\n").is_empty());
    }

    #[test]
    fn ohne_event_namen_ist_es_message() {
        let mut leser = Leser::default();
        let fertig = leser.schieben("data: x\n\n");
        assert_eq!(fertig.len(), 1);
        assert_eq!(fertig[0].name, "message");
    }

    /// Eine Zeile über 64 KB wird verworfen und gezählt – der Speicher wächst
    /// nicht mit (Paket 3).
    #[test]
    fn eine_zu_lange_zeile_wird_verworfen() {
        let mut leser = Leser::default();
        let lang = format!("data: {}\n\n", "x".repeat(ZEILE_MAX + 100));
        assert!(leser.schieben(&lang).is_empty(), "kein Ereignis");
        assert_eq!(leser.verworfene_zeilen(), 1);
        // Danach wieder normale Ereignisse.
        let fertig = leser.schieben("event: soll\ndata: r\n\n");
        assert_eq!(fertig.len(), 1);
        assert_eq!(fertig[0].daten, "r");
    }

    /// Auch in vielen kleinen Stücken wächst der Puffer nicht über die Grenze.
    #[test]
    fn der_puffer_waechst_nicht_ueber_die_grenze() {
        let mut leser = Leser::default();
        let stueck = "y".repeat(1024);
        for _ in 0..(ZEILE_MAX / 1024 + 2) {
            assert!(leser.schieben(&stueck).is_empty());
        }
        assert_eq!(leser.verworfene_zeilen(), 1);
    }
}
