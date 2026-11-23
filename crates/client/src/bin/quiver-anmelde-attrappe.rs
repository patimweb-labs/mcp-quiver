// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Eine Attrappe für die Anmeldung eines Eintrags (Aufgabe 10).
//!
//! Sie steht für ein Werkzeug wie quiver-m365: **ein** Programm mit
//! Unterbefehlen. `mcp` startet den MCP-Server über stdin/stdout, die anderen
//! drei Befehle steuern die Anmeldung. Der Zustand liegt in einer Datei;
//! Aufgabe 10 verlangt, dass alle Befehle dasselbe Programm mit derselben
//! Umgebung sind, deshalb kommt der Pfad der Datei aus
//! `QUIVER_ATTRAPPE_DATEI` – also aus der Umgebung des Eintrags.
//!
//! ```text
//! QUIVER_ATTRAPPE_DATEI=/tmp/stand quiver-anmelde-attrappe status
//! QUIVER_ATTRAPPE_DATEI=/tmp/stand quiver-anmelde-attrappe anmelden test@beispiel.de
//! QUIVER_ATTRAPPE_DATEI=/tmp/stand quiver-anmelde-attrappe abmelden
//! QUIVER_ATTRAPPE_MCP=/pfad/quiver-demo-mcp quiver-anmelde-attrappe mcp
//! ```
//!
//! - `status`: angemeldet → Konto ausgeben, Exit 0; sonst Exit 1.
//! - `anmelden [konto]`: schreibt das Konto in die Datei, Exit 0.
//! - `abmelden`: leert die Datei, Exit 0.
//! - `mcp`: reicht stdin/stdout an den MCP-Server aus `QUIVER_ATTRAPPE_MCP`
//!   weiter – so kann ein Test den Neustart an dessen Protokoll abzählen.
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
        "kaputt" => {
            eprintln!("kaputt");
            ExitCode::from(3)
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
