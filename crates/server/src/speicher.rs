// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ablage in SQLite: je Tabelle Kennung und JSON.
//!
//! Das Modell liegt in `quiver-domain`; hier steht nur, wie es auf die Platte
//! kommt. Alle Aufrufe sind synchron und laufen auf einer blockierenden
//! Aufgabe (siehe [`crate::db`]), damit keine Verbindung über ein `.await`
//! gehalten wird.

use std::path::Path;

use anyhow::Context;
use quiver_domain::{Eintrag, EintragId, Geraet, Gruppe, GruppeId, Satz};
use quiver_protocol::Meldung;
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;
use time::OffsetDateTime;

use crate::hilfen;

pub const EINTRAEGE: &str = "eintraege";
pub const SAETZE: &str = "saetze";
pub const GRUPPEN: &str = "gruppen";
pub const GERAETE: &str = "geraete";

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS eintraege (id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS saetze    (id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS gruppen   (id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS geraete (
    id              TEXT PRIMARY KEY,
    json            TEXT NOT NULL,
    token_hash      TEXT NOT NULL UNIQUE,
    zuletzt_gesehen TEXT,
    meldung_json    TEXT
);
CREATE TABLE IF NOT EXISTS codes (
    code        TEXT PRIMARY KEY,
    gruppen_json TEXT NOT NULL,
    gueltig_bis TEXT NOT NULL
);
"#;

/// Datenbank öffnen und das Schema anlegen.
pub fn oeffnen(pfad: &Path) -> anyhow::Result<Connection> {
    let conn =
        Connection::open(pfad).with_context(|| format!("Datenbank {} öffnen", pfad.display()))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")?;
    conn.execute_batch(SCHEMA)
        .with_context(|| format!("Schema in {} anlegen", pfad.display()))?;
    Ok(conn)
}

/// Ein Eintrag, ein Satz, eine Gruppe: `id` plus JSON.
pub fn liste<T: DeserializeOwned>(conn: &Connection, tabelle: &str) -> anyhow::Result<Vec<T>> {
    let sql = format!("SELECT json FROM {tabelle} ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let zeilen = stmt.query_map([], |zeile| zeile.get::<_, String>(0))?;
    let mut aus = Vec::new();
    for zeile in zeilen {
        let text = zeile?;
        aus.push(
            serde_json::from_str(&text).with_context(|| format!("{tabelle}: JSON nicht lesbar"))?,
        );
    }
    Ok(aus)
}

pub fn hole<T: DeserializeOwned>(
    conn: &Connection,
    tabelle: &str,
    id: &str,
) -> anyhow::Result<Option<T>> {
    let sql = format!("SELECT json FROM {tabelle} WHERE id = ?1");
    let text = conn
        .query_row(&sql, params![id], |zeile| zeile.get::<_, String>(0))
        .optional()?;
    match text {
        Some(text) => {
            Ok(Some(serde_json::from_str(&text).with_context(|| {
                format!("{tabelle}: JSON nicht lesbar")
            })?))
        }
        None => Ok(None),
    }
}

pub fn setze<T: Serialize>(
    conn: &Connection,
    tabelle: &str,
    id: &str,
    wert: &T,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(wert).context("Wert nicht serialisierbar")?;
    let sql = format!("INSERT OR REPLACE INTO {tabelle} (id, json) VALUES (?1, ?2)");
    conn.execute(&sql, params![id, json])?;
    Ok(())
}

pub fn loesche(conn: &Connection, tabelle: &str, id: &str) -> anyhow::Result<bool> {
    let sql = format!("DELETE FROM {tabelle} WHERE id = ?1");
    Ok(conn.execute(&sql, params![id])? > 0)
}

/// Alle Einträge, Sätze und Gruppen in einem Rutsch.
#[derive(Debug, Clone, Default)]
pub struct Bestand {
    pub eintraege: Vec<Eintrag>,
    pub saetze: Vec<Satz>,
    pub gruppen: Vec<Gruppe>,
}

pub fn bestand(conn: &Connection) -> anyhow::Result<Bestand> {
    Ok(Bestand {
        eintraege: liste(conn, EINTRAEGE)?,
        saetze: liste(conn, SAETZE)?,
        gruppen: liste(conn, GRUPPEN)?,
    })
}

/// Wird der Eintrag noch von einem Satz benutzt?
pub fn eintrag_benutzt(conn: &Connection, id: &str) -> anyhow::Result<bool> {
    let saetze: Vec<Satz> = liste(conn, SAETZE)?;
    Ok(saetze
        .iter()
        .any(|satz| satz.eintraege.iter().any(|e| e.eintrag.als_str() == id)))
}

/// Wird der Satz noch von einer Gruppe benutzt?
pub fn satz_benutzt(conn: &Connection, id: &str) -> anyhow::Result<bool> {
    let gruppen: Vec<Gruppe> = liste(conn, GRUPPEN)?;
    Ok(gruppen
        .iter()
        .any(|gruppe| gruppe.saetze.iter().any(|s| s.als_str() == id)))
}

/// Verweise eines Satzes auf Einträge, die es nicht gibt.
pub fn unbekannte_eintraege(conn: &Connection, satz: &Satz) -> anyhow::Result<Vec<EintragId>> {
    let eintraege: Vec<Eintrag> = liste(conn, EINTRAEGE)?;
    Ok(satz
        .eintraege
        .iter()
        .map(|e| &e.eintrag)
        .filter(|id| !eintraege.iter().any(|e| &e.id == *id))
        .cloned()
        .collect())
}

/// Ein Gerät mit allem, was dazu in der Datenbank steht.
#[derive(Debug, Clone)]
pub struct GeraetZeile {
    pub geraet: Geraet,
    pub zuletzt_gesehen: Option<String>,
    pub meldung: Option<Meldung>,
}

const GERAET_SPALTEN: &str = "json, zuletzt_gesehen, meldung_json";

fn zeile_aus(zeile: &rusqlite::Row<'_>) -> rusqlite::Result<GeraetZeile> {
    let json: String = zeile.get(0)?;
    let meldung_json: Option<String> = zeile.get(2)?;
    let geraet = serde_json::from_str(&json).map_err(|fehler| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(fehler))
    })?;
    let meldung = match meldung_json {
        Some(text) => Some(serde_json::from_str(&text).map_err(|fehler| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(fehler),
            )
        })?),
        None => None,
    };
    Ok(GeraetZeile {
        geraet,
        zuletzt_gesehen: zeile.get(1)?,
        meldung,
    })
}

pub fn alle_geraete(conn: &Connection) -> anyhow::Result<Vec<GeraetZeile>> {
    let sql = format!("SELECT {GERAET_SPALTEN} FROM {GERAETE} ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let zeilen = stmt.query_map([], zeile_aus)?;
    let mut aus = Vec::new();
    for zeile in zeilen {
        aus.push(zeile?);
    }
    Ok(aus)
}

pub fn hole_geraet(conn: &Connection, id: &str) -> anyhow::Result<Option<GeraetZeile>> {
    let sql = format!("SELECT {GERAET_SPALTEN} FROM geraete WHERE id = ?1");
    Ok(conn.query_row(&sql, params![id], zeile_aus).optional()?)
}

pub fn geraet_per_hash(conn: &Connection, hash: &str) -> anyhow::Result<Option<GeraetZeile>> {
    let sql = format!("SELECT {GERAET_SPALTEN} FROM geraete WHERE token_hash = ?1");
    Ok(conn.query_row(&sql, params![hash], zeile_aus).optional()?)
}

pub fn lege_geraet_an(conn: &Connection, geraet: &Geraet, token_hash: &str) -> anyhow::Result<()> {
    let json = serde_json::to_string(geraet)?;
    conn.execute(
        "INSERT INTO geraete (id, json, token_hash) VALUES (?1, ?2, ?3)",
        params![geraet.id.als_str(), json, token_hash],
    )?;
    Ok(())
}

pub fn aendere_geraet(conn: &Connection, geraet: &Geraet) -> anyhow::Result<bool> {
    let json = serde_json::to_string(geraet)?;
    Ok(conn.execute(
        "UPDATE geraete SET json = ?2 WHERE id = ?1",
        params![geraet.id.als_str(), json],
    )? > 0)
}

pub fn loesche_geraet(conn: &Connection, id: &str) -> anyhow::Result<bool> {
    Ok(conn.execute("DELETE FROM geraete WHERE id = ?1", params![id])? > 0)
}

/// „Das Gerät hat sich gemeldet" – Zeitstempel setzen, Meldung bleiben lassen.
pub fn setze_gesehen(conn: &Connection, id: &str, gesehen: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE geraete SET zuletzt_gesehen = ?2 WHERE id = ?1",
        params![id, gesehen],
    )?;
    Ok(())
}

/// Letzte Meldung und Zeitstempel speichern.
pub fn setze_meldung(
    conn: &Connection,
    id: &str,
    meldung: &Meldung,
    gesehen: &str,
) -> anyhow::Result<bool> {
    let json = serde_json::to_string(meldung)?;
    Ok(conn.execute(
        "UPDATE geraete SET meldung_json = ?2, zuletzt_gesehen = ?3 WHERE id = ?1",
        params![id, json, gesehen],
    )? > 0)
}

/// Ergebnis des Einlösens eines Einmal-Codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeStand {
    Unbekannt,
    Abgelaufen,
    Gueltig(Vec<GruppeId>),
}

/// Code einlösen: prüfen, verbrauchen, Gruppen zurückgeben.
///
/// Ein Code ist einmal benutzbar; abgelaufene werden dabei entfernt.
pub fn code_einloesen(
    conn: &Connection,
    code: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<CodeStand> {
    let zeile: Option<(String, String)> = conn
        .query_row(
            "SELECT gruppen_json, gueltig_bis FROM codes WHERE code = ?1",
            params![code],
            |zeile| Ok((zeile.get(0)?, zeile.get(1)?)),
        )
        .optional()?;
    let Some((gruppen_json, gueltig_bis)) = zeile else {
        return Ok(CodeStand::Unbekannt);
    };
    conn.execute("DELETE FROM codes WHERE code = ?1", params![code])?;
    match hilfen::zeit_lesen(&gueltig_bis) {
        Some(bis) if jetzt >= bis => Ok(CodeStand::Abgelaufen),
        Some(_) => Ok(CodeStand::Gueltig(serde_json::from_str(&gruppen_json)?)),
        None => Ok(CodeStand::Abgelaufen),
    }
}

pub fn code_existiert(conn: &Connection, code: &str) -> anyhow::Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM codes WHERE code = ?1", params![code], |_| {
            Ok(())
        })
        .optional()?
        .is_some())
}

pub fn lege_code_an(
    conn: &Connection,
    code: &str,
    gruppen: &[GruppeId],
    gueltig_bis: &str,
) -> anyhow::Result<()> {
    let gruppen_json = serde_json::to_string(gruppen)?;
    conn.execute(
        "INSERT INTO codes (code, gruppen_json, gueltig_bis) VALUES (?1, ?2, ?3)",
        params![code, gruppen_json, gueltig_bis],
    )?;
    Ok(())
}

/// Zähler für den Überblick.
pub fn anzahl(conn: &Connection, tabelle: &str) -> anyhow::Result<u32> {
    let sql = format!("SELECT COUNT(*) FROM {tabelle}");
    let n: i64 = conn.query_row(&sql, [], |zeile| zeile.get(0))?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_domain::{Herkunft, Laufzeit};

    fn eintrag(id: &str) -> Eintrag {
        Eintrag {
            id: EintragId::neu(id),
            version: "1.0.0".into(),
            titel: id.into(),
            beschreibung: String::new(),
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: "npx".into(),
                argumente: Vec::new(),
                umgebung: Default::default(),
            },
            felder: Vec::new(),
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
        }
    }

    fn gedaechtnis() -> Connection {
        oeffnen(Path::new(":memory:")).expect("Datenbank im Speicher")
    }

    #[test]
    fn eintraege_rundlauf() {
        let conn = gedaechtnis();
        assert!(liste::<Eintrag>(&conn, EINTRAEGE).unwrap().is_empty());
        setze(&conn, EINTRAEGE, "a", &eintrag("a")).unwrap();
        setze(&conn, EINTRAEGE, "b", &eintrag("b")).unwrap();
        assert_eq!(liste::<Eintrag>(&conn, EINTRAEGE).unwrap().len(), 2);
        let geholt: Eintrag = hole(&conn, EINTRAEGE, "a").unwrap().unwrap();
        assert_eq!(geholt, eintrag("a"));
        assert!(hole::<Eintrag>(&conn, EINTRAEGE, "fehlt")
            .unwrap()
            .is_none());
        assert!(loesche(&conn, EINTRAEGE, "a").unwrap());
        assert!(!loesche(&conn, EINTRAEGE, "a").unwrap());
    }

    #[test]
    fn codes_sind_einmalig() {
        let conn = gedaechtnis();
        let gruppen = vec![GruppeId::neu("team")];
        let jetzt = hilfen::jetzt();
        lege_code_an(
            &conn,
            "ABCD2345",
            &gruppen,
            &hilfen::als_text(jetzt + time::Duration::hours(1)),
        )
        .unwrap();
        assert_eq!(
            code_einloesen(&conn, "ABCD2345", jetzt).unwrap(),
            CodeStand::Gueltig(gruppen)
        );
        assert_eq!(
            code_einloesen(&conn, "ABCD2345", jetzt).unwrap(),
            CodeStand::Unbekannt
        );
    }

    #[test]
    fn abgelaufener_code_wird_entfernt() {
        let conn = gedaechtnis();
        let jetzt = hilfen::jetzt();
        lege_code_an(&conn, "ABCD2345", &[], &hilfen::als_text(jetzt)).unwrap();
        assert_eq!(
            code_einloesen(&conn, "ABCD2345", jetzt).unwrap(),
            CodeStand::Abgelaufen
        );
        assert_eq!(
            code_einloesen(&conn, "ABCD2345", jetzt).unwrap(),
            CodeStand::Unbekannt
        );
    }
}
