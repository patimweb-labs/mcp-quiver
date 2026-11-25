// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Nur eine Instanz je Gerät.
//!
//! Ein zweiter Start (Doppelklick im Dock, im Menü, in der Shell) soll nicht
//! zwei Kerne und zwei Tray-Symbole erzeugen, sondern das Fenster der
//! laufenden Instanz zeigen — und sich selbst beenden.
//!
//! Erkannt wird die laufende Instanz an einer **Sperrdatei** im Profil
//! (`<profil>/instanz.sperre`, Paket 3): wer sie sperren kann, ist die erste
//! Instanz. Daneben steht `<profil>/instanz.port` mit dem Port, auf dem sie
//! lauscht — der wird **frei gewählt** (Port 0) und ist nur zum Zeigen da,
//! nicht zum Erkennen. Der frühere Weg über „MCP-Port + 1“ war falsch: er
//! kollidierte mit einem anderen Programm auf diesem Port und sagte nichts
//! über das Profil.
//!
//! Der Port steht in einer **eigenen** Datei, nicht in der gesperrten: Windows
//! sperrt verpflichtend, und zwar für jede fremde Hand — auch im selben
//! Prozess. Wer in die gesperrte Datei schriebe oder sie läse, bekäme dort
//! eine Sperrverletzung; die zweite Instanz könnte den Port also nie lesen.
//!
//! Über den Port kommt genau eine Zeile Text („zeigen“), keine Token-Wache,
//! kein Zugriff auf Werkzeuge oder Daten. Je Verbindung gibt es eine Frist von
//! [`ANNAHME_FRIST`], und höchstens [`VERBINDUNGEN_MAX`] Verbindungen werden
//! gleichzeitig angenommen — wer den Port belegt, soll den Client nicht mit
//! offenen Verbindungen ausbremsen können.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::tray::TrayZustand;

/// So lange darf eine Verbindung auf ihre Anfrage warten.
pub const ANNAHME_FRIST: Duration = Duration::from_millis(500);
/// So viele Verbindungen werden gleichzeitig angenommen.
pub const VERBINDUNGEN_MAX: usize = 8;
/// Die Sperrdatei im Profil: sie ist die Kennung „hier läuft eine Instanz“.
pub const SPERRE: &str = "instanz.sperre";
/// Der Port der laufenden Instanz. Bewusst **nicht** gesperrt: nur so kommt
/// die zweite Instanz an ihn heran (siehe Modulkopf).
pub const PORT: &str = "instanz.port";

/// Was die zweite Instanz schickt.
const ANFORDERUNG: &str = "zeigen";

/// Der Wächter der laufenden Instanz. Er lebt so lange wie das Programm; sein
/// Faden horcht im Hintergrund auf Anfragen und schickt dann
/// [`crate::tray::Befehl::Zeigen`].
#[derive(Debug)]
pub struct Waechter {
    adresse: SocketAddr,
    /// Die Sperre des Profils – sie fällt mit dem Wächter.
    _sperre: std::fs::File,
}

impl Waechter {
    /// Die Sperrdatei belegen und den Horchfaden starten. Ein Fehler heißt:
    /// es läuft schon eine Instanz auf diesem Profil.
    pub fn belegen(profil: &Path, zustand: TrayZustand) -> std::io::Result<Self> {
        std::fs::create_dir_all(profil)?;
        let pfad = sperrpfad(profil);
        let datei = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&pfad)?;
        if datei.try_lock().is_err() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "es läuft schon eine Instanz für dieses Profil",
            ));
        }
        // Port frei wählen: nur zum Zeigen, nie zum Erkennen. Er landet in
        // einer eigenen Datei — die gesperrte bleibt unberührt (Modulkopf).
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let adresse = listener.local_addr()?;
        schreibe_port(&portpfad(profil), adresse.port())?;

        let offen = Arc::new(AtomicUsize::new(0));
        std::thread::spawn(move || {
            for strom in listener.incoming() {
                let Ok(strom) = strom else { continue };
                if offen.load(Ordering::SeqCst) >= VERBINDUNGEN_MAX {
                    // Zu viele gleichzeitig: sofort schließen, nicht warten.
                    continue;
                }
                offen.fetch_add(1, Ordering::SeqCst);
                let zustand = zustand.clone();
                let offen = offen.clone();
                std::thread::spawn(move || {
                    bedienen(strom, &zustand);
                    offen.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        Ok(Self {
            adresse,
            _sperre: datei,
        })
    }

    pub fn adresse(&self) -> SocketAddr {
        self.adresse
    }
}

/// Eine Anfrage bedienen: 500 ms Frist, dann zu.
fn bedienen(strom: TcpStream, zustand: &TrayZustand) {
    let _ = strom.set_read_timeout(Some(ANNAHME_FRIST));
    let _ = strom.set_write_timeout(Some(ANNAHME_FRIST));
    if anfrage_lesen(&strom).as_deref() == Some(ANFORDERUNG) {
        zustand.senden(crate::tray::Befehl::Zeigen);
        let _ = (&strom).write_all(b"ok\n");
    }
}

/// Der laufenden Instanz sagen: „Fenster zeigen“. `Ok`, wenn sie antwortet;
/// ein Fehler heißt: es läuft keine.
///
/// Der Port kommt aus der Sperrdatei. Fehlt sie oder ist sie frei, läuft
/// niemand – dann wird auch nichts angefasst.
pub fn zeigen_anfordern(profil: &Path) -> std::io::Result<()> {
    let pfad = sperrpfad(profil);
    if !pfad.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "keine Sperrdatei",
        ));
    }
    let datei = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pfad)?;
    match datei.try_lock() {
        // Free: es läuft keine Instanz.
        Ok(()) => {
            let _ = datei.unlock();
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "die Sperre ist frei",
            ))
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            let port = port_aus_datei(&portpfad(profil))?;
            zeigen_an(port)
        }
        Err(std::fs::TryLockError::Error(fehler)) => Err(fehler),
    }
}

/// Den Port aus der Portdatei lesen.
pub fn port_aus_sperre(profil: &Path) -> Option<u16> {
    port_aus_datei(&portpfad(profil)).ok()
}

fn zeigen_an(port: u16) -> std::io::Result<()> {
    let adresse = SocketAddr::from(([127, 0, 0, 1], port));
    let mut strom = TcpStream::connect_timeout(&adresse, Duration::from_secs(2))?;
    strom.set_read_timeout(Some(Duration::from_secs(2)))?;
    strom.set_write_timeout(Some(Duration::from_secs(2)))?;
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
    let mut zeile = String::new();
    BufReader::new(strom).read_line(&mut zeile).ok()?;
    Some(zeile.trim().to_string())
}

fn sperrpfad(profil: &Path) -> PathBuf {
    profil.join(SPERRE)
}

fn portpfad(profil: &Path) -> PathBuf {
    profil.join(PORT)
}

/// Den Port schreiben (nur den Port, sonst nichts). Über eine Zwischendatei
/// und ein Umbenennen: eine halb geschriebene Zahl läse niemand gern.
fn schreibe_port(pfad: &Path, port: u16) -> std::io::Result<()> {
    let zwischen = pfad.with_extension("port.neu");
    std::fs::write(&zwischen, format!("{port}\n"))?;
    if let Err(fehler) = std::fs::rename(&zwischen, pfad) {
        let _ = std::fs::remove_file(&zwischen);
        return Err(fehler);
    }
    Ok(())
}

fn port_aus_datei(pfad: &Path) -> std::io::Result<u16> {
    let text = std::fs::read_to_string(pfad)?;
    text.trim()
        .parse::<u16>()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "kein Port"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::{Befehl, TrayZustand};

    fn ordner() -> PathBuf {
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let pfad = std::env::temp_dir().join(format!(
            "quiver-fenster-instanz-{}-{stempel}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    fn zustand() -> (TrayZustand, std::sync::mpsc::Receiver<Befehl>) {
        let (sender, empfaenger) = std::sync::mpsc::channel();
        (TrayZustand::neu(sender), empfaenger)
    }

    #[test]
    fn zweite_instanz_weckt_die_erste() {
        let profil = ordner();
        let (zustand, empfaenger) = zustand();
        let waechter = Waechter::belegen(&profil, zustand).expect("Sperre frei");
        assert_ne!(waechter.adresse().port(), 0, "der Port steht fest");
        assert_eq!(
            port_aus_sperre(&profil),
            Some(waechter.adresse().port()),
            "der Port steht in der Portdatei"
        );

        assert!(zeigen_anfordern(&profil).is_ok());
        assert_eq!(
            empfaenger
                .recv_timeout(Duration::from_secs(2))
                .expect("Befehl angekommen"),
            Befehl::Zeigen
        );
        std::fs::remove_dir_all(&profil).ok();
    }

    #[test]
    fn die_sperrdatei_ist_das_erkennungszeichen() {
        let profil = ordner();
        let (zustand, _empfaenger) = zustand();
        let erster = Waechter::belegen(&profil, zustand.clone()).expect("Sperre frei");
        // Ein zweiter Wächter auf demselben Profil scheitert — daran erkennt
        // die zweite Instanz, dass sie nicht die erste ist. Der Port ist
        // dabei egal: auch ein anderer Port ändert nichts.
        let zweiter = Waechter::belegen(&profil, zustand);
        assert!(zweiter.is_err(), "dieselbe Sperrdatei zweimal belegt");
        assert_ne!(erster.adresse().port(), 0);
        std::fs::remove_dir_all(&profil).ok();
    }

    #[test]
    fn ohne_instanz_antwortet_niemand() {
        let profil = ordner();
        assert!(
            zeigen_anfordern(&profil).is_err(),
            "ohne Sperrdatei läuft keine Instanz"
        );
        // Eine freie Sperrdatei (niemand hält sie) ist auch keine Instanz.
        std::fs::write(portpfad(&profil), "65000\n").expect("schreibbar");
        std::fs::write(sperrpfad(&profil), "").expect("schreibbar");
        assert!(zeigen_anfordern(&profil).is_err());
        std::fs::remove_dir_all(&profil).ok();
    }

    /// Der Port steht in einer **eigenen** Datei, nicht in der gesperrten:
    /// unter Windows gilt eine Sperre für jede fremde Hand (auch im selben
    /// Prozess), ein Port darin wäre für die zweite Instanz unlesbar. Dass er
    /// **während** der Sperre lesbar ist, prüfen die Tests davor.
    #[test]
    fn der_port_steht_in_der_portdatei() {
        let profil = ordner();
        let (zustand, _empfaenger) = zustand();
        let waechter = Waechter::belegen(&profil, zustand).expect("Sperre frei");
        assert!(sperrpfad(&profil).is_file(), "die Sperrdatei gibt es");
        let text = std::fs::read_to_string(portpfad(&profil)).expect("lesbar");
        assert_eq!(
            text,
            format!("{}\n", waechter.adresse().port()),
            "in der Portdatei steht nur der Port"
        );
        std::fs::remove_dir_all(&profil).ok();
    }

    #[test]
    fn fremder_text_zeigt_kein_fenster() {
        let profil = ordner();
        let (zustand, empfaenger) = zustand();
        let waechter = Waechter::belegen(&profil, zustand).expect("Sperre frei");
        let mut strom = TcpStream::connect(waechter.adresse()).expect("verbunden");
        strom.write_all(b"irgendwas\n").expect("schreibbar");
        assert!(
            empfaenger.recv_timeout(Duration::from_millis(300)).is_err(),
            "nur „zeigen“ öffnet das Fenster"
        );
        std::fs::remove_dir_all(&profil).ok();
    }

    /// Eine Verbindung, die nichts schickt, wird nach [`ANNAHME_FRIST`]
    /// geschlossen – der Wächter wartet nicht ewig.
    #[test]
    fn eine_stumme_verbindung_haelt_den_waechter_nicht_auf() {
        let profil = ordner();
        let (zustand, empfaenger) = zustand();
        let waechter = Waechter::belegen(&profil, zustand).expect("Sperre frei");
        let stumm = TcpStream::connect(waechter.adresse()).expect("verbunden");
        std::thread::sleep(ANNAHME_FRIST + Duration::from_millis(200));
        // Danach geht eine echte Anfrage weiterhin durch.
        assert!(zeigen_anfordern(&profil).is_ok());
        assert_eq!(
            empfaenger
                .recv_timeout(Duration::from_secs(2))
                .expect("Befehl angekommen"),
            Befehl::Zeigen
        );
        drop(stumm);
        std::fs::remove_dir_all(&profil).ok();
    }
}
