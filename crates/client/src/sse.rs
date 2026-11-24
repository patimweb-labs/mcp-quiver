// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Server-sent Events (SSE) selbst lesen.
//!
//! Der Server schickt auf `/geraet/ereignisse` Zeilen der Form `event: soll`
//! und `data: <revision>`; eine Leerzeile beendet ein Ereignis. `:`-Zeilen
//! sind Kommentare (Keep-Alive) und werden übergangen.
//!
//! Der Leser nimmt beliebig zerteilte Stücke an, weil ein HTTP-Rumpf an jeder
//! Byte-Grenze ankommen darf.

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
}

impl Leser {
    /// Ein Stück Text einspeisen; gibt die Ereignisse zurück, die damit fertig sind.
    pub fn schieben(&mut self, stueck: &str) -> Vec<Ereignis> {
        self.puffer.push_str(stueck);
        let mut fertig = Vec::new();
        while let Some(ende) = self.puffer.find('\n') {
            let mut zeile = self.puffer[..ende].to_string();
            self.puffer.drain(..=ende);
            if zeile.ends_with('\r') {
                zeile.pop();
            }
            self.zeile(&zeile, &mut fertig);
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
}
