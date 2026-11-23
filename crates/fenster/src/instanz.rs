// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Nur eine Instanz je Gerät.
//!
//! Ein zweiter Start (Doppelklick im Dock, im Menü, in der Shell) soll nicht
//! zwei Kerne und zwei Tray-Symbole erzeugen, sondern das Fenster der
//! laufenden Instanz zeigen — und sich selbst beenden.
//!
//! Erkannt wird die laufende Instanz an einem eigenen, winzigen Endpunkt auf
//! `127.0.0.1:<MCP-Port + 1>`. Er kann **nur** „Fenster zeigen": eine Zeile
//! Text, keine Token-Wache, kein Zugriff auf Werkzeuge oder Daten. Wer den Port
//! belegt, hat die Instanz; wer ihn belegt vorfindet, fragt dort höflich an.
//! (Der MCP-Endpunkt selbst taugt dafür nicht: er verlangt ein Agent-Token,
//! und die Anwendung soll sich nicht selbst als Agent ausgeben.)

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use crate::tray::TrayZustand;

/// Abstand zwischen MCP-Port und dem „Fenster zeigen"-Port.
pub const ABSTAND: u16 = 1;

/// Was die zweite Instanz schickt.
const ANFORDERUNG: &str = "zeigen";

/// Der Port, über den eine laufende Instanz ihr Fenster zeigen lässt.
pub fn adresse(mcp: SocketAddr) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], mcp.port().wrapping_add(ABSTAND)))
}

/// Der Wächter der laufenden Instanz. Er lebt so lange wie das Programm; sein
/// Faden horcht im Hintergrund auf Anfragen und schickt dann
/// [`crate::tray::Befehl::Zeigen`].
#[derive(Debug)]
pub struct Waechter {
    adresse: SocketAddr,
}

impl Waechter {
    /// Den Port belegen und den Faden starten. Ein Fehler (`AddrInUse`) heißt:
    /// es läuft schon eine Instanz.
    pub fn belegen(adresse: SocketAddr, zustand: TrayZustand) -> std::io::Result<Self> {
        let listener = TcpListener::bind(adresse)?;
        let adresse = listener.local_addr()?;
        std::thread::spawn(move || {
            for strom in listener.incoming() {
                let Ok(strom) = strom else { continue };
                if anfrage_lesen(&strom) == Some(ANFORDERUNG.to_string()) {
                    zustand.senden(crate::tray::Befehl::Zeigen);
                    let _ = (&strom).write_all(b"ok\n");
                }
            }
        });
        Ok(Self { adresse })
    }

    pub fn adresse(&self) -> SocketAddr {
        self.adresse
    }
}

/// Der laufenden Instanz sagen: „Fenster zeigen". `Ok`, wenn sie antwortet;
/// ein Fehler heißt: es läuft keine.
pub fn zeigen_anfordern(adresse: SocketAddr) -> std::io::Result<()> {
    let mut strom = TcpStream::connect_timeout(&adresse, Duration::from_secs(2))?;
    strom.set_read_timeout(Some(Duration::from_secs(2)))?;
    strom.write_all(format!("{ANFORDERUNG}\n").as_bytes())?;
    strom.flush()?;
    // Die Antwort abwarten: erst dann hat die andere Instanz den Befehl.
    let mut antwort = String::new();
    let mut leser = BufReader::new(&strom);
    leser.read_line(&mut antwort)?;
    if antwort.trim() == "ok" {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "keine Antwort",
        ))
    }
}

fn anfrage_lesen(strom: &TcpStream) -> Option<String> {
    strom.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut zeile = String::new();
    BufReader::new(strom).read_line(&mut zeile).ok()?;
    Some(zeile.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::{Befehl, TrayZustand};

    fn freie_adresse() -> SocketAddr {
        // Port 0: das Betriebssystem vergibt einen freien Port.
        SocketAddr::from(([127, 0, 0, 1], 0))
    }

    fn zustand() -> (TrayZustand, std::sync::mpsc::Receiver<Befehl>) {
        let (sender, empfaenger) = std::sync::mpsc::channel();
        (TrayZustand::neu(sender), empfaenger)
    }

    #[test]
    fn zweite_instanz_weckt_die_erste() {
        let (zustand, empfaenger) = zustand();
        let waechter = Waechter::belegen(freie_adresse(), zustand).expect("Port frei");
        assert_ne!(waechter.adresse().port(), 0, "Port steht fest");

        assert!(zeigen_anfordern(waechter.adresse()).is_ok());
        assert_eq!(
            empfaenger
                .recv_timeout(Duration::from_secs(2))
                .expect("Befehl angekommen"),
            Befehl::Zeigen
        );
        // Jede weitere Anfrage kommt genauso an.
        assert!(zeigen_anfordern(waechter.adresse()).is_ok());
        assert_eq!(
            empfaenger
                .recv_timeout(Duration::from_secs(2))
                .expect("Befehl angekommen"),
            Befehl::Zeigen
        );
    }

    #[test]
    fn der_port_ist_das_erkennungszeichen() {
        let (zustand, _empfaenger) = zustand();
        let waechter = Waechter::belegen(freie_adresse(), zustand.clone()).expect("Port frei");
        // Ein zweiter Wächter auf demselben Port scheitert — daran erkennt die
        // zweite Instanz, dass sie nicht die erste ist.
        let zweiter = Waechter::belegen(waechter.adresse(), zustand);
        assert!(zweiter.is_err(), "derselbe Port zweimal belegt");
    }

    #[test]
    fn ohne_instanz_antwortet_niemand() {
        // Ein Port, auf dem nichts lauscht: Verbindung abgelehnt.
        let adresse = freie_adresse();
        let adresse = SocketAddr::from(([127, 0, 0, 1], adresse.port()));
        assert!(zeigen_anfordern(adresse).is_err());
    }

    #[test]
    fn der_port_liegt_neben_dem_mcp_port() {
        let mcp = SocketAddr::from(([127, 0, 0, 1], 8741));
        assert_eq!(adresse(mcp).port(), 8742);
        assert_eq!(adresse(mcp).ip().to_string(), "127.0.0.1");
    }

    #[test]
    fn fremder_text_zeigt_kein_fenster() {
        let (zustand, empfaenger) = zustand();
        let waechter = Waechter::belegen(freie_adresse(), zustand).expect("Port frei");
        let mut strom = TcpStream::connect(waechter.adresse()).expect("verbunden");
        strom.write_all(b"irgendwas\n").expect("schreibbar");
        assert!(
            empfaenger.recv_timeout(Duration::from_millis(300)).is_err(),
            "nur „zeigen“ öffnet das Fenster"
        );
    }
}
