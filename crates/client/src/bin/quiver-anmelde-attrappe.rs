// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eine Attrappe für die Anmeldung und die Einrichtung eines Eintrags
//! (Aufgaben 10 und 11).
//!
//! Sie steht für ein Werkzeug wie quiver-m365: **ein** Programm mit
//! Unterbefehlen. `mcp` startet den MCP-Server über stdin/stdout, die anderen
//! Befehle steuern Anmeldung und Einrichtung. Der Zustand liegt in einer Datei;
//! Aufgabe 10 verlangt, dass alle Befehle dasselbe Programm mit derselben
//! Umgebung sind, deshalb kommt der Pfad der Datei aus
//! `QUIVER_ATTRAPPE_DATEI` – also aus der Umgebung des Eintrags.
//!
//! ```text
//! QUIVER_ATTRAPPE_DATEI=/tmp/stand quiver-anmelde-attrappe status
//! QUIVER_ATTRAPPE_DATEI=/tmp/stand quiver-anmelde-attrappe anmelden test@beispiel.de
//! QUIVER_ATTRAPPE_DATEI=/tmp/stand quiver-anmelde-attrappe abmelden
//! quiver-anmelde-attrappe einrichten /tmp/einrichtung.stand
//! quiver-anmelde-attrappe einrichten abbruch
//! QUIVER_ATTRAPPE_MCP=/pfad/quiver-demo-mcp quiver-anmelde-attrappe mcp
//! ```
//!
//! - `status`: angemeldet → Konto ausgeben, Exit 0; sonst Exit 1.
//! - `anmelden [konto]`: schreibt das Konto in die Datei, Exit 0.
//! - `abmelden`: leert die Datei, Exit 0.
//! - `einrichten <datei>`: schreibt „eingerichtet“ in die Datei, Exit 0 – so
//!   wie ein Tool, das die Seite im Browser öffnet und danach Profile hat.
//!   `einrichten abbruch`: Exit 1 (der User hat die Seite geschlossen).
//!   Steht `QUIVER_ATTRAPPE_WARTE_AUF` in der Umgebung, wartet der Befehl
//!   vorher auf diese Datei – damit ein Test den Zustand „läuft“ festhalten
//!   und den zweiten Klick prüfen kann.
//! - `mcp`: reicht stdin/stdout an den MCP-Server aus `QUIVER_ATTRAPPE_MCP`
//!   weiter – so kann ein Test den Neustart an dessen Protokoll abzählen.
//!   Steht `QUIVER_ZUGAENGE_DATEI` in der Umgebung, schreibt sie vorher den
//!   Inhalt von `QUIVER_ZUGAENGE` dorthin (Aufgabe 14: was das Tool beim
//!   Start sieht).
//! - `zugaenge <datei>`: schreibt `QUIVER_ZUGAENGE` (oder `[]`) in die Datei
//!   und endet, Exit 0 – derselbe Blick auf die Umgebung ohne MCP-Server.
//! - `warten <ms>`: wartet so viele Millisekunden und endet mit 0 – so sieht
//!   eine Einrichtung aus, die auf den User wartet (Paket 3: der Abbruch muss
//!   sie beenden können).
//! - `kaputt`: Exit 3 – für den Fehlerfall im Test.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut argumente = std::env::args().skip(1);
    let befehl = argumente.next().unwrap_or_default();
    let konto = argumente.next().unwrap_or_default();

    let datei = match std::env::var("QUIVER_ATTRAPPE_DATEI") {
        Ok(pfad) if !pfad.trim().is_empty() => pfad,
        _ => {
            eprintln!("QUIVER_ATTRAPPE_DATEI fehlt");
            return ExitCode::from(2);
        }
    };

    match befehl.as_str() {
        "mcp" => mcp(),
        "status" => {
            let angemeldet = std::fs::read_to_string(&datei)
                .unwrap_or_default()
                .trim()
                .to_string();
            if angemeldet.is_empty() {
                println!("nicht angemeldet");
                ExitCode::from(1)
            } else {
                println!("{angemeldet}");
                ExitCode::SUCCESS
            }
        }
        "anmelden" => {
            if schreiben(&datei, konto.trim()) {
                println!("angemeldet");
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            }
        }
        "abmelden" => {
            if schreiben(&datei, "") {
                println!("abgemeldet");
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            }
        }
        // Einrichten (Aufgabe 11): die Seite im Browser öffnen und danach
        // Profile haben. Hier steht dafür eine Datei.
        "einrichten" => {
            if konto.trim() == "abbruch" {
                println!("abgebrochen");
                return ExitCode::from(1);
            }
            if konto.trim().is_empty() {
                eprintln!("einrichten braucht eine Datei");
                return ExitCode::from(2);
            }
            if !warten_erlaubt() {
                return ExitCode::from(2);
            }
            if schreiben(&konto, "eingerichtet") {
                println!("eingerichtet");
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            }
        }
        // Eine Einrichtung, die auf den User wartet.
        "warten" => {
            let ms = konto.trim().parse::<u64>().unwrap_or(0);
            std::thread::sleep(std::time::Duration::from_millis(ms));
            println!("gewartet");
            ExitCode::SUCCESS
        }
        "kaputt" => {
            eprintln!("kaputt");
            ExitCode::from(3)
        }
        // Die Zugänge, wie das Tool sie beim Start sieht (Aufgabe 14).
        "zugaenge" => {
            if konto.trim().is_empty() {
                eprintln!("zugaenge braucht eine Datei");
                return ExitCode::from(2);
            }
            match schreiben(&konto, &zugaenge_env()) {
                true => ExitCode::SUCCESS,
                false => ExitCode::from(2),
            }
        }
        _ => {
            eprintln!("unbekannter Befehl: {befehl}");
            ExitCode::from(2)
        }
    }
}

/// Den MCP-Server aus der Umgebung übernehmen: stdin und stdout bleiben die
/// Leitungen des Clients, der Prozess ersetzt so den eigentlichen Server.
fn mcp() -> ExitCode {
    // Was das Tool beim Start sieht: erst die Zugänge festhalten, dann den
    // MCP-Server übernehmen (Aufgabe 14).
    if let Ok(datei) = std::env::var("QUIVER_ZUGAENGE_DATEI") {
        if !datei.trim().is_empty() {
            schreiben(&datei, &zugaenge_env());
        }
    }
    let programm = match std::env::var("QUIVER_ATTRAPPE_MCP") {
        Ok(pfad) if !pfad.trim().is_empty() => pfad,
        _ => {
            eprintln!("QUIVER_ATTRAPPE_MCP fehlt");
            return ExitCode::from(2);
        }
    };
    match std::process::Command::new(&programm).status() {
        Ok(status) => ExitCode::from(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1)),
        Err(fehler) => {
            eprintln!("{programm} lässt sich nicht starten: {fehler}");
            ExitCode::from(2)
        }
    }
}

/// Auf eine Datei warten, wenn der Test das verlangt (Aufgabe 11): so lässt
/// sich der Zustand „läuft“ festhalten. Ohne die Variable geht es sofort
/// weiter; `false`, wenn die Frist abläuft.
fn warten_erlaubt() -> bool {
    let Ok(datei) = std::env::var("QUIVER_ATTRAPPE_WARTE_AUF") else {
        return true;
    };
    if datei.trim().is_empty() {
        return true;
    }
    let frist = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !std::path::Path::new(&datei).exists() {
        if std::time::Instant::now() > frist {
            eprintln!("{datei} kam nicht");
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    true
}

/// Der Inhalt von `QUIVER_ZUGAENGE`; ohne die Variable eine leere Liste.
fn zugaenge_env() -> String {
    match std::env::var("QUIVER_ZUGAENGE") {
        Ok(wert) if !wert.trim().is_empty() => wert,
        _ => "[]".to_string(),
    }
}

/// Den Zustand ablegen; `false`, wenn die Datei nicht schreibbar ist.
fn schreiben(datei: &str, inhalt: &str) -> bool {
    match std::fs::write(datei, format!("{inhalt}\n")) {
        Ok(()) => true,
        Err(fehler) => {
            eprintln!("{datei} nicht schreibbar: {fehler}");
            false
        }
    }
}
