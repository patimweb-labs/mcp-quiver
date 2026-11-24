// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Ablage in SQLite: je Tabelle Kennung und JSON, dazu Benutzer, Sitzungen,
//! Einmal-Codes, Prüfspur und die kurzfristigen OIDC-Zustände.
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
use quiver_protocol::Rolle;

pub const EINTRAEGE: &str = "eintraege";
pub const SAETZE: &str = "saetze";
pub const GRUPPEN: &str = "gruppen";
pub const GERAETE: &str = "geraete";

/// `gueltig_bis` eines Codes ohne Ablauf.
pub const UNBEGRENZT: &str = "unbegrenzt";

/// Die Schemaversion, die diese Fassung schreibt.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS eintraege (id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS saetze    (id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS gruppen   (id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS geraete (
    id              TEXT PRIMARY KEY,
    json            TEXT NOT NULL,
    token_hash      TEXT NOT NULL UNIQUE,
    zuletzt_gesehen TEXT,
    meldung_json    TEXT,
    oid             TEXT,
    tid             TEXT,
    manuell_json    TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS codes (
    code        TEXT PRIMARY KEY,
    gruppen_json TEXT NOT NULL,
    gueltig_bis TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS benutzer (
    oid           TEXT PRIMARY KEY,
    tid           TEXT NOT NULL,
    email         TEXT,
    name          TEXT,
    rolle         TEXT NOT NULL DEFAULT 'benutzer',
    gesperrt      INTEGER NOT NULL DEFAULT 0,
    entra_json    TEXT NOT NULL DEFAULT '[]',
    token         TEXT,
    passwort      TEXT,
    token_erneuert TEXT,
    token_stand   TEXT NOT NULL DEFAULT 'neu_anmelden',
    erstellt      TEXT NOT NULL,
    zuletzt       TEXT
);
CREATE TABLE IF NOT EXISTS sitzungen (
    hash          TEXT PRIMARY KEY,
    oid           TEXT NOT NULL,
    erstellt      TEXT NOT NULL,
    zuletzt       TEXT NOT NULL,
    absolut_bis   TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS pruefspur (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    zeit          TEXT NOT NULL,
    art           TEXT NOT NULL,
    akteur        TEXT NOT NULL,
    text          TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS oidc_zustand (
    state_hash    TEXT PRIMARY KEY,
    cookie_hash   TEXT NOT NULL,
    verifier      TEXT NOT NULL,
    nonce         TEXT NOT NULL,
    challenge     TEXT NOT NULL,
    modus         TEXT NOT NULL DEFAULT 'geraet',
    port          INTEGER NOT NULL,
    geraetezustand TEXT NOT NULL,
    name          TEXT NOT NULL,
    gueltig_bis   TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS bestaetigungen (
    token_hash    TEXT PRIMARY KEY,
    cookie_hash   TEXT NOT NULL,
    csrf          TEXT NOT NULL,
    oid           TEXT NOT NULL,
    tid           TEXT NOT NULL,
    email         TEXT,
    name          TEXT,
    entra_json    TEXT NOT NULL,
    challenge     TEXT NOT NULL,
    geraete_name  TEXT NOT NULL,
    port          INTEGER NOT NULL,
    geraetezustand TEXT NOT NULL,
    gueltig_bis   TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS geraete_codes (
    code_hash     TEXT PRIMARY KEY,
    oid           TEXT NOT NULL,
    tid           TEXT NOT NULL,
    entra_json    TEXT NOT NULL,
    challenge     TEXT NOT NULL,
    name          TEXT NOT NULL,
    gueltig_bis   TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS entra_gruppen (
    id            TEXT PRIMARY KEY,
    anzeigename   TEXT,
    zuletzt       TEXT NOT NULL
);
"#;

/// Datenbank öffnen, sichern (nur bei einer Datei), migrieren, Schema prüfen.
pub fn oeffnen(pfad: &Path) -> anyhow::Result<Connection> {
    let ist_datei = pfad.to_string_lossy() != ":memory:";
    let version = if ist_datei && pfad.exists() {
        zeige_version(pfad)?
    } else {
        0
    };
    if ist_datei && version < SCHEMA_VERSION && pfad.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
        let sicherung = pfad.with_file_name(format!(
            "{}.vor-{}",
            pfad.file_name().unwrap_or_default().to_string_lossy(),
            SCHEMA_VERSION
        ));
        std::fs::copy(pfad, &sicherung).with_context(|| {
            format!(
                "Sicherung der Datenbank nach {} vor der Migration",
                sicherung.display()
            )
        })?;
        tracing::info!(
            "Datenbank gesichert: {} → {}",
            pfad.display(),
            sicherung.display()
        );
    }

    let conn =
        Connection::open(pfad).with_context(|| format!("Datenbank {} öffnen", pfad.display()))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")?;
    migrieren(&conn)?;
    Ok(conn)
}

fn zeige_version(pfad: &Path) -> anyhow::Result<i64> {
    let conn = Connection::open(pfad)
        .with_context(|| format!("Datenbank {} für die Versionsprobe öffnen", pfad.display()))?;
    Ok(conn.query_row("PRAGMA user_version", [], |zeile| zeile.get(0))?)
}

/// Versionierte Migrationen. Jede Stufe ist wiederholbar.
fn migrieren(conn: &Connection) -> anyhow::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |zeile| zeile.get(0))?;
    if version >= SCHEMA_VERSION {
        return Ok(());
    }
    conn.execute_batch(SCHEMA_V1)?;
    // Alte `geraete`-Tabelle (Stand vor Aufgabe 20) hat die neuen Spalten nicht.
    let mut manuell_neu = false;
    for (name, typ, vorgabe) in [
        ("oid", "TEXT", None),
        ("tid", "TEXT", None),
        ("manuell_json", "TEXT", Some("'[]'")),
    ] {
        if !spalte_existiert(conn, GERAETE, name)? {
            let zusatz = match vorgabe {
                Some(wert) => format!(" DEFAULT {wert}"),
                None => String::new(),
            };
            conn.execute(
                &format!("ALTER TABLE {GERAETE} ADD COLUMN {name} {typ}{zusatz}"),
                [],
            )?;
            if name == "manuell_json" {
                manuell_neu = true;
            }
        }
    }
    // Alte Geräte kennen nur eine Gruppenliste; sie wird die manuelle Liste.
    if manuell_neu {
        let mut stmt = conn.prepare("SELECT id, json FROM geraete")?;
        let zeilen = stmt
            .query_map([], |zeile| {
                Ok((zeile.get::<_, String>(0)?, zeile.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, json) in zeilen {
            if let Ok(geraet) = serde_json::from_str::<Geraet>(&json) {
                let manuell = serde_json::to_string(&geraet.gruppen)?;
                conn.execute(
                    "UPDATE geraete SET manuell_json = ?2 WHERE id = ?1",
                    params![id, manuell],
                )?;
            }
        }
    }
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn spalte_existiert(conn: &Connection, tabelle: &str, spalte: &str) -> anyhow::Result<bool> {
    let sql = format!("SELECT 1 FROM pragma_table_info('{tabelle}') WHERE name = ?1");
    Ok(conn
        .query_row(&sql, params![spalte], |_| Ok(()))
        .optional()?
        .is_some())
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
    /// Entra-Identität des Geräts; `None` bei einem Altgerät mit Code.
    pub oid: Option<String>,
    pub tid: Option<String>,
    /// Nur die am Gerät gesetzten Gruppen (ohne die aus Entra).
    pub manuell: Vec<GruppeId>,
}

const GERAET_SPALTEN: &str = "json, zuletzt_gesehen, meldung_json, oid, tid, manuell_json";

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
    let manuell_json: String = zeile.get(5)?;
    let manuell = serde_json::from_str(&manuell_json).map_err(|fehler| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(fehler))
    })?;
    Ok(GeraetZeile {
        geraet,
        zuletzt_gesehen: zeile.get(1)?,
        meldung,
        oid: zeile.get(3)?,
        tid: zeile.get(4)?,
        manuell,
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

pub fn lege_geraet_an(
    conn: &Connection,
    geraet: &Geraet,
    token_hash: &str,
    oid: Option<&str>,
    tid: Option<&str>,
    // Am Gerät gesetzte Gruppen; `None` heißt: die Gruppen des Geräts sind
    // auch die manuellen (Altgerät ohne Entra).
    manuell_gruppen: Option<&[GruppeId]>,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(geraet)?;
    let manuell = serde_json::to_string(manuell_gruppen.unwrap_or(&geraet.gruppen))?;
    conn.execute(
        "INSERT INTO geraete (id, json, token_hash, oid, tid, manuell_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![geraet.id.als_str(), json, token_hash, oid, tid, manuell],
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

/// Gerät umhängen: neues Token, neue Identität, neue Gruppen.
pub fn erneuere_geraet(
    conn: &Connection,
    id: &str,
    token_hash: &str,
    oid: &str,
    tid: &str,
    geraet: &Geraet,
) -> anyhow::Result<bool> {
    let json = serde_json::to_string(geraet)?;
    let manuell = serde_json::to_string(&geraet.gruppen)?;
    Ok(conn.execute(
        "UPDATE geraete SET token_hash = ?2, oid = ?3, tid = ?4, json = ?5, manuell_json = ?6 \
         WHERE id = ?1",
        params![id, token_hash, oid, tid, json, manuell],
    )? > 0)
}

pub fn setze_manuell(
    conn: &Connection,
    id: &str,
    geraet: &Geraet,
    manuell: &[GruppeId],
) -> anyhow::Result<bool> {
    let json = serde_json::to_string(geraet)?;
    let manuell = serde_json::to_string(manuell)?;
    Ok(conn.execute(
        "UPDATE geraete SET json = ?2, manuell_json = ?3 WHERE id = ?1",
        params![id, json, manuell],
    )? > 0)
}

pub fn loesche_geraet(conn: &Connection, id: &str) -> anyhow::Result<bool> {
    Ok(conn.execute("DELETE FROM geraete WHERE id = ?1", params![id])? > 0)
}

pub fn geraete_von_user(conn: &Connection, oid: &str) -> anyhow::Result<Vec<GeraetZeile>> {
    let sql = format!("SELECT {GERAET_SPALTEN} FROM {GERAETE} WHERE oid = ?1 ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let zeilen = stmt.query_map(params![oid], zeile_aus)?;
    let mut aus = Vec::new();
    for zeile in zeilen {
        aus.push(zeile?);
    }
    Ok(aus)
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
/// Ein Code ist einmal benutzbar; abgelaufene werden dabei entfernt. Ein Code
/// ohne Ablauf ([`UNBEGRENZT`]) gilt, bis er eingelöst wird.
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
    if gueltig_bis == UNBEGRENZT {
        return Ok(CodeStand::Gueltig(serde_json::from_str(&gruppen_json)?));
    }
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

// ------------------------------------------------------------------ Benutzer

/// Ein Benutzer, wie er in der Datenbank steht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Benutzer {
    pub oid: String,
    pub tid: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub rolle: Rolle,
    pub gesperrt: bool,
    /// Die Entra-Gruppen-Ids aus dem letzten ID-Token.
    pub entra: Vec<String>,
    /// `gueltig` oder `neu_anmelden`.
    pub token_stand: String,
    /// Zeitpunkt der letzten erfolgreichen Nachprüfung.
    pub token_erneuert: Option<String>,
    pub erstellt: String,
    pub zuletzt: Option<String>,
    /// Verschlüsseltes Refresh-Token (nie im Klartext).
    pub token: Option<String>,
}

const BENUTZER_SPALTEN: &str =
    "oid, tid, email, name, rolle, gesperrt, entra_json, token, token_erneuert, token_stand, \
     erstellt, zuletzt";

fn benutzer_aus(zeile: &rusqlite::Row<'_>) -> rusqlite::Result<Benutzer> {
    let rolle: String = zeile.get(4)?;
    let entra_json: String = zeile.get(6)?;
    Ok(Benutzer {
        oid: zeile.get(0)?,
        tid: zeile.get(1)?,
        email: zeile.get(2)?,
        name: zeile.get(3)?,
        rolle: Rolle::aus_str(&rolle),
        gesperrt: zeile.get::<_, i64>(5)? != 0,
        entra: serde_json::from_str(&entra_json).unwrap_or_default(),
        token: zeile.get(7)?,
        token_erneuert: zeile.get(8)?,
        token_stand: zeile.get(9)?,
        erstellt: zeile.get(10)?,
        zuletzt: zeile.get(11)?,
    })
}

pub fn benutzer_holen(conn: &Connection, oid: &str) -> anyhow::Result<Option<Benutzer>> {
    let sql = format!("SELECT {BENUTZER_SPALTEN} FROM benutzer WHERE oid = ?1");
    Ok(conn
        .query_row(&sql, params![oid], benutzer_aus)
        .optional()?)
}

pub fn benutzer_liste(conn: &Connection) -> anyhow::Result<Vec<Benutzer>> {
    let sql = format!("SELECT {BENUTZER_SPALTEN} FROM benutzer ORDER BY oid");
    let mut stmt = conn.prepare(&sql)?;
    let zeilen = stmt.query_map([], benutzer_aus)?;
    let mut aus = Vec::new();
    for zeile in zeilen {
        aus.push(zeile?);
    }
    Ok(aus)
}

/// Benutzer aus einem ID-Token anlegen oder auffrischen. Rechte bleiben, wo sie
/// sind: eine Anmeldung allein macht niemanden zum Admin.
#[allow(clippy::too_many_arguments)]
pub fn benutzer_anmelden(
    conn: &Connection,
    oid: &str,
    tid: &str,
    email: Option<&str>,
    name: Option<&str>,
    entra: &[String],
    token: Option<&str>,
    jetzt: OffsetDateTime,
) -> anyhow::Result<()> {
    let jetzt_text = hilfen::als_text(jetzt);
    let entra_json = serde_json::to_string(entra)?;
    let vorhanden = benutzer_holen(conn, oid)?;
    match vorhanden {
        Some(b) => {
            conn.execute(
                "UPDATE benutzer SET tid = ?2, email = ?3, name = ?4, entra_json = ?5, \
                 token = COALESCE(?6, token), token_erneuert = ?7, token_stand = 'gueltig', \
                 zuletzt = ?8 WHERE oid = ?1",
                params![oid, tid, email, name, entra_json, token, jetzt_text, jetzt_text],
            )?;
            let _ = b;
        }
        None => {
            conn.execute(
                "INSERT INTO benutzer (oid, tid, email, name, rolle, gesperrt, entra_json, \
                 token, token_erneuert, token_stand, erstellt, zuletzt) \
                 VALUES (?1, ?2, ?3, ?4, 'benutzer', 0, ?5, ?6, ?7, 'gueltig', ?7, ?7)",
                params![oid, tid, email, name, entra_json, token, jetzt_text],
            )?;
        }
    }
    Ok(())
}

/// Nur den Token-Stand ändern (Nachprüfung).
pub fn benutzer_token(
    conn: &Connection,
    oid: &str,
    token: Option<&str>,
    erneuert: Option<OffsetDateTime>,
    stand: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE benutzer SET token = ?2, token_erneuert = COALESCE(?3, token_erneuert), \
         token_stand = ?4 WHERE oid = ?1",
        params![oid, token, erneuert.map(hilfen::als_text), stand],
    )?;
    Ok(())
}

/// Gruppen und Anzeige aus einem neuen ID-Token übernehmen.
pub fn benutzer_gruppen(
    conn: &Connection,
    oid: &str,
    entra: &[String],
    email: Option<&str>,
    name: Option<&str>,
    jetzt: OffsetDateTime,
) -> anyhow::Result<()> {
    let entra_json = serde_json::to_string(entra)?;
    conn.execute(
        "UPDATE benutzer SET entra_json = ?2, email = COALESCE(?3, email), \
         name = COALESCE(?4, name), zuletzt = ?5 WHERE oid = ?1",
        params![oid, entra_json, email, name, hilfen::als_text(jetzt)],
    )?;
    Ok(())
}

pub fn rolle_setzen(conn: &Connection, oid: &str, rolle: Rolle) -> anyhow::Result<bool> {
    Ok(conn.execute(
        "UPDATE benutzer SET rolle = ?2 WHERE oid = ?1",
        params![oid, rolle.als_str()],
    )? > 0)
}

pub fn sperre_setzen(conn: &Connection, oid: &str, gesperrt: bool) -> anyhow::Result<bool> {
    Ok(conn.execute(
        "UPDATE benutzer SET gesperrt = ?2 WHERE oid = ?1",
        params![oid, i64::from(gesperrt)],
    )? > 0)
}

/// Benutzer löschen: Token, Sitzungen und alle Geräte gehen mit.
pub fn benutzer_loeschen(conn: &Connection, oid: &str) -> anyhow::Result<bool> {
    conn.execute("DELETE FROM geraete WHERE oid = ?1", params![oid])?;
    conn.execute("DELETE FROM sitzungen WHERE oid = ?1", params![oid])?;
    Ok(conn.execute("DELETE FROM benutzer WHERE oid = ?1", params![oid])? > 0)
}

/// Der Root-Benutzer: höchstens einer, Kennung `root`.
pub const ROOT_OID: &str = "root";

pub fn root_holen(conn: &Connection) -> anyhow::Result<Option<Benutzer>> {
    benutzer_holen(conn, ROOT_OID)
}

/// Root setzen oder ändern; alle Root-Sitzungen enden dabei.
pub fn root_setzen(
    conn: &Connection,
    email: &str,
    passwort_hash: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<()> {
    let jetzt_text = hilfen::als_text(jetzt);
    conn.execute(
        "INSERT INTO benutzer (oid, tid, email, name, rolle, gesperrt, entra_json, token, \
         token_erneuert, token_stand, erstellt, zuletzt) \
         VALUES (?1, 'root', ?2, 'Root', 'root', 0, '[]', NULL, NULL, 'neu_anmelden', ?3, ?3) \
         ON CONFLICT(oid) DO UPDATE SET email = ?2, rolle = 'root', gesperrt = 0",
        params![ROOT_OID, email, jetzt_text],
    )?;
    // Das Passwort steht in einer eigenen Spalte `token`, damit die Tabelle
    // für Entra-Benutzer gleich bleibt? Nein: ein eigenes Feld ist klarer.
    conn.execute(
        "UPDATE benutzer SET passwort = ?2 WHERE oid = ?1",
        params![ROOT_OID, passwort_hash],
    )?;
    conn.execute("DELETE FROM sitzungen WHERE oid = ?1", params![ROOT_OID])?;
    Ok(())
}

/// Den gespeicherten Passwort-Hash des Root-Benutzers holen.
pub fn root_passwort(conn: &Connection) -> anyhow::Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT passwort FROM benutzer WHERE oid = ?1",
            params![ROOT_OID],
            |zeile| zeile.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// Root per E-Mail finden (normalisiert, ohne Rücksicht auf Groß-/Kleinschreibung).
pub fn root_per_email(conn: &Connection, email: &str) -> anyhow::Result<Option<Benutzer>> {
    let sql = format!(
        "SELECT {BENUTZER_SPALTEN} FROM benutzer WHERE oid = ?1 AND email = ?2 COLLATE NOCASE"
    );
    Ok(conn
        .query_row(&sql, params![ROOT_OID, email], benutzer_aus)
        .optional()?)
}

/// Gibt es schon einen Root?
pub fn root_ist_gesetzt(conn: &Connection) -> anyhow::Result<bool> {
    Ok(root_holen(conn)?.is_some())
}

// ----------------------------------------------------------------- Sitzungen

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sitzung {
    pub oid: String,
    pub erstellt: OffsetDateTime,
    pub zuletzt: OffsetDateTime,
    pub absolut_bis: OffsetDateTime,
}

pub fn sitzung_anlegen(
    conn: &Connection,
    hash: &str,
    oid: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<()> {
    let absolut = jetzt + time::Duration::hours(12);
    conn.execute(
        "INSERT INTO sitzungen (hash, oid, erstellt, zuletzt, absolut_bis) \
         VALUES (?1, ?2, ?3, ?3, ?4)",
        params![
            hash,
            oid,
            hilfen::als_text(jetzt),
            hilfen::als_text(absolut)
        ],
    )?;
    Ok(())
}

/// Sitzung prüfen und die letzte Nutzung nachziehen. `None` heißt: ungültig.
pub fn sitzung_holen(
    conn: &Connection,
    hash: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<Option<Sitzung>> {
    let zeile: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT oid, erstellt, zuletzt, absolut_bis FROM sitzungen WHERE hash = ?1",
            params![hash],
            |zeile| Ok((zeile.get(0)?, zeile.get(1)?, zeile.get(2)?, zeile.get(3)?)),
        )
        .optional()?;
    let Some((oid, erstellt, zuletzt, absolut_bis)) = zeile else {
        return Ok(None);
    };
    let (Some(erstellt), Some(zuletzt), Some(absolut_bis)) = (
        hilfen::zeit_lesen(&erstellt),
        hilfen::zeit_lesen(&zuletzt),
        hilfen::zeit_lesen(&absolut_bis),
    ) else {
        conn.execute("DELETE FROM sitzungen WHERE hash = ?1", params![hash])?;
        return Ok(None);
    };
    if jetzt >= absolut_bis
        || jetzt - zuletzt > time::Duration::minutes(crate::sitzung::LEERLAUF_MINUTEN)
    {
        conn.execute("DELETE FROM sitzungen WHERE hash = ?1", params![hash])?;
        return Ok(None);
    }
    conn.execute(
        "UPDATE sitzungen SET zuletzt = ?2 WHERE hash = ?1",
        params![hash, hilfen::als_text(jetzt)],
    )?;
    Ok(Some(Sitzung {
        oid,
        erstellt,
        zuletzt,
        absolut_bis,
    }))
}

pub fn sitzung_loeschen(conn: &Connection, hash: &str) -> anyhow::Result<bool> {
    Ok(conn.execute("DELETE FROM sitzungen WHERE hash = ?1", params![hash])? > 0)
}

pub fn sitzungen_aufraeumen(conn: &Connection, jetzt: OffsetDateTime) -> anyhow::Result<()> {
    let grenze = jetzt - time::Duration::hours(24);
    conn.execute(
        "DELETE FROM sitzungen WHERE absolut_bis < ?1",
        params![hilfen::als_text(grenze)],
    )?;
    Ok(())
}

// ----------------------------------------------------------------- Prüfspur

/// Wie lange die Prüfspur bleibt.
pub const PRUEFSPUR_TAGE: i64 = 180;

pub fn pruefspur_schreiben(
    conn: &Connection,
    jetzt: OffsetDateTime,
    art: &str,
    akteur: &str,
    text: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO pruefspur (zeit, art, akteur, text) VALUES (?1, ?2, ?3, ?4)",
        params![hilfen::als_text(jetzt), art, akteur, text],
    )?;
    let grenze = jetzt - time::Duration::days(PRUEFSPUR_TAGE);
    conn.execute(
        "DELETE FROM pruefspur WHERE zeit < ?1",
        params![hilfen::als_text(grenze)],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruefspurZeile {
    pub zeit: String,
    pub art: String,
    pub akteur: String,
    pub text: String,
}

pub fn pruefspur_liste(conn: &Connection, limit: u32) -> anyhow::Result<Vec<PruefspurZeile>> {
    let mut stmt =
        conn.prepare("SELECT zeit, art, akteur, text FROM pruefspur ORDER BY id DESC LIMIT ?1")?;
    let zeilen = stmt.query_map(params![i64::from(limit.clamp(1, 1000))], |zeile| {
        Ok(PruefspurZeile {
            zeit: zeile.get(0)?,
            art: zeile.get(1)?,
            akteur: zeile.get(2)?,
            text: zeile.get(3)?,
        })
    })?;
    let mut aus = Vec::new();
    for zeile in zeilen {
        aus.push(zeile?);
    }
    Ok(aus)
}

// ------------------------------------------------------------ OIDC-Zustände

/// Ein laufender Entra-Anmeldevorgang (vor der Rückkehr).
#[derive(Debug, Clone)]
pub struct OidcZustand {
    pub cookie_hash: String,
    pub verifier: String,
    pub nonce: String,
    /// Der PKCE-Challenge des Geräts (S256 des Verifiers, den nur das Gerät kennt).
    pub challenge: String,
    /// `geraet` oder `admin`.
    pub modus: String,
    pub port: u16,
    pub geraetezustand: String,
    pub name: String,
    pub gueltig_bis: String,
}

pub fn oidc_anlegen(
    conn: &Connection,
    state_hash: &str,
    zustand: &OidcZustand,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO oidc_zustand (state_hash, cookie_hash, verifier, nonce, challenge, modus, \
         port, geraetezustand, name, gueltig_bis) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            state_hash,
            zustand.cookie_hash,
            zustand.verifier,
            zustand.nonce,
            zustand.challenge,
            zustand.modus,
            zustand.port,
            zustand.geraetezustand,
            zustand.name,
            zustand.gueltig_bis
        ],
    )?;
    Ok(())
}

/// Zustand einmalig holen und dabei verbrauchen.
pub fn oidc_nehmen(
    conn: &Connection,
    state_hash: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<Option<(OidcZustand, bool)>> {
    #[allow(clippy::type_complexity)]
    let zeile: Option<(
        String,
        String,
        String,
        String,
        String,
        u16,
        String,
        String,
        String,
    )> = conn
        .query_row(
            "SELECT cookie_hash, verifier, nonce, challenge, modus, port, geraetezustand, name, \
             gueltig_bis FROM oidc_zustand WHERE state_hash = ?1",
            params![state_hash],
            |zeile| {
                Ok((
                    zeile.get(0)?,
                    zeile.get(1)?,
                    zeile.get(2)?,
                    zeile.get(3)?,
                    zeile.get(4)?,
                    zeile.get(5)?,
                    zeile.get(6)?,
                    zeile.get(7)?,
                    zeile.get(8)?,
                ))
            },
        )
        .optional()?;
    let Some((
        cookie_hash,
        verifier,
        nonce,
        challenge,
        modus,
        port,
        geraetezustand,
        name,
        gueltig_bis,
    )) = zeile
    else {
        return Ok(None);
    };
    conn.execute(
        "DELETE FROM oidc_zustand WHERE state_hash = ?1",
        params![state_hash],
    )?;
    let gueltig = hilfen::zeit_lesen(&gueltig_bis).is_some_and(|bis| jetzt < bis);
    Ok(Some((
        OidcZustand {
            cookie_hash,
            verifier,
            nonce,
            challenge,
            modus,
            port,
            geraetezustand,
            name,
            gueltig_bis,
        },
        gueltig,
    )))
}

pub fn oidc_aufraeumen(conn: &Connection, jetzt: OffsetDateTime) -> anyhow::Result<()> {
    let text = hilfen::als_text(jetzt);
    conn.execute(
        "DELETE FROM oidc_zustand WHERE gueltig_bis < ?1",
        params![text],
    )?;
    conn.execute(
        "DELETE FROM bestaetigungen WHERE gueltig_bis < ?1",
        params![text],
    )?;
    Ok(())
}

/// Eine wartende Bestätigung nach der Rückkehr von Entra.
#[derive(Debug, Clone)]
pub struct Bestaetigung {
    pub cookie_hash: String,
    pub csrf: String,
    pub oid: String,
    pub tid: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub entra: Vec<String>,
    pub challenge: String,
    pub geraete_name: String,
    pub port: u16,
    pub geraetezustand: String,
    pub gueltig_bis: String,
}

pub fn bestaetigung_anlegen(
    conn: &Connection,
    token_hash: &str,
    b: &Bestaetigung,
) -> anyhow::Result<()> {
    let entra_json = serde_json::to_string(&b.entra)?;
    conn.execute(
        "INSERT INTO bestaetigungen (token_hash, cookie_hash, csrf, oid, tid, email, name, \
         entra_json, challenge, geraete_name, port, geraetezustand, gueltig_bis) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            token_hash,
            b.cookie_hash,
            b.csrf,
            b.oid,
            b.tid,
            b.email,
            b.name,
            entra_json,
            b.challenge,
            b.geraete_name,
            b.port,
            b.geraetezustand,
            b.gueltig_bis
        ],
    )?;
    Ok(())
}

pub fn bestaetigung_nehmen(
    conn: &Connection,
    token_hash: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<Option<Bestaetigung>> {
    #[allow(clippy::type_complexity)]
    let zeile: Option<(String, String, String, String, Option<String>, Option<String>, String, String, String, u16, String, String)> = conn
        .query_row(
            "SELECT cookie_hash, csrf, oid, tid, email, name, entra_json, challenge, geraete_name, \
             port, geraetezustand, gueltig_bis FROM bestaetigungen WHERE token_hash = ?1",
            params![token_hash],
            |zeile| {
                Ok((
                    zeile.get(0)?,
                    zeile.get(1)?,
                    zeile.get(2)?,
                    zeile.get(3)?,
                    zeile.get(4)?,
                    zeile.get(5)?,
                    zeile.get(6)?,
                    zeile.get(7)?,
                    zeile.get(8)?,
                    zeile.get(9)?,
                    zeile.get(10)?,
                    zeile.get(11)?,
                ))
            },
        )
        .optional()?;
    let Some((
        cookie_hash,
        csrf,
        oid,
        tid,
        email,
        name,
        entra_json,
        challenge,
        geraete_name,
        port,
        geraetezustand,
        gueltig_bis,
    )) = zeile
    else {
        return Ok(None);
    };
    conn.execute(
        "DELETE FROM bestaetigungen WHERE token_hash = ?1",
        params![token_hash],
    )?;
    if !hilfen::zeit_lesen(&gueltig_bis).is_some_and(|bis| jetzt < bis) {
        return Ok(None);
    }
    Ok(Some(Bestaetigung {
        cookie_hash,
        csrf,
        oid,
        tid,
        email,
        name,
        entra: serde_json::from_str(&entra_json).unwrap_or_default(),
        challenge,
        geraete_name,
        port,
        geraetezustand,
        gueltig_bis,
    }))
}

// ---------------------------------------------------------- Geräte-Einmalcode

/// Ein Einmalcode für den Tausch `Code → Geräte-Token` (60 s, einmalig).
#[derive(Debug, Clone)]
pub struct GeraeteCode {
    pub oid: String,
    pub tid: String,
    pub entra: Vec<String>,
    pub challenge: String,
    pub name: String,
    pub gueltig_bis: String,
}

pub fn geraete_code_anlegen(
    conn: &Connection,
    code_hash: &str,
    c: &GeraeteCode,
) -> anyhow::Result<()> {
    let entra_json = serde_json::to_string(&c.entra)?;
    conn.execute(
        "INSERT INTO geraete_codes (code_hash, oid, tid, entra_json, challenge, name, gueltig_bis) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            code_hash,
            c.oid,
            c.tid,
            entra_json,
            c.challenge,
            c.name,
            c.gueltig_bis
        ],
    )?;
    Ok(())
}

/// Code holen und **in jedem Fall** verbrauchen (auch bei falschem Verifier).
pub fn geraete_code_nehmen(
    conn: &Connection,
    code_hash: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<Option<(GeraeteCode, bool)>> {
    let zeile: Option<(String, String, String, String, String, String)> = conn
        .query_row(
            "SELECT oid, tid, entra_json, challenge, name, gueltig_bis FROM geraete_codes \
             WHERE code_hash = ?1",
            params![code_hash],
            |zeile| {
                Ok((
                    zeile.get(0)?,
                    zeile.get(1)?,
                    zeile.get(2)?,
                    zeile.get(3)?,
                    zeile.get(4)?,
                    zeile.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((oid, tid, entra_json, challenge, name, gueltig_bis)) = zeile else {
        return Ok(None);
    };
    conn.execute(
        "DELETE FROM geraete_codes WHERE code_hash = ?1",
        params![code_hash],
    )?;
    let gueltig = hilfen::zeit_lesen(&gueltig_bis).is_some_and(|bis| jetzt < bis);
    Ok(Some((
        GeraeteCode {
            oid,
            tid,
            entra: serde_json::from_str(&entra_json).unwrap_or_default(),
            challenge,
            name,
            gueltig_bis,
        },
        gueltig,
    )))
}

// ------------------------------------------------------------- Entra-Gruppen

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntraGruppeZeile {
    pub id: String,
    pub anzeigename: Option<String>,
    pub zuletzt: String,
}

pub fn entra_gruppen_merken(
    conn: &Connection,
    ids: &[String],
    jetzt: OffsetDateTime,
) -> anyhow::Result<()> {
    let text = hilfen::als_text(jetzt);
    for id in ids {
        conn.execute(
            "INSERT INTO entra_gruppen (id, anzeigename, zuletzt) VALUES (?1, NULL, ?2) \
             ON CONFLICT(id) DO UPDATE SET zuletzt = ?2",
            params![id, text],
        )?;
    }
    Ok(())
}

pub fn entra_gruppen_liste(conn: &Connection) -> anyhow::Result<Vec<EntraGruppeZeile>> {
    let mut stmt =
        conn.prepare("SELECT id, anzeigename, zuletzt FROM entra_gruppen ORDER BY id")?;
    let zeilen = stmt.query_map([], |zeile| {
        Ok(EntraGruppeZeile {
            id: zeile.get(0)?,
            anzeigename: zeile.get(1)?,
            zuletzt: zeile.get(2)?,
        })
    })?;
    let mut aus = Vec::new();
    for zeile in zeilen {
        aus.push(zeile?);
    }
    Ok(aus)
}

pub fn entra_anzeigename_setzen(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
) -> anyhow::Result<bool> {
    Ok(conn.execute(
        "UPDATE entra_gruppen SET anzeigename = ?2 WHERE id = ?1",
        params![id, name],
    )? > 0)
}

/// Wirksame Gruppen eines Geräts: die am Gerät gesetzten **und** die über die
/// Entra-Ids des Benutzers zugeordneten (SPEC §5, Aufgabe 20).
pub fn wirksame_gruppen(
    conn: &Connection,
    manuell: &[GruppeId],
    entra: &[String],
) -> anyhow::Result<Vec<GruppeId>> {
    let gruppen: Vec<Gruppe> = liste(conn, GRUPPEN)?;
    let mut aus = manuell.to_vec();
    for gruppe in gruppen {
        if gruppe
            .entra
            .iter()
            .any(|id| entra.iter().any(|e| e.eq_ignore_ascii_case(id)))
            && !aus.contains(&gruppe.id)
        {
            aus.push(gruppe.id);
        }
    }
    aus.sort();
    Ok(aus)
}

// ---------------------------------------------------- Datenbankänderung sehen

/// `PRAGMA data_version` – ändert sich, wenn eine andere Verbindung schreibt.
pub fn data_version(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row("PRAGMA data_version", [], |zeile| zeile.get(0))?)
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
            mehrfach: None,
        }
    }

    fn gedaechtnis() -> Connection {
        oeffnen(Path::new(":memory:")).expect("Datenbank im Speicher")
    }

    fn ordner(name: &str) -> std::path::PathBuf {
        let pfad = std::env::temp_dir().join(format!(
            "quiver-speicher-{name}-{}-{}",
            std::process::id(),
            hilfen::token_erzeugen()
        ));
        std::fs::create_dir_all(&pfad).unwrap();
        pfad
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

    /// Eine Datei im heutigen (alten) Schema behält bei der Migration alle Daten
    /// und bekommt vorher eine Sicherung.
    #[test]
    fn migration_aus_dem_heutigen_schema() {
        let ordner = ordner("migration");
        let pfad = ordner.join("quiver.db");
        {
            let conn = Connection::open(&pfad).unwrap();
            conn.execute_batch(
                "CREATE TABLE eintraege (id TEXT PRIMARY KEY, json TEXT NOT NULL);
                 CREATE TABLE saetze    (id TEXT PRIMARY KEY, json TEXT NOT NULL);
                 CREATE TABLE gruppen   (id TEXT PRIMARY KEY, json TEXT NOT NULL);
                 CREATE TABLE geraete (
                    id TEXT PRIMARY KEY, json TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE,
                    zuletzt_gesehen TEXT, meldung_json TEXT);
                 CREATE TABLE codes (
                    code TEXT PRIMARY KEY, gruppen_json TEXT NOT NULL, gueltig_bis TEXT NOT NULL);",
            )
            .unwrap();
            setze(&conn, EINTRAEGE, "wetter", &eintrag("wetter")).unwrap();
            let geraet = Geraet {
                id: quiver_domain::GeraetId::neu("alt-1"),
                name: "Altgerät".into(),
                gruppen: vec![GruppeId::neu("team")],
                gesperrt: false,
            };
            conn.execute(
                "INSERT INTO geraete (id, json, token_hash) VALUES ('alt-1', ?1, 'hash')",
                params![serde_json::to_string(&geraet).unwrap()],
            )
            .unwrap();
        }
        let conn = oeffnen(&pfad).unwrap();
        assert_eq!(
            liste::<Eintrag>(&conn, EINTRAEGE).unwrap().len(),
            1,
            "Einträge bleiben"
        );
        let alt = hole_geraet(&conn, "alt-1").unwrap().expect("Gerät bleibt");
        assert_eq!(alt.geraet.name, "Altgerät");
        assert_eq!(alt.manuell, vec![GruppeId::neu("team")]);
        assert_eq!(alt.oid, None);
        assert!(
            ordner.join("quiver.db.vor-1").exists(),
            "Sicherung liegt da"
        );
        // Zweiter Start migriert nicht noch einmal.
        let conn = oeffnen(&pfad).unwrap();
        assert!(benutzer_liste(&conn).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn sitzung_laeuft_ab() {
        let conn = gedaechtnis();
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        sitzung_anlegen(&conn, "hash", "oid-1", jetzt).unwrap();
        assert!(
            sitzung_holen(&conn, "hash", jetzt + time::Duration::minutes(59))
                .unwrap()
                .is_some()
        );
        // Eine andere Sitzung ohne Nutzung läuft nach 61 Minuten ab.
        sitzung_anlegen(&conn, "hash2", "oid-1", jetzt).unwrap();
        assert!(
            sitzung_holen(&conn, "hash2", jetzt + time::Duration::minutes(61))
                .unwrap()
                .is_none()
        );
        assert!(sitzung_holen(&conn, "hash2", jetzt).unwrap().is_none());
    }

    #[test]
    fn sitzung_hat_absolute_grenze() {
        let conn = gedaechtnis();
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        sitzung_anlegen(&conn, "hash", "oid-1", jetzt).unwrap();
        // Jede Stunde genutzt, aber nach 12 h ist Schluss.
        let mut zeit = jetzt;
        for _ in 0..11 {
            zeit += time::Duration::minutes(59);
            assert!(sitzung_holen(&conn, "hash", zeit).unwrap().is_some());
        }
        assert!(
            sitzung_holen(&conn, "hash", jetzt + time::Duration::hours(12))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pruefspur_haelt_180_tage() {
        let conn = gedaechtnis();
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        pruefspur_schreiben(&conn, jetzt, "test", "wer", "alt").unwrap();
        pruefspur_schreiben(
            &conn,
            jetzt + time::Duration::days(PRUEFSPUR_TAGE + 1),
            "test",
            "wer",
            "neu",
        )
        .unwrap();
        let liste = pruefspur_liste(&conn, 10).unwrap();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].text, "neu");
    }

    #[test]
    fn wirksame_gruppen_vereinigen() {
        let conn = gedaechtnis();
        let a = Gruppe {
            id: GruppeId::neu("a"),
            titel: "A".into(),
            saetze: Vec::new(),
            entra: vec!["entra-a".into()],
        };
        let b = Gruppe {
            id: GruppeId::neu("b"),
            titel: "B".into(),
            saetze: Vec::new(),
            entra: vec!["entra-b".into()],
        };
        setze(&conn, GRUPPEN, "a", &a).unwrap();
        setze(&conn, GRUPPEN, "b", &b).unwrap();
        let wirksam = wirksame_gruppen(
            &conn,
            &[GruppeId::neu("b")],
            &["entra-a".to_string(), "entra-fremd".to_string()],
        )
        .unwrap();
        assert_eq!(wirksam, vec![GruppeId::neu("a"), GruppeId::neu("b")]);
    }
}
