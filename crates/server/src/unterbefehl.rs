// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Unterbefehle von `quiver-server` (Aufgabe 20). Sie laufen im Container und
//! benutzen **denselben** Prüfweg wie die API; die Prüfspur vermerkt
//! „Kommandozeile“. Der laufende Server bemerkt Änderungen über
//! `PRAGMA data_version` (siehe `beobachte_datenbank`).

use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
// Nur Unix: `stty` schaltet die Anzeige der Eingabe ab; Windows kennt es nicht.
#[cfg(unix)]
use std::process::Command;

use anyhow::Context;
use quiver_domain::{Eintrag, Satz};
use time::OffsetDateTime;

use crate::benutzer::{admin_setzen_auf_platte, root_setzen_auf_platte};
use crate::speicher;

/// Bekannte Unterbefehle (für die Hilfe).
pub const UNTERBEFEHLE: &[&str] = &[
    "root-setzen",
    "admin-setzen",
    "benutzer",
    "eintrag-setzen",
    "satz-setzen",
    "eintraege",
    "saetze",
    "entra-schluessel",
    "gesund",
];

/// Ist das erste Argument ein Unterbefehl?
pub fn ist_unterbefehl(args: &[String]) -> bool {
    args.first()
        .is_some_and(|erstes| UNTERBEFEHLE.contains(&erstes.as_str()))
}

fn daten() -> PathBuf {
    PathBuf::from(std::env::var("QUIVER_DATEN").unwrap_or_else(|_| String::from("./daten")))
}

fn datenbank() -> anyhow::Result<rusqlite::Connection> {
    let ordner = daten();
    crate::rechte::ordner_anlegen(&ordner)?;
    let conn = speicher::oeffnen(&ordner.join("quiver.db"))?;
    if let Err(fehler) = crate::rechte::daten_pruefen(&ordner) {
        tracing::warn!("Rechte in {} nicht prüfbar: {fehler}", ordner.display());
    }
    Ok(conn)
}

/// Ist der Aufruf der Gesundheitsweg (`gesund`)?
pub fn ist_gesund(args: &[String]) -> bool {
    args.len() == 1 && args[0] == "gesund"
}

/// `quiver-server gesund`: fragt den **eigenen** Server über Loopback.
///
/// Gedacht für `HEALTHCHECK` im Container: kein Netzzugriff nach außen, kein
/// Wissen über die öffentliche Adresse – nur `127.0.0.1` und der Port aus
/// `QUIVER_ADRESSE`. Antwortet der Server mit 200, ist alles gut.
pub async fn gesund() -> anyhow::Result<()> {
    let port = std::env::var("QUIVER_ADRESSE")
        .ok()
        .and_then(|adresse| adresse.parse::<std::net::SocketAddr>().ok())
        .map(|adresse| adresse.port())
        .unwrap_or(8740);
    let ziel = format!("http://127.0.0.1:{port}{}", crate::GESUND);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        // Kein Proxy aus der Umgebung: der Weg bleibt auf diesem Rechner.
        .no_proxy()
        .build()
        .context("Prüfclient baubar")?;
    let antwort = client
        .get(&ziel)
        .send()
        .await
        .with_context(|| format!("Server antwortet nicht auf {ziel}"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("Server antwortet auf {ziel} mit {}", antwort.status());
    }
    Ok(())
}

/// Führt einen Unterbefehl aus; `Err` bedeutet Exit-Code 1.
pub fn ausfuehren(args: &[String]) -> anyhow::Result<()> {
    let (befehl, rest) = args
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("kein Unterbefehl"))?;
    match befehl.as_str() {
        "root-setzen" => root_setzen(rest),
        "admin-setzen" => admin_setzen(rest),
        "benutzer" => benutzer_zeigen(),
        "eintrag-setzen" => eintrag_setzen(rest),
        "satz-setzen" => satz_setzen(rest),
        "eintraege" => eintraege_zeigen(),
        "saetze" => saetze_zeigen(),
        "entra-schluessel" => entra_schluessel(rest),
        anderes => anyhow::bail!("unbekannter Unterbefehl {anderes}"),
    }
}

fn wert(rest: &[String], name: &str) -> Option<String> {
    let mut i = 0;
    while i < rest.len() {
        if rest[i] == name {
            return rest.get(i + 1).cloned();
        }
        if let Some(wert) = rest[i].strip_prefix(&format!("{name}=")) {
            return Some(wert.to_string());
        }
        i += 1;
    }
    None
}

fn hat(rest: &[String], name: &str) -> bool {
    rest.iter().any(|wert| wert == name)
}

/// Erstes Argument, das nicht mit `--` beginnt.
fn position(rest: &[String]) -> Option<String> {
    rest.iter().find(|wert| !wert.starts_with("--")).cloned()
}

// ---------------------------------------------------------------- root-setzen

fn root_setzen(rest: &[String]) -> anyhow::Result<()> {
    let email = wert(rest, "--email")
        .or_else(|| position(rest))
        .ok_or_else(|| anyhow::anyhow!("Aufruf: quiver-server root-setzen <email> [--stdin]"))?;
    let passwort = if hat(rest, "--stdin") {
        let mut zeile = String::new();
        io::stdin()
            .read_line(&mut zeile)
            .context("Passwort aus der Eingabe lesen")?;
        zeile.trim_end_matches(['\n', '\r']).to_string()
    } else {
        zweimal_lesen()?
    };
    root_setzen_auf_platte(&daten(), &email, &passwort)?;
    println!("Root gesetzt für {email}; alle Root-Sitzungen sind beendet.");
    Ok(())
}

fn zweimal_lesen() -> anyhow::Result<String> {
    print!(
        "Passwort ({} bis {} Zeichen): ",
        crate::benutzer::PASSWORT_MIN,
        crate::benutzer::PASSWORT_MAX
    );
    io::stdout().flush()?;
    let erstes = verdeckt_lesen()?;
    print!("Passwort wiederholen: ");
    io::stdout().flush()?;
    let zweites = verdeckt_lesen()?;
    if erstes != zweites {
        anyhow::bail!("die beiden Passwörter stimmen nicht überein");
    }
    Ok(erstes)
}

/// Liest eine Zeile, ohne sie anzuzeigen.
///
/// Verbergen kann nur `stty`. Fehlt es (im distroless-Abbild ist es nicht
/// enthalten) und kommt die Eingabe von einem Terminal, bricht der Aufruf ab:
/// ein Passwort wird nicht sichtbar gelesen. Von einer Pipe oder Datei wird
/// ohnehin nichts angezeigt — dort geht es ohne `stty`, genau wie `--stdin`.
fn verdeckt_lesen() -> anyhow::Result<String> {
    let terminal = io::stdin().is_terminal();
    #[cfg(unix)]
    let stumm = terminal && schalte_echo(false);
    #[cfg(not(unix))]
    let stumm = false;

    let ergebnis = zeile_lesen(terminal, stumm, &mut io::stdin().lock());

    // Die Eingabe wieder anzeigen — auch wenn das Lesen scheiterte.
    #[cfg(unix)]
    if stumm {
        schalte_echo(true);
        eprintln!();
    }
    ergebnis
}

/// Schaltet die Anzeige der Eingabe an oder aus; `false`, wenn `stty` fehlt
/// oder scheitert.
#[cfg(unix)]
fn schalte_echo(an: bool) -> bool {
    Command::new("stty")
        .arg(if an { "echo" } else { "-echo" })
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Die eigentliche Entscheidung, prüfbar ohne Terminal: liegt die Eingabe auf
/// einem Terminal (`terminal`) und lässt sie sich dort **nicht** verbergen
/// (`stumm`), wird nichts gelesen.
fn zeile_lesen<R: std::io::BufRead>(
    terminal: bool,
    stumm: bool,
    quelle: &mut R,
) -> anyhow::Result<String> {
    if terminal && !stumm {
        anyhow::bail!(
            "Die Eingabe lässt sich hier nicht verbergen (kein `stty`); ein Passwort \
             wird nicht sichtbar gelesen. Übergabe über `--stdin`, z. B.\n  \
             read -rsp 'Neues Passwort: ' P; echo\n  \
             printf '%s\\n' \"$P\" | quiver-server root-setzen <email> --stdin\n  \
             unset P"
        );
    }
    let mut zeile = String::new();
    quelle.read_line(&mut zeile).context("Passwort lesen")?;
    Ok(zeile.trim_end_matches(['\n', '\r']).to_string())
}

// --------------------------------------------------------------- admin-setzen

fn admin_setzen(rest: &[String]) -> anyhow::Result<()> {
    let oid = wert(rest, "--oid")
        .or_else(|| position(rest))
        .ok_or_else(|| anyhow::anyhow!("Aufruf: quiver-server admin-setzen --oid <objekt-id>"))?;
    admin_setzen_auf_platte(&daten(), &oid)?;
    println!("{oid} ist jetzt Admin.");
    Ok(())
}

// ------------------------------------------------------------ entra-schluessel

/// `quiver-server entra-schluessel [--erneuern|--umschalten]` (Aufgabe 23).
///
/// Gibt **nur** das öffentliche Zertifikat, den Fingerabdruck und den Ablauf
/// aus. Der private Schlüssel bleibt in `<daten>/entra-schluessel.pem` (0600)
/// und wird nie überschrieben.
fn entra_schluessel(rest: &[String]) -> anyhow::Result<()> {
    use crate::entra_schluessel::{
        erneuern, erzeugen, pfad_aus_umgebung, umschalten, EntraSchluessel, GUELTIG_TAGE,
    };

    if hat(rest, "--erneuern") && hat(rest, "--umschalten") {
        anyhow::bail!("entweder --erneuern oder --umschalten, nicht beides");
    }
    let ordner = daten();
    std::fs::create_dir_all(&ordner)?;
    let aktiv = pfad_aus_umgebung(&ordner);
    let jetzt = OffsetDateTime::now_utc();

    if hat(rest, "--erneuern") {
        let (pfad, schluessel) = erneuern(&aktiv, jetzt)?;
        ausgeben(&schluessel);
        println!(
            "Der zweite Schlüssel liegt in {} und ist noch nicht aktiv; \
             `quiver-server entra-schluessel --umschalten` macht ihn aktiv.",
            pfad.display()
        );
        return Ok(());
    }
    if hat(rest, "--umschalten") {
        let schluessel = umschalten(&aktiv)?;
        ausgeben(&schluessel);
        println!("Dieser Schlüssel ist jetzt aktiv (kein Neustart nötig).");
        return Ok(());
    }
    if aktiv.exists() {
        let schluessel = EntraSchluessel::laden(&aktiv)?;
        ausgeben(&schluessel);
        println!(
            "Der Schlüssel war schon da; nichts wurde geändert. \
             Für einen neuen: --erneuern, danach --umschalten."
        );
        return Ok(());
    }
    let schluessel = erzeugen(&aktiv, jetzt, GUELTIG_TAGE)?;
    ausgeben(&schluessel);
    Ok(())
}

fn ausgeben(schluessel: &crate::entra_schluessel::EntraSchluessel) {
    println!("Fingerabdruck (SHA-256): {}", schluessel.fingerabdruck());
    println!("Gültig bis: {}", schluessel.nicht_nach().date());
    println!("x5t#S256: {}", schluessel.x5t_s256());
    println!("Öffentliches Zertifikat (PEM):");
    print!("{}", schluessel.zertifikat_pem());
    if !schluessel.zertifikat_pem().ends_with('\n') {
        println!();
    }
}

// ------------------------------------------------------------------ Anzeigen

fn benutzer_zeigen() -> anyhow::Result<()> {
    let conn = datenbank()?;
    for benutzer in speicher::benutzer_liste(&conn)? {
        let geraete = speicher::geraete_von_user(&conn, &benutzer.oid)?.len();
        println!(
            "{}\t{}\t{}\t{}\t{}\t{} Gerät(e)",
            benutzer.oid,
            benutzer.email.as_deref().unwrap_or("-"),
            benutzer.rolle.als_str(),
            if benutzer.gesperrt {
                "gesperrt"
            } else {
                "offen"
            },
            benutzer.token_stand,
            geraete
        );
    }
    Ok(())
}

fn eintraege_zeigen() -> anyhow::Result<()> {
    let conn = datenbank()?;
    let eintraege: Vec<Eintrag> = speicher::liste(&conn, speicher::EINTRAEGE)?;
    println!("{}", serde_json::to_string_pretty(&eintraege)?);
    Ok(())
}

fn saetze_zeigen() -> anyhow::Result<()> {
    let conn = datenbank()?;
    let saetze: Vec<Satz> = speicher::liste(&conn, speicher::SAETZE)?;
    println!("{}", serde_json::to_string_pretty(&saetze)?);
    Ok(())
}

// ------------------------------------------------------------ Setzen aus Datei

fn lies_datei(rest: &[String], aufruf: &str) -> anyhow::Result<String> {
    let quelle = position(rest).ok_or_else(|| anyhow::anyhow!("Aufruf: {aufruf} <datei|->"))?;
    if quelle == "-" {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        Ok(text)
    } else {
        Ok(std::fs::read_to_string(&quelle).with_context(|| format!("Datei {quelle} lesen"))?)
    }
}

fn eintrag_setzen(rest: &[String]) -> anyhow::Result<()> {
    let text = lies_datei(rest, "quiver-server eintrag-setzen")?;
    let eintrag: Eintrag = serde_json::from_str(&text).context("Eintrag ist kein gültiges JSON")?;
    let conn = datenbank()?;
    speicher::setze(&conn, speicher::EINTRAEGE, eintrag.id.als_str(), &eintrag)?;
    speicher::pruefspur_schreiben(
        &conn,
        OffsetDateTime::now_utc(),
        "kommandozeile",
        "root-setzen",
        &format!("Eintrag {} gesetzt", eintrag.id),
    )?;
    println!("Eintrag {} gesetzt.", eintrag.id);
    Ok(())
}

fn satz_setzen(rest: &[String]) -> anyhow::Result<()> {
    let text = lies_datei(rest, "quiver-server satz-setzen")?;
    let satz: Satz = serde_json::from_str(&text).context("Satz ist kein gültiges JSON")?;
    let conn = datenbank()?;
    let unbekannt = speicher::unbekannte_eintraege(&conn, &satz)?;
    if !unbekannt.is_empty() {
        anyhow::bail!(
            "Satz {} verweist auf unbekannte Einträge: {}",
            satz.id,
            unbekannt
                .iter()
                .map(|id| id.als_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    speicher::setze(&conn, speicher::SAETZE, satz.id.als_str(), &satz)?;
    speicher::pruefspur_schreiben(
        &conn,
        OffsetDateTime::now_utc(),
        "kommandozeile",
        "root-setzen",
        &format!("Satz {} gesetzt", satz.id),
    )?;
    println!("Satz {} gesetzt.", satz.id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argumente_werden_gelesen() {
        let args = vec!["--oid".to_string(), "abc".into()];
        assert_eq!(wert(&args, "--oid").as_deref(), Some("abc"));
        let args = vec!["--oid=abc".to_string()];
        assert_eq!(wert(&args, "--oid").as_deref(), Some("abc"));
        assert!(wert(&args, "--fehlt").is_none());
        assert!(hat(&["--stdin".to_string()], "--stdin"));
        assert_eq!(
            position(&["x".into(), "--stdin".into()]).as_deref(),
            Some("x")
        );
        assert!(ist_unterbefehl(&["root-setzen".into()]));
        assert!(!ist_unterbefehl(&["--hilfe".into()]));
        assert!(ist_gesund(&["gesund".into()]));
        assert!(!ist_gesund(&["gesund".into(), "x".into()]));
        assert!(!ist_gesund(&["root-setzen".into()]));
    }

    /// Ohne `stty` (distroless-Abbild) darf ein Passwort nicht sichtbar von
    /// einem Terminal gelesen werden — lieber abbrechen und `--stdin` nennen.
    #[test]
    fn sichtbare_passworteingabe_wird_abgelehnt() {
        let mut quelle = std::io::Cursor::new("geheim\n");
        let fehler = zeile_lesen(true, false, &mut quelle)
            .expect_err("sichtbares Lesen muss abbrechen")
            .to_string();
        assert!(
            fehler.contains("--stdin"),
            "die Meldung muss den Ausweg nennen: {fehler}"
        );
        assert!(
            fehler.contains("stty"),
            "die Meldung muss den Grund nennen: {fehler}"
        );
        // Nichts wurde gelesen: das Passwort bleibt in der Quelle.
        assert_eq!(quelle.position(), 0);
    }

    /// Verdeckt am Terminal und ohne Terminal (Pipe, `--stdin`) wird gelesen.
    #[test]
    fn verdeckt_und_ohne_terminal_wird_gelesen() {
        let mut quelle = std::io::Cursor::new("erstes\nzweite Zeile\n");
        assert_eq!(zeile_lesen(true, true, &mut quelle).unwrap(), "erstes");
        assert_eq!(
            zeile_lesen(false, false, &mut quelle).unwrap(),
            "zweite Zeile"
        );
    }
}
