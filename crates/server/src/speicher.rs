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
use serde::{Deserialize, Serialize};
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
///
/// 2 = Aufgabe 23: `entra_gruppen` merkt sich gelöschte Gruppen.
/// 3 = Aufgabe 30: `entra_gruppen` merkt sich den Namen aus Entra und wann er
///     geholt wurde.
/// 4 = Härtung: Alter und letzte Nutzung des Geräte-Tokens, Zähler in der
///     Prüfspur, gemerkte Root-Anmeldeadressen, Indizes für Ablauf und Zeit.
/// 5 = Aufgabe 33: `benutzer_gruppen` – von Hand vergebene Zugehörigkeiten.
/// 6 = Aufgabe 34: `update_stand` merkt sich die zuletzt gelesene
///     Veröffentlichung (eine Zeile; sonst fragte jeder Neustart sofort).
pub const SCHEMA_VERSION: i64 = 6;

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
    manuell_json    TEXT NOT NULL DEFAULT '[]',
    token_erstellt  TEXT,
    token_zuletzt   TEXT
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
    text          TEXT NOT NULL,
    anzahl        INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS pruefspur_zeit ON pruefspur (zeit);
CREATE INDEX IF NOT EXISTS pruefspur_art_akteur ON pruefspur (art, akteur, zeit);
CREATE TABLE IF NOT EXISTS root_ips (
    ip            TEXT PRIMARY KEY,
    zuletzt       TEXT NOT NULL
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
CREATE INDEX IF NOT EXISTS oidc_zustand_gueltig ON oidc_zustand (gueltig_bis);
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
CREATE INDEX IF NOT EXISTS bestaetigungen_gueltig ON bestaetigungen (gueltig_bis);
CREATE TABLE IF NOT EXISTS geraete_codes (
    code_hash     TEXT PRIMARY KEY,
    oid           TEXT NOT NULL,
    tid           TEXT NOT NULL,
    entra_json    TEXT NOT NULL,
    challenge     TEXT NOT NULL,
    name          TEXT NOT NULL,
    gueltig_bis   TEXT NOT NULL
);
-- Der zuletzt gelesene Stand der Veröffentlichung (Aufgabe 34): eine Zeile,
-- damit der Server nach einem Neustart nicht sofort wieder fragt und die
-- Verwaltung etwas zu zeigen hat.
CREATE TABLE IF NOT EXISTS update_stand (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS entra_gruppen (
    id            TEXT PRIMARY KEY,
    anzeigename   TEXT,
    zuletzt       TEXT NOT NULL,
    geloescht_am  TEXT,
    name_entra    TEXT,
    name_entra_am TEXT
);
-- Von Hand vergebene Zugehörigkeit (Aufgabe 33). Sie gehört dem **Benutzer**
-- und überlebt Auffrischung, Neuanmeldung und neue Geräte. `von` ist, wer sie
-- vergeben hat; `seit` wann. Die Entra-Ids stehen weiter in `benutzer`. 
CREATE TABLE IF NOT EXISTS benutzer_gruppen (
    oid    TEXT NOT NULL,
    tid    TEXT NOT NULL,
    gruppe TEXT NOT NULL,
    von    TEXT NOT NULL,
    seit   TEXT NOT NULL,
    PRIMARY KEY (oid, gruppe)
);
CREATE INDEX IF NOT EXISTS benutzer_gruppen_gruppe ON benutzer_gruppen (gruppe);
"#;

/// Datenbank öffnen, sichern (nur bei einer Datei), migrieren, prüfen.
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
        // Über die Sicherungsschnittstelle von SQLite und nicht über eine
        // Dateikopie: so ist der Inhalt des WAL mit im Bild und die Kopie ist
        // in sich stimmig, auch während geschrieben wird.
        sichern(pfad, &sicherung)?;
        // Die Sicherung enthält dieselben Daten wie die Datenbank: nur der
        // Eigentümer darf sie lesen. Und es bleiben die letzten drei.
        crate::rechte::datei_schuetzen(&sicherung);
        if let Some(ordner) = pfad.parent() {
            crate::rechte::sicherungen_kuerzen(ordner);
        }
        tracing::info!(
            "Datenbank gesichert: {} → {}",
            pfad.display(),
            sicherung.display()
        );
    }

    let conn =
        Connection::open(pfad).with_context(|| format!("Datenbank {} öffnen", pfad.display()))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")?;
    if ist_datei {
        // `-wal` und `-shm` entstehen genau hier und tragen dieselben Daten wie
        // die Datenbank: nur der Eigentümer darf sie lesen.
        for anhang in ["", "-wal", "-shm"] {
            let mut name = pfad.as_os_str().to_owned();
            name.push(anhang);
            crate::rechte::datei_schuetzen(Path::new(&name));
        }
    }
    migrieren(&conn)?;
    // Nach der Migration muss die Datenbank in Ordnung sein; sonst lieber
    // nicht weiterarbeiten (die Sicherung von vorhin liegt ja daneben).
    integritaet_pruefen(&conn)?;
    Ok(conn)
}

/// Die Datenbank über die Sicherungsschnittstelle kopieren (mit WAL).
fn sichern(quelle: &Path, ziel: &Path) -> anyhow::Result<()> {
    let von = Connection::open(quelle)
        .with_context(|| format!("Datenbank {} für die Sicherung öffnen", quelle.display()))?;
    let mut nach = Connection::open(ziel)
        .with_context(|| format!("Sicherung {} vor der Migration anlegen", ziel.display()))?;
    let sicherung = rusqlite::backup::Backup::new(&von, &mut nach)
        .with_context(|| format!("Sicherung nach {} beginnen", ziel.display()))?;
    sicherung
        .run_to_completion(100, std::time::Duration::from_millis(20), None)
        .with_context(|| format!("Sicherung nach {} schreiben", ziel.display()))?;
    drop(sicherung);
    // Die Kopie muss für sich stimmig sein, bevor die Migration sie ersetzt.
    integritaet_pruefen(&nach)
}

/// `PRAGMA integrity_check` – meldet nur „ok“ als Erfolg.
fn integritaet_pruefen(conn: &Connection) -> anyhow::Result<()> {
    let befund: String = conn.query_row("PRAGMA integrity_check", [], |zeile| zeile.get(0))?;
    if befund != "ok" {
        anyhow::bail!("integrity_check meldet: {befund}");
    }
    Ok(())
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
    //
    // `ALTER TABLE … ADD COLUMN oid` geht nicht mehr: `oid` ist in SQLite ein
    // Name der rowid, und eine solche Spalte lässt sich nicht nachträglich
    // anhängen („cannot add rowid alias“). Deshalb wird die Tabelle neu
    // aufgebaut – der Weg, den SQLite für Schemaänderungen ohnehin vorsieht.
    let mut manuell_neu = false;
    if ["oid", "tid", "manuell_json"]
        .iter()
        .any(|spalte| !spalte_existiert(conn, GERAETE, spalte).unwrap_or(false))
    {
        manuell_neu = !spalte_existiert(conn, GERAETE, "manuell_json")?;
        conn.execute_batch(
            "DROP TABLE IF EXISTS geraete_neu;
             ALTER TABLE geraete RENAME TO geraete_alt;
             CREATE TABLE geraete (
                 id              TEXT PRIMARY KEY,
                 json            TEXT NOT NULL,
                 token_hash      TEXT NOT NULL UNIQUE,
                 zuletzt_gesehen TEXT,
                 meldung_json    TEXT,
                 oid             TEXT,
                 tid             TEXT,
                 manuell_json    TEXT NOT NULL DEFAULT '[]'
             );
             INSERT INTO geraete
                 (id, json, token_hash, zuletzt_gesehen, meldung_json, oid, tid, manuell_json)
                 SELECT id, json, token_hash, zuletzt_gesehen, meldung_json, NULL, NULL, '[]'
                 FROM geraete_alt;
             DROP TABLE geraete_alt;",
        )?;
    }
    // Aufgabe 23 und 30: alte `entra_gruppen` kennen die neuen Spalten nicht.
    // Jede Stufe ist wiederholbar und einzeln nachholbar.
    for (spalte, typ) in [
        ("geloescht_am", "TEXT"),
        ("name_entra", "TEXT"),
        ("name_entra_am", "TEXT"),
    ] {
        if !spalte_existiert(conn, "entra_gruppen", spalte)? {
            conn.execute(
                &format!("ALTER TABLE entra_gruppen ADD COLUMN {spalte} {typ}"),
                [],
            )?;
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
    // Härtung (Paket 2): Alter und letzte Nutzung des Geräte-Tokens. Was schon
    // da ist, beginnt bei Null – gerechnet ab dieser Migration.
    let jetzt_text = hilfen::als_text(hilfen::jetzt());
    for spalte in ["token_erstellt", "token_zuletzt"] {
        if !spalte_existiert(conn, GERAETE, spalte)? {
            conn.execute(&format!("ALTER TABLE geraete ADD COLUMN {spalte} TEXT"), [])?;
        }
        // Was schon da ist, beginnt bei Null – gerechnet ab dieser Migration.
        conn.execute(
            &format!("UPDATE geraete SET {spalte} = ?1 WHERE {spalte} IS NULL"),
            params![jetzt_text],
        )?;
    }
    // Die Prüfspur zählt gleiche Fehlversuche je Stunde zusammen.
    if !spalte_existiert(conn, "pruefspur", "anzahl")? {
        conn.execute(
            "ALTER TABLE pruefspur ADD COLUMN anzahl INTEGER NOT NULL DEFAULT 1",
            [],
        )?;
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

/// Eine Zeile in `update_stand`: die zuletzt gelesene Veröffentlichung und
/// wann sie gelesen wurde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStandZeile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub veroeffentlichung: Option<quiver_domain::updates::Veroeffentlichung>,
    /// RFC 3339; wann zuletzt erfolgreich gefragt wurde.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zuletzt: Option<String>,
}

/// Der zuletzt gelesene Stand der Veröffentlichung (Aufgabe 34).
///
/// Er liegt in der Datenbank und nicht nur im Speicher: sonst fragte jeder
/// Neustart des Servers sofort wieder, und die Verwaltung hätte nach einem
/// Neustart nichts zu zeigen.
pub fn update_stand(conn: &Connection) -> anyhow::Result<Option<UpdateStandZeile>> {
    let text = conn
        .query_row("SELECT json FROM update_stand WHERE id = 1", [], |zeile| {
            zeile.get::<_, String>(0)
        })
        .optional()?;
    match text {
        Some(text) => Ok(Some(
            serde_json::from_str(&text).context("update_stand: JSON nicht lesbar")?,
        )),
        None => Ok(None),
    }
}

/// Den Stand der Veröffentlichung ablegen (eine Zeile).
pub fn update_stand_setzen(conn: &Connection, stand: &UpdateStandZeile) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO update_stand (id, json) VALUES (1, ?1) \
         ON CONFLICT (id) DO UPDATE SET json = excluded.json",
        params![serde_json::to_string(stand)?],
    )?;
    Ok(())
}

/// Verweise einer Gruppe auf Sätze, die es nicht gibt.
pub fn unbekannte_saetze(conn: &Connection, gruppe: &Gruppe) -> anyhow::Result<Vec<String>> {
    let saetze: Vec<Satz> = liste(conn, SAETZE)?;
    Ok(gruppe
        .saetze
        .iter()
        .map(|id| id.als_str().to_string())
        .filter(|id| !saetze.iter().any(|satz| satz.id.als_str() == id))
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
    /// Wann das Geräte-Token entstand (RFC 3339).
    pub token_erstellt: Option<String>,
    /// Wann das Geräte-Token zuletzt benutzt wurde (RFC 3339).
    pub token_zuletzt: Option<String>,
}

const GERAET_SPALTEN: &str =
    "json, zuletzt_gesehen, meldung_json, oid, tid, manuell_json, token_erstellt, token_zuletzt";

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
        token_erstellt: zeile.get(6)?,
        token_zuletzt: zeile.get(7)?,
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
    let jetzt = hilfen::als_text(hilfen::jetzt());
    conn.execute(
        "INSERT INTO geraete (id, json, token_hash, oid, tid, manuell_json, token_erstellt, \
         token_zuletzt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            geraet.id.als_str(),
            json,
            token_hash,
            oid,
            tid,
            manuell,
            jetzt,
            jetzt
        ],
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
    // Ein neues Token: Alter und letzte Nutzung beginnen von vorn.
    let jetzt = hilfen::als_text(hilfen::jetzt());
    Ok(conn.execute(
        "UPDATE geraete SET token_hash = ?2, oid = ?3, tid = ?4, json = ?5, manuell_json = ?6, \
         token_erstellt = ?7, token_zuletzt = ?7 \
         WHERE id = ?1",
        params![id, token_hash, oid, tid, json, manuell, jetzt],
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

/// Letzte Nutzung des Geräte-Tokens nachziehen.
///
/// Höchstens einmal je Stunde geschrieben: der Wert dient der Altersgrenze und
/// muss dafür nicht auf die Sekunde stimmen; jede Anfrage zu schreiben wäre
/// nur Last auf der einen Verbindung.
pub fn geraet_token_nutzen(
    conn: &Connection,
    id: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<()> {
    let vorhanden: Option<String> = conn
        .query_row(
            "SELECT token_zuletzt FROM geraete WHERE id = ?1",
            params![id],
            |zeile| zeile.get(0),
        )
        .optional()?
        .flatten();
    if vorhanden
        .as_deref()
        .and_then(hilfen::zeit_lesen)
        .is_some_and(|zuletzt| jetzt - zuletzt < time::Duration::hours(1))
    {
        return Ok(());
    }
    conn.execute(
        "UPDATE geraete SET token_zuletzt = ?2 WHERE id = ?1",
        params![id, hilfen::als_text(jetzt)],
    )?;
    Ok(())
}

// ------------------------------------------------------- Root-Anmeldeadressen

/// So lange zählt eine Adresse als „hier kam schon einmal ein Root durch".
pub const ROOT_IP_TAGE: i64 = 30;

/// Eine Adresse merken, von der eine Root-Anmeldung gelang.
pub fn root_ip_merken(conn: &Connection, ip: &str, jetzt: OffsetDateTime) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO root_ips (ip, zuletzt) VALUES (?1, ?2) \
         ON CONFLICT(ip) DO UPDATE SET zuletzt = excluded.zuletzt",
        params![ip, hilfen::als_text(jetzt)],
    )?;
    let grenze = jetzt - time::Duration::days(ROOT_IP_TAGE);
    conn.execute(
        "DELETE FROM root_ips WHERE zuletzt < ?1",
        params![hilfen::als_text(grenze)],
    )?;
    Ok(())
}

/// Kam von dieser Adresse in den letzten [`ROOT_IP_TAGE`] eine Root-Anmeldung?
pub fn root_ip_bekannt(conn: &Connection, ip: &str, jetzt: OffsetDateTime) -> anyhow::Result<bool> {
    let grenze = jetzt - time::Duration::days(ROOT_IP_TAGE);
    Ok(conn
        .query_row(
            "SELECT 1 FROM root_ips WHERE ip = ?1 AND zuletzt >= ?2",
            params![ip, hilfen::als_text(grenze)],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
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

impl Benutzer {
    /// Darf dieser Benutzer noch durch? Nachprüfen kann `neu_anmelden` setzen
    /// oder sperren; Sitzungen enden dann sofort (Aufgabe 26, Punkt 1).
    pub fn anmeldung_gueltig(&self) -> bool {
        !self.gesperrt && self.token_stand == "gueltig"
    }
}

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

/// Benutzer löschen: Token, Sitzungen, Geräte und Zugehörigkeiten gehen mit.
pub fn benutzer_loeschen(conn: &Connection, oid: &str) -> anyhow::Result<bool> {
    conn.execute("DELETE FROM geraete WHERE oid = ?1", params![oid])?;
    conn.execute("DELETE FROM sitzungen WHERE oid = ?1", params![oid])?;
    // Eine Zuordnung ohne Benutzer wäre eine Zeile, die niemand mehr sieht.
    handzuordnungen_von_loeschen(conn, oid)?;
    Ok(conn.execute("DELETE FROM benutzer WHERE oid = ?1", params![oid])? > 0)
}

/// Der Root-Benutzer: höchstens einer, Kennung `root`.
pub const ROOT_OID: &str = "root";

pub fn root_holen(conn: &Connection) -> anyhow::Result<Option<Benutzer>> {
    benutzer_holen(conn, ROOT_OID)
}

/// Den Root **einmalig** anlegen (Ersteinrichtung, Aufgabe 25).
///
/// Prüfung „kein Root“ und Einfügen liegen in **einer** Transaktion
/// (`BEGIN IMMEDIATE`): bei gleichzeitigen Aufrufen gewinnt genau einer; die
/// anderen bekommen `false`.
pub fn root_einrichten(
    conn: &mut Connection,
    email: &str,
    passwort_hash: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<bool> {
    let transaktion = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let vorhanden: bool = transaktion
        .query_row(
            "SELECT 1 FROM benutzer WHERE oid = ?1",
            params![ROOT_OID],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if vorhanden {
        // Rollback beim Fallenlassen der Transaktion.
        return Ok(false);
    }
    let jetzt_text = hilfen::als_text(jetzt);
    transaktion.execute(
        "INSERT INTO benutzer (oid, tid, email, name, rolle, gesperrt, entra_json, token, \
         passwort, token_erneuert, token_stand, erstellt, zuletzt) \
         VALUES (?1, 'root', ?2, 'Root', 'root', 0, '[]', NULL, ?3, NULL, 'neu_anmelden', ?4, ?4)",
        params![ROOT_OID, email, passwort_hash, jetzt_text],
    )?;
    transaktion.commit()?;
    Ok(true)
}

/// Alle Sitzungen eines Benutzers beenden (Sperre, Rollenentzug, Löschung).
pub fn sitzungen_von_user_loeschen(conn: &Connection, oid: &str) -> anyhow::Result<usize> {
    Ok(conn.execute("DELETE FROM sitzungen WHERE oid = ?1", params![oid])?)
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
    Ok(())
}

/// Ein Fehlversuch je Adresse und Stunde: ein Eintrag mit Zähler statt einer
/// Zeile je Versuch.
///
/// Die Stunde steckt im Zeitstempel (Anfang `JJJJ-MM-TTTHH`); der Zähler wächst
/// im selben Eintrag. Das hält die Spur auch bei vielen Versuchen klein.
pub fn pruefspur_fehlschlag(
    conn: &Connection,
    jetzt: OffsetDateTime,
    akteur: &str,
    text: &str,
) -> anyhow::Result<()> {
    let zeit = hilfen::als_text(jetzt);
    let stunde = zeit.get(..13).unwrap_or(&zeit);
    let getroffen = conn.execute(
        "UPDATE pruefspur SET anzahl = anzahl + 1 \
         WHERE art = 'fehlschlag' AND akteur = ?1 AND substr(zeit, 1, 13) = ?2",
        params![akteur, stunde],
    )?;
    if getroffen > 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO pruefspur (zeit, art, akteur, text, anzahl) VALUES (?1, 'fehlschlag', ?2, ?3, 1)",
        params![zeit, akteur, text],
    )?;
    Ok(())
}

/// Alte Einträge entfernen – wird höchstens stündlich aufgerufen, nicht bei
/// jedem Schreiben.
pub fn pruefspur_aufraeumen(conn: &Connection, jetzt: OffsetDateTime) -> anyhow::Result<usize> {
    let grenze = jetzt - time::Duration::days(PRUEFSPUR_TAGE);
    Ok(conn.execute(
        "DELETE FROM pruefspur WHERE zeit < ?1",
        params![hilfen::als_text(grenze)],
    )?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruefspurZeile {
    pub zeit: String,
    pub art: String,
    pub akteur: String,
    pub text: String,
    /// Wie oft dasselbe (Fehlversuche einer Adresse in einer Stunde).
    pub anzahl: u32,
}

/// Steht zu diesem Tag schon ein Hinweis mit diesem Anfang in der Prüfspur?
///
/// Damit erscheint die Zertifikatswarnung höchstens einmal je Tag
/// (Aufgabe 23), auch wenn viele Aufrufe sie prüfen.
pub fn pruefspur_hat_am_tag(
    conn: &Connection,
    art: &str,
    tag: &str,
    text_anfang: &str,
) -> anyhow::Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM pruefspur WHERE art = ?1 AND zeit LIKE ?2 AND text LIKE ?3 LIMIT 1",
            params![art, format!("{tag}%"), format!("{text_anfang}%")],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub fn pruefspur_liste(conn: &Connection, limit: u32) -> anyhow::Result<Vec<PruefspurZeile>> {
    let mut stmt = conn.prepare(
        "SELECT zeit, art, akteur, text, anzahl FROM pruefspur ORDER BY id DESC LIMIT ?1",
    )?;
    let zeilen = stmt.query_map(params![i64::from(limit.clamp(1, 1000))], |zeile| {
        Ok(PruefspurZeile {
            zeit: zeile.get(0)?,
            art: zeile.get(1)?,
            akteur: zeile.get(2)?,
            text: zeile.get(3)?,
            anzahl: zeile.get::<_, i64>(4).unwrap_or(1).max(1) as u32,
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

/// Wie viele Anmeldevorgänge gerade offen sind (für die Obergrenze).
pub fn oidc_offen(conn: &Connection) -> anyhow::Result<usize> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM oidc_zustand", [], |zeile| {
        zeile.get(0)
    })?;
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
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

/// Höchstens so viele Gruppen legt der Server aus Entra-Ids an (Aufgabe 23).
pub const MAX_ENTRA_GRUPPEN: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntraGruppeZeile {
    pub id: String,
    /// Titel der quiver-Gruppe, die diese Id trägt.
    pub anzeigename: Option<String>,
    pub zuletzt: String,
    /// Kennung der quiver-Gruppe, falls es sie gibt.
    pub gruppe: Option<GruppeId>,
    /// Vom Admin gelöscht: bleibt gelöscht, bis er sie wieder aufnimmt.
    pub geloescht: bool,
    /// Anzeigename aus Entra (Aufgabe 30), nur Anzeige.
    pub name_entra: Option<String>,
    /// Wann er geholt wurde (RFC 3339).
    pub name_entra_am: Option<String>,
}

/// Den Titel einer automatisch angelegten Gruppe: „Entra-Gruppe <erste 8>“.
fn entra_titel(id: &str) -> String {
    let kurz: String = id.chars().take(8).collect();
    format!("Entra-Gruppe {kurz}")
}

/// Eine Id in der Merkliste anlegen oder auffrischen.
fn entra_zeile_merken(conn: &Connection, id: &str, jetzt: OffsetDateTime) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO entra_gruppen (id, anzeigename, zuletzt) VALUES (?1, NULL, ?2) \
         ON CONFLICT(id) DO UPDATE SET zuletzt = ?2",
        params![id, hilfen::als_text(jetzt)],
    )?;
    Ok(())
}

fn entra_geloescht_am(conn: &Connection, id: &str) -> anyhow::Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT geloescht_am FROM entra_gruppen WHERE id = ?1",
            params![id],
            |zeile| zeile.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// Der zuletzt aus Entra geholte Name dieser Id, falls es einen gibt.
fn entra_name(conn: &Connection, id: &str) -> anyhow::Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT name_entra FROM entra_gruppen WHERE id = ?1",
            params![id],
            |zeile| zeile.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// Der Titel einer neuen Gruppe: der Name aus Entra, sonst der automatische.
fn titel_fuer(conn: &Connection, id: &str) -> anyhow::Result<String> {
    Ok(entra_name(conn, id)?.unwrap_or_else(|| entra_titel(id)))
}

/// Gruppen, die diese Entra-Id tragen.
fn gruppe_mit_entra<'a>(gruppen: &'a [Gruppe], id: &str) -> Option<&'a Gruppe> {
    gruppen
        .iter()
        .find(|gruppe| gruppe.entra.iter().any(|e| e.eq_ignore_ascii_case(id)))
}

/// Jede gesehene Entra-Id **einmal** als quiver-Gruppe anlegen, falls noch
/// keine Gruppe sie trägt (Aufgabe 23). Titel und Sätze werden nie
/// überschrieben; gelöschte Ids bleiben gelöscht.
///
/// Gibt die neu angelegten Gruppen zurück (für die Prüfspur).
pub fn entra_gruppen_merken(
    conn: &Connection,
    ids: &[String],
    jetzt: OffsetDateTime,
) -> anyhow::Result<Vec<Gruppe>> {
    let mut gruppen: Vec<Gruppe> = liste(conn, GRUPPEN)?;
    let mut anzahl = gruppen
        .iter()
        .filter(|gruppe| !gruppe.entra.is_empty())
        .count();
    let mut neu = Vec::new();
    for id in ids {
        let id = id.to_ascii_lowercase();
        entra_zeile_merken(conn, &id, jetzt)?;
        // Trägt schon eine Gruppe diese Id? Dann gehören Titel und Sätze dem
        // Admin – nichts wird angefasst.
        if gruppe_mit_entra(&gruppen, &id).is_some() {
            continue;
        }
        if entra_geloescht_am(conn, &id)?.is_some() {
            // Der Admin hat sie gelöscht; sie bleibt gelöscht.
            continue;
        }
        if anzahl >= MAX_ENTRA_GRUPPEN {
            tracing::warn!(
                "Entra-Gruppe {id} wird nicht angelegt: es gibt schon {MAX_ENTRA_GRUPPEN} \
                 Gruppen aus Entra"
            );
            continue;
        }
        if gruppen.iter().any(|gruppe| gruppe.id.als_str() == id) {
            // Die Kennung ist von einer anderen Gruppe belegt; sie gehört dem
            // Admin und wird nicht überschrieben.
            tracing::warn!(
                "Entra-Gruppe {id} wird nicht angelegt: die Kennung trägt schon eine andere Gruppe"
            );
            continue;
        }
        let gruppe = Gruppe {
            id: GruppeId::neu(id.clone()),
            titel: titel_fuer(conn, &id)?,
            saetze: Vec::new(),
            entra: vec![id.clone()],
        };
        setze(conn, GRUPPEN, &id, &gruppe)?;
        anzahl += 1;
        gruppen.push(gruppe.clone());
        neu.push(gruppe);
    }
    Ok(neu)
}

/// Wie [`entra_gruppen_merken`], plus Prüfspur „angelegt durch …“.
pub fn entra_gruppen_merken_und_spur(
    conn: &Connection,
    ids: &[String],
    jetzt: OffsetDateTime,
    durch: &str,
    akteur: &str,
) -> anyhow::Result<Vec<Gruppe>> {
    let neu = entra_gruppen_merken(conn, ids, jetzt)?;
    for gruppe in &neu {
        pruefspur_schreiben(
            conn,
            jetzt,
            "gruppe",
            akteur,
            &format!("{} angelegt durch {durch}", gruppe.id),
        )?;
    }
    Ok(neu)
}

pub fn entra_gruppen_liste(conn: &Connection) -> anyhow::Result<Vec<EntraGruppeZeile>> {
    let gruppen: Vec<Gruppe> = liste(conn, GRUPPEN)?;
    let mut stmt = conn.prepare(
        "SELECT id, anzeigename, zuletzt, geloescht_am, name_entra, name_entra_am \
         FROM entra_gruppen ORDER BY id",
    )?;
    let zeilen = stmt.query_map([], |zeile| {
        Ok((
            zeile.get::<_, String>(0)?,
            zeile.get::<_, Option<String>>(1)?,
            zeile.get::<_, String>(2)?,
            zeile.get::<_, Option<String>>(3)?,
            zeile.get::<_, Option<String>>(4)?,
            zeile.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut aus = Vec::new();
    for zeile in zeilen {
        let (id, gespeicherter_name, zuletzt, geloescht_am, name_entra, name_entra_am) = zeile?;
        let gruppe = gruppe_mit_entra(&gruppen, &id);
        aus.push(EntraGruppeZeile {
            anzeigename: gruppe.map(|g| g.titel.clone()).or(gespeicherter_name),
            gruppe: gruppe.map(|g| g.id.clone()),
            geloescht: geloescht_am.is_some(),
            id,
            zuletzt,
            name_entra,
            name_entra_am,
        });
    }
    Ok(aus)
}

/// Welche dieser Ids brauchen einen (neuen) Namen aus Entra (Aufgabe 30)?
///
/// Fehlt der Name oder ist er älter als [`crate::graph::NAME_STUNDEN`], kommt
/// die Id in die Liste – die ältesten zuerst.
pub fn entra_namen_fehlen(
    conn: &Connection,
    ids: &[String],
    jetzt: OffsetDateTime,
) -> anyhow::Result<Vec<String>> {
    let grenze = hilfen::als_text(jetzt - time::Duration::hours(crate::graph::NAME_STUNDEN));
    let mut stmt = conn.prepare(
        "SELECT id FROM entra_gruppen \
         WHERE name_entra IS NULL OR name_entra_am IS NULL OR name_entra_am < ?1 \
         ORDER BY COALESCE(name_entra_am, ''), id",
    )?;
    let faellig = stmt
        .query_map(params![grenze], |zeile| zeile.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let gesucht: std::collections::HashSet<String> =
        ids.iter().map(|id| id.to_ascii_lowercase()).collect();
    Ok(faellig
        .into_iter()
        .filter(|id| gesucht.contains(&id.to_ascii_lowercase()))
        .collect())
}

/// Einen aus Entra geholten Namen übernehmen (Aufgabe 30).
///
/// Der Name wird immer gemerkt. Der Titel der quiver-Gruppe folgt **nur**,
/// solange er der automatische ist oder dem zuletzt übernommenen Namen
/// entspricht; einen Titel des Admins überschreibt der Server nie.
/// `Ok(true)` heißt: der Titel hat sich geändert (für die Prüfspur).
pub fn entra_name_uebernehmen(
    conn: &Connection,
    id: &str,
    name: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<bool> {
    let id = id.to_ascii_lowercase();
    let bisher = entra_name(conn, &id)?;
    conn.execute(
        "UPDATE entra_gruppen SET name_entra = ?2, name_entra_am = ?3 WHERE id = ?1",
        params![id, name, hilfen::als_text(jetzt)],
    )?;
    let gruppen: Vec<Gruppe> = liste(conn, GRUPPEN)?;
    let Some(mut gruppe) = gruppe_mit_entra(&gruppen, &id).cloned() else {
        return Ok(false);
    };
    let automatisch = gruppe.titel == entra_titel(&id)
        || bisher
            .as_deref()
            .is_some_and(|bisher| gruppe.titel == bisher);
    if !automatisch || gruppe.titel == name {
        return Ok(false);
    }
    gruppe.titel = name.to_string();
    setze(conn, GRUPPEN, gruppe.id.als_str(), &gruppe)?;
    Ok(true)
}

/// Eine gelöschte Entra-Gruppe wieder aufnehmen (oder eine neue anlegen): die
/// quiver-Gruppe entsteht mit dieser Id, ohne Sätze. Der Titel darf vorgegeben
/// werden; sonst gilt der gemerkte oder „Entra-Gruppe <erste 8>“.
pub fn entra_gruppe_aufnehmen(
    conn: &Connection,
    id: &str,
    titel: Option<&str>,
    jetzt: OffsetDateTime,
) -> anyhow::Result<Gruppe> {
    let id = id.to_ascii_lowercase();
    entra_zeile_merken(conn, &id, jetzt)?;
    let gruppen: Vec<Gruppe> = liste(conn, GRUPPEN)?;
    let mut gruppe = match gruppe_mit_entra(&gruppen, &id) {
        Some(gruppe) => gruppe.clone(),
        None => {
            if gruppen.iter().any(|g| g.id.als_str() == id) {
                anyhow::bail!("die Kennung {id} trägt schon eine andere Gruppe");
            }
            Gruppe {
                id: GruppeId::neu(id.clone()),
                titel: titel_fuer(conn, &id)?,
                saetze: Vec::new(),
                entra: vec![id.clone()],
            }
        }
    };
    // Von Hand aufgenommen: das Löschmerkmal gilt nicht mehr.
    conn.execute(
        "UPDATE entra_gruppen SET geloescht_am = NULL WHERE id = ?1",
        params![id],
    )?;
    if let Some(titel) = titel.map(str::trim).filter(|t| !t.is_empty()) {
        gruppe.titel = titel.to_string();
    } else if gruppe.titel.trim().is_empty() {
        gruppe.titel = entra_titel(&id);
    }
    setze(conn, GRUPPEN, gruppe.id.als_str(), &gruppe)?;
    Ok(gruppe)
}

/// Eine Entra-Gruppe löschen: die Id bleibt gemerkt und wird nicht wieder
/// angelegt, bis der Admin sie aufnimmt.
pub fn entra_gruppe_loeschen_merken(
    conn: &Connection,
    entra_ids: &[String],
    jetzt: OffsetDateTime,
) -> anyhow::Result<()> {
    for id in entra_ids {
        let id = id.to_ascii_lowercase();
        entra_zeile_merken(conn, &id, jetzt)?;
        conn.execute(
            "UPDATE entra_gruppen SET geloescht_am = ?2 WHERE id = ?1",
            params![id, hilfen::als_text(jetzt)],
        )?;
    }
    Ok(())
}

/// Alle Geräte eines Benutzers neu rechnen: die Entra-Ids bestimmen die
/// wirksamen Gruppen. Änderungen wirken sofort auf verbundene Geräte.
pub fn geraete_gruppen_neu(conn: &Connection, oid: &str, entra: &[String]) -> anyhow::Result<bool> {
    let mut geaendert = false;
    for zeile in geraete_von_user(conn, oid)? {
        let wirksam = wirksame_gruppen(conn, Some(oid), &zeile.manuell, entra)?;
        if wirksam != zeile.geraet.gruppen {
            let mut geraet = zeile.geraet;
            geraet.gruppen = wirksam;
            aendere_geraet(conn, &geraet)?;
            geaendert = true;
        }
    }
    Ok(geaendert)
}

/// Wirksame Gruppen eines Geräts: die am Gerät gesetzten **und** die über die
/// Entra-Ids des Benutzers zugeordneten **und** die von Hand vergebenen
/// (SPEC §5, Aufgaben 20 und 33).
///
/// **Eine Stelle** für Soll, Verwaltung und Nachprüfen. Gruppen, die es nicht
/// mehr gibt, zählen nicht (gelöschte Gruppe → Zuordnung verschwindet); ein
/// gesperrter oder gelöschter Benutzer kommt hier gar nicht erst durch, weil
/// beide Wege vorher abweisen.
pub fn wirksame_gruppen(
    conn: &Connection,
    oid: Option<&str>,
    manuell: &[GruppeId],
    entra: &[String],
) -> anyhow::Result<Vec<GruppeId>> {
    let gruppen: Vec<Gruppe> = liste(conn, GRUPPEN)?;
    let hand = match oid {
        Some(oid) => handzuordnungen_von(conn, oid)?,
        None => Vec::new(),
    };
    let mut aus: Vec<GruppeId> = manuell
        .iter()
        .filter(|id| gruppen.iter().any(|gruppe| &gruppe.id == *id))
        .cloned()
        .collect();
    for zuordnung in hand {
        if gruppen.iter().any(|gruppe| gruppe.id == zuordnung.gruppe)
            && !aus.contains(&zuordnung.gruppe)
        {
            aus.push(zuordnung.gruppe);
        }
    }
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

/// Alle wirksamen Gruppen eines Benutzers über **alle** seine Geräte – die
/// Sicht der Verwaltung (Aufgabe 33).
///
/// Ein gesperrter oder gelöschter Benutzer hat keine wirksamen Gruppen: seine
/// Zuordnungen wirken nicht.
pub fn wirksame_gruppen_benutzer(conn: &Connection, oid: &str) -> anyhow::Result<Vec<GruppeId>> {
    let Some(benutzer) = benutzer_holen(conn, oid)? else {
        return Ok(Vec::new());
    };
    if benutzer.gesperrt {
        return Ok(Vec::new());
    }
    let mut aus: Vec<GruppeId> = Vec::new();
    for zeile in geraete_von_user(conn, oid)? {
        for id in wirksame_gruppen(conn, Some(oid), &zeile.manuell, &benutzer.entra)? {
            if !aus.contains(&id) {
                aus.push(id);
            }
        }
    }
    if aus.is_empty() {
        // Ohne Gerät gibt es trotzdem Zugehörigkeiten: sie wirken, sobald ein
        // Gerät dazukommt.
        aus = wirksame_gruppen(conn, Some(oid), &[], &benutzer.entra)?;
    }
    aus.sort();
    Ok(aus)
}

// ------------------------------------------- Zugehörigkeiten von Hand (33)

/// Eine von Hand vergebene Zugehörigkeit: Benutzer → Gruppe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handzuordnung {
    pub oid: String,
    pub gruppe: GruppeId,
    /// Wer sie vergeben hat (E-Mail, Name oder Objekt-Id).
    pub von: String,
    /// Wann (RFC 3339).
    pub seit: String,
}

fn handzuordnung_aus(zeile: &rusqlite::Row<'_>) -> rusqlite::Result<Handzuordnung> {
    Ok(Handzuordnung {
        oid: zeile.get(0)?,
        gruppe: GruppeId::neu(zeile.get::<_, String>(1)?),
        von: zeile.get(2)?,
        seit: zeile.get(3)?,
    })
}

/// Alle Zugehörigkeiten eines Benutzers, die von Hand vergeben wurden.
pub fn handzuordnungen_von(conn: &Connection, oid: &str) -> anyhow::Result<Vec<Handzuordnung>> {
    let mut stmt = conn.prepare(
        "SELECT oid, gruppe, von, seit FROM benutzer_gruppen WHERE oid = ?1 ORDER BY gruppe",
    )?;
    let zeilen = stmt.query_map(params![oid], handzuordnung_aus)?;
    Ok(zeilen.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Alle Zugehörigkeiten zu einer Gruppe, die von Hand vergeben wurden.
pub fn handzuordnungen_zu_gruppe(
    conn: &Connection,
    gruppe: &str,
) -> anyhow::Result<Vec<Handzuordnung>> {
    let mut stmt = conn.prepare(
        "SELECT oid, gruppe, von, seit FROM benutzer_gruppen WHERE gruppe = ?1 ORDER BY oid",
    )?;
    let zeilen = stmt.query_map(params![gruppe], handzuordnung_aus)?;
    Ok(zeilen.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Eine Zugehörigkeit von Hand setzen. `false` heißt: sie gab es schon, und
/// wer sie wann vergeben hat, bleibt stehen (kein zweiter Eintrag).
pub fn handzuordnung_setzen(
    conn: &Connection,
    oid: &str,
    tid: &str,
    gruppe: &GruppeId,
    von: &str,
    jetzt: OffsetDateTime,
) -> anyhow::Result<bool> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO benutzer_gruppen (oid, tid, gruppe, von, seit) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![oid, tid, gruppe.als_str(), von, hilfen::als_text(jetzt)],
    )? > 0)
}

/// Eine von Hand vergebene Zugehörigkeit entfernen; `false` heißt: es gab keine.
pub fn handzuordnung_entfernen(conn: &Connection, oid: &str, gruppe: &str) -> anyhow::Result<bool> {
    Ok(conn.execute(
        "DELETE FROM benutzer_gruppen WHERE oid = ?1 AND gruppe = ?2",
        params![oid, gruppe],
    )? > 0)
}

/// Alle Zugehörigkeiten eines Benutzers entfernen (Löschung des Benutzers).
pub fn handzuordnungen_von_loeschen(conn: &Connection, oid: &str) -> anyhow::Result<usize> {
    Ok(conn.execute("DELETE FROM benutzer_gruppen WHERE oid = ?1", params![oid])?)
}

/// Alle Zugehörigkeiten zu einer Gruppe entfernen (Löschung der Gruppe).
pub fn handzuordnungen_zu_gruppe_loeschen(
    conn: &Connection,
    gruppe: &str,
) -> anyhow::Result<usize> {
    Ok(conn.execute(
        "DELETE FROM benutzer_gruppen WHERE gruppe = ?1",
        params![gruppe],
    )?)
}

// ---------------------------------------------------- Datenbankänderung sehen

/// `PRAGMA data_version` – ändert sich, wenn eine andere Verbindung schreibt.
pub fn data_version(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row("PRAGMA data_version", [], |zeile| zeile.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_domain::{Herkunft, Laufzeit, SatzId};

    /// Die SQLite-Fassung, die mindestens mitgebaut werden muss.
    ///
    /// rusqlite 0.43 bringt 3.54.1 mit. Der Test hält die Grenze fest: wer
    /// rusqlite zurückdreht, merkt es hier und nicht erst im Betrieb.
    const SQLITE_MINDESTENS: i32 = 3_054_001;

    #[test]
    fn sqlite_fassung_ist_mindestens_die_behobene() {
        let conn = gedaechtnis();
        let text: String = conn
            .query_row("SELECT sqlite_version()", [], |zeile| zeile.get(0))
            .expect("Version lesbar");
        let teile: Vec<i32> = text
            .split('.')
            .map(|teil| teil.parse().unwrap_or(0))
            .collect();
        assert_eq!(teile.len(), 3, "unerwartete Form: {text}");
        let nummer = teile[0] * 1_000_000 + teile[1] * 1_000 + teile[2];
        assert!(
            nummer >= SQLITE_MINDESTENS,
            "SQLite {text} ist älter als die behobene Fassung"
        );
        // Die Fassung, gegen die gebaut wurde, ist dieselbe wie die im Betrieb.
        assert_eq!(rusqlite::version(), text);
        assert!(rusqlite::version_number() >= SQLITE_MINDESTENS);
    }

    #[cfg(unix)]
    #[test]
    fn datenbank_und_anhaenge_gehoeren_nur_dem_eigentuemer() {
        use std::os::unix::fs::PermissionsExt;
        let ordner = ordner("rechte");
        let pfad = ordner.join("quiver.db");
        crate::rechte::umask_setzen();
        let _conn = oeffnen(&pfad).expect("Datenbank");
        assert!(pfad.exists(), "die Datenbank liegt da");
        for anhang in ["", "-wal", "-shm"] {
            let datei = ordner.join(format!("quiver.db{anhang}"));
            let Ok(meta) = std::fs::metadata(&datei) else {
                continue;
            };
            assert_eq!(meta.permissions().mode() & 0o777, 0o600, "{datei:?}");
        }
        let _ = std::fs::remove_dir_all(&ordner);
    }

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
            ordner
                .join(format!("quiver.db.vor-{SCHEMA_VERSION}"))
                .exists(),
            "Sicherung liegt da"
        );
        // Zweiter Start migriert nicht noch einmal.
        let conn = oeffnen(&pfad).unwrap();
        assert!(benutzer_liste(&conn).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Aufgabe 33: Eine alte Datenbank (Schema 4) bekommt die Tabelle für
    /// Zugehörigkeiten von Hand, und alles Vorhandene bleibt stehen.
    #[test]
    fn migration_legt_die_zuordnungstabelle_an_und_behaelt_die_daten() {
        let ordner = ordner("zuordnung-migration");
        let pfad = ordner.join("quiver.db");
        {
            let conn = Connection::open(&pfad).unwrap();
            conn.execute_batch(
                "CREATE TABLE eintraege (id TEXT PRIMARY KEY, json TEXT NOT NULL);
                 CREATE TABLE saetze    (id TEXT PRIMARY KEY, json TEXT NOT NULL);
                 CREATE TABLE gruppen   (id TEXT PRIMARY KEY, json TEXT NOT NULL);
                 CREATE TABLE geraete (
                    id TEXT PRIMARY KEY, json TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE,
                    zuletzt_gesehen TEXT, meldung_json TEXT, oid TEXT, tid TEXT,
                    manuell_json TEXT NOT NULL DEFAULT '[]',
                    token_erstellt TEXT, token_zuletzt TEXT);
                 CREATE TABLE benutzer (
                    oid TEXT PRIMARY KEY, tid TEXT NOT NULL, email TEXT, name TEXT,
                    rolle TEXT NOT NULL DEFAULT 'benutzer',
                    gesperrt INTEGER NOT NULL DEFAULT 0,
                    entra_json TEXT NOT NULL DEFAULT '[]', token TEXT, passwort TEXT,
                    token_erneuert TEXT, token_stand TEXT NOT NULL DEFAULT 'neu_anmelden',
                    erstellt TEXT NOT NULL, zuletzt TEXT);
                 PRAGMA user_version = 4;",
            )
            .unwrap();
            let gruppe = Gruppe {
                id: GruppeId::neu("team"),
                titel: "Team".into(),
                saetze: Vec::new(),
                entra: Vec::new(),
            };
            setze(&conn, GRUPPEN, "team", &gruppe).unwrap();
            conn.execute(
                "INSERT INTO benutzer (oid, tid, email, rolle, erstellt) \
                 VALUES ('oid-1', 'tenant', 'test@example.test', 'benutzer', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let conn = oeffnen(&pfad).unwrap();
        assert!(
            ordner
                .join(format!("quiver.db.vor-{SCHEMA_VERSION}"))
                .exists(),
            "Sicherung vor der Migration liegt daneben"
        );
        // Die neue Tabelle ist da und leer: nichts wurde erfunden.
        assert!(handzuordnungen_von(&conn, "oid-1").unwrap().is_empty());
        assert_eq!(
            liste::<Gruppe>(&conn, GRUPPEN).unwrap().len(),
            1,
            "Gruppen bleiben"
        );
        assert_eq!(benutzer_liste(&conn).unwrap().len(), 1, "Benutzer bleiben");
        // Und sie nimmt eine Zugehörigkeit auf.
        assert!(handzuordnung_setzen(
            &conn,
            "oid-1",
            "tenant",
            &GruppeId::neu("team"),
            "root@example.test",
            jetzt()
        )
        .unwrap());
        assert_eq!(handzuordnungen_von(&conn, "oid-1").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Aufgabe 33: Zugehörigkeiten von Hand sind einmalig, bleiben stehen und
    /// verschwinden mit Benutzer oder Gruppe.
    #[test]
    fn zuordnungen_von_hand_sind_einmalig_und_verschwinden_mit_ihrem_ziel() {
        let conn = gedaechtnis();
        let team = Gruppe {
            id: GruppeId::neu("team"),
            titel: "Team".into(),
            saetze: Vec::new(),
            entra: Vec::new(),
        };
        setze(&conn, GRUPPEN, "team", &team).unwrap();
        assert!(
            handzuordnung_setzen(&conn, "oid-1", "t", &team.id, "wer@example.test", jetzt())
                .unwrap(),
            "die erste Vergabe ist neu"
        );
        // Zweimal vergeben ändert nichts: wer und wann bleiben die ersten.
        assert!(!handzuordnung_setzen(
            &conn,
            "oid-1",
            "t",
            &team.id,
            "andere@example.test",
            jetzt()
        )
        .unwrap());
        let zeilen = handzuordnungen_von(&conn, "oid-1").unwrap();
        assert_eq!(zeilen.len(), 1);
        assert_eq!(zeilen[0].von, "wer@example.test");
        assert_eq!(handzuordnungen_zu_gruppe(&conn, "team").unwrap().len(), 1);
        assert!(handzuordnung_entfernen(&conn, "oid-1", "team").unwrap());
        assert!(!handzuordnung_entfernen(&conn, "oid-1", "team").unwrap());
        assert!(handzuordnungen_von(&conn, "oid-1").unwrap().is_empty());

        // Mit dem Benutzer geht die Zuordnung; mit der Gruppe ebenso.
        handzuordnung_setzen(&conn, "oid-1", "t", &team.id, "wer", jetzt()).unwrap();
        handzuordnungen_von_loeschen(&conn, "oid-1").unwrap();
        assert!(handzuordnungen_zu_gruppe(&conn, "team").unwrap().is_empty());
        handzuordnung_setzen(&conn, "oid-2", "t", &team.id, "wer", jetzt()).unwrap();
        handzuordnungen_zu_gruppe_loeschen(&conn, "team").unwrap();
        assert!(handzuordnungen_von(&conn, "oid-2").unwrap().is_empty());
    }

    /// Aufgabe 33: wirksam sind Entra ∪ von Hand ∪ Gerät (alt) – und nur
    /// Gruppen, die es noch gibt.
    #[test]
    fn wirksame_gruppen_vereinigen_entra_hand_und_geraet() {
        let conn = gedaechtnis();
        let gruppe = |id: &str, entra: Vec<String>| Gruppe {
            id: GruppeId::neu(id),
            titel: id.to_string(),
            saetze: Vec::new(),
            entra,
        };
        setze(
            &conn,
            GRUPPEN,
            "entra",
            &gruppe("entra", vec!["ENTRA-A".into()]),
        )
        .unwrap();
        setze(&conn, GRUPPEN, "hand", &gruppe("hand", Vec::new())).unwrap();
        setze(&conn, GRUPPEN, "geraet", &gruppe("geraet", Vec::new())).unwrap();
        handzuordnung_setzen(&conn, "oid-1", "t", &GruppeId::neu("hand"), "wer", jetzt()).unwrap();
        let wirksam = wirksame_gruppen(
            &conn,
            Some("oid-1"),
            &[GruppeId::neu("geraet"), GruppeId::neu("geloescht")],
            &["entra-a".to_string()],
        )
        .unwrap();
        assert_eq!(
            wirksam,
            vec![
                GruppeId::neu("entra"),
                GruppeId::neu("geraet"),
                GruppeId::neu("hand")
            ]
        );
        // Ohne Benutzer zählt nur, was am Gerät und in Entra steht.
        let ohne = wirksame_gruppen(&conn, None, &[GruppeId::neu("geraet")], &[]).unwrap();
        assert_eq!(ohne, vec![GruppeId::neu("geraet")]);
    }

    /// Aufgabe 33: Ein gesperrter Benutzer hat keine wirksamen Gruppen; seine
    /// Zuordnungen bleiben aber gespeichert.
    #[test]
    fn gesperrter_benutzer_hat_keine_wirksamen_gruppen() {
        let conn = gedaechtnis();
        let gruppe = Gruppe {
            id: GruppeId::neu("team"),
            titel: "Team".into(),
            saetze: Vec::new(),
            entra: Vec::new(),
        };
        setze(&conn, GRUPPEN, "team", &gruppe).unwrap();
        conn.execute(
            "INSERT INTO benutzer (oid, tid, rolle, gesperrt, erstellt) \
             VALUES ('oid-1', 't', 'benutzer', 0, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        handzuordnung_setzen(&conn, "oid-1", "t", &gruppe.id, "wer", jetzt()).unwrap();
        assert_eq!(
            wirksame_gruppen_benutzer(&conn, "oid-1").unwrap(),
            vec![GruppeId::neu("team")]
        );
        conn.execute("UPDATE benutzer SET gesperrt = 1 WHERE oid = 'oid-1'", [])
            .unwrap();
        assert!(wirksame_gruppen_benutzer(&conn, "oid-1")
            .unwrap()
            .is_empty());
        // Gelöscht: auch nichts.
        assert!(wirksame_gruppen_benutzer(&conn, "fehlt")
            .unwrap()
            .is_empty());
        assert_eq!(handzuordnungen_von(&conn, "oid-1").unwrap().len(), 1);
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

    /// Härtung Punkt 11: vorhandene Geräte beginnen bei Null – Alter und letzte
    /// Nutzung tragen den Zeitpunkt der Migration.
    #[test]
    fn vorhandene_geraete_beginnen_bei_null() {
        let ordner = ordner("token-alter-migration");
        let pfad = ordner.join("quiver.db");
        {
            let conn = Connection::open(&pfad).unwrap();
            conn.execute_batch(
                "CREATE TABLE geraete (
                    id TEXT PRIMARY KEY, json TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE,
                    zuletzt_gesehen TEXT, meldung_json TEXT, oid TEXT, tid TEXT,
                    manuell_json TEXT NOT NULL DEFAULT '[]');
                 PRAGMA user_version = 3;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO geraete (id, json, token_hash) VALUES ('alt-1', ?1, 'hash')",
                params![serde_json::to_string(&Geraet {
                    id: quiver_domain::GeraetId::neu("alt-1"),
                    name: "Altgerät".into(),
                    gruppen: Vec::new(),
                    gesperrt: false,
                })
                .unwrap()],
            )
            .unwrap();
        }
        let conn = oeffnen(&pfad).unwrap();
        let zeile = hole_geraet(&conn, "alt-1").unwrap().expect("Gerät bleibt");
        assert!(
            zeile.token_erstellt.is_some(),
            "Alter beginnt bei der Migration"
        );
        assert!(
            zeile.token_zuletzt.is_some(),
            "Nutzung beginnt bei der Migration"
        );
        // Frisch: nicht abgelaufen.
        let erstellt = hilfen::zeit_lesen(zeile.token_erstellt.as_deref().unwrap()).unwrap();
        let zuletzt = hilfen::zeit_lesen(zeile.token_zuletzt.as_deref().unwrap()).unwrap();
        assert!(hilfen::jetzt() - erstellt < time::Duration::minutes(5));
        assert!(hilfen::jetzt() - zuletzt < time::Duration::minutes(5));
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn root_adresse_wird_gemerkt_und_vergisst_nach_dreissig_tagen() {
        let conn = gedaechtnis();
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        assert!(!root_ip_bekannt(&conn, "ip:203.0.113.20", jetzt).unwrap());
        root_ip_merken(&conn, "ip:203.0.113.20", jetzt).unwrap();
        assert!(root_ip_bekannt(&conn, "ip:203.0.113.20", jetzt).unwrap());
        assert!(!root_ip_bekannt(&conn, "ip:203.0.113.21", jetzt).unwrap());
        // Nach 30 Tagen zählt sie nicht mehr.
        assert!(!root_ip_bekannt(
            &conn,
            "ip:203.0.113.20",
            jetzt + time::Duration::days(ROOT_IP_TAGE + 1)
        )
        .unwrap());
    }

    #[test]
    fn fehlversuche_stehen_je_adresse_und_stunde_als_ein_eintrag() {
        let conn = gedaechtnis();
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        for _ in 0..5 {
            pruefspur_fehlschlag(&conn, jetzt, "ip:203.0.113.9", "Root-Anmeldung").unwrap();
        }
        let liste = pruefspur_liste(&conn, 10).unwrap();
        assert_eq!(liste.len(), 1, "ein Eintrag mit Zähler");
        assert_eq!(liste[0].anzahl, 5);
        assert_eq!(liste[0].art, "fehlschlag");
        // Die nächste Stunde ist ein neuer Eintrag.
        pruefspur_fehlschlag(
            &conn,
            jetzt + time::Duration::hours(1),
            "ip:203.0.113.9",
            "Root-Anmeldung",
        )
        .unwrap();
        assert_eq!(pruefspur_liste(&conn, 10).unwrap().len(), 2);
    }

    #[test]
    fn pruefspur_haelt_180_tage() {
        let conn = gedaechtnis();
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        pruefspur_schreiben(&conn, jetzt, "test", "wer", "alt").unwrap();
        let spaeter = jetzt + time::Duration::days(PRUEFSPUR_TAGE + 1);
        pruefspur_schreiben(&conn, spaeter, "test", "wer", "neu").unwrap();
        // Aufgeräumt wird im Hintergrund, nicht beim Schreiben (Punkt 7).
        assert_eq!(pruefspur_liste(&conn, 10).unwrap().len(), 2);
        assert_eq!(pruefspur_aufraeumen(&conn, spaeter).unwrap(), 1);
        let liste = pruefspur_liste(&conn, 10).unwrap();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].text, "neu");
        assert_eq!(liste[0].anzahl, 1);
    }

    fn jetzt() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()
    }

    #[test]
    fn entra_gruppe_entsteht_einmal_und_behaelt_titel_und_saetze() {
        let conn = gedaechtnis();
        let ids = vec![String::from("12345678-1234-1234-1234-123456789012")];
        let neu = entra_gruppen_merken(&conn, &ids, jetzt()).unwrap();
        assert_eq!(neu.len(), 1);
        assert_eq!(neu[0].id.als_str(), ids[0]);
        assert_eq!(neu[0].titel, "Entra-Gruppe 12345678");
        assert!(neu[0].saetze.is_empty());

        // Der Admin benennt um und ordnet einen Satz zu.
        let mut gruppe = neu[0].clone();
        gruppe.titel = String::from("Team");
        gruppe.saetze = vec![SatzId::neu("basis")];
        setze(&conn, GRUPPEN, gruppe.id.als_str(), &gruppe).unwrap();

        // Eine zweite Meldung derselben Id ändert nichts.
        assert!(entra_gruppen_merken(&conn, &ids, jetzt())
            .unwrap()
            .is_empty());
        let alle: Vec<Gruppe> = liste(&conn, GRUPPEN).unwrap();
        assert_eq!(alle.len(), 1);
        assert_eq!(alle[0].titel, "Team");
        assert_eq!(alle[0].saetze, vec![SatzId::neu("basis")]);
    }

    #[test]
    fn geloeschte_entra_gruppe_bleibt_geloescht_bis_zur_aufnahme() {
        let conn = gedaechtnis();
        let id = String::from("abcdefab-1234-1234-1234-123456789012");
        entra_gruppen_merken(&conn, std::slice::from_ref(&id), jetzt()).unwrap();
        loesche(&conn, GRUPPEN, &id).unwrap();
        entra_gruppe_loeschen_merken(&conn, std::slice::from_ref(&id), jetzt()).unwrap();

        // Erneutes Sehen legt sie nicht wieder an.
        assert!(
            entra_gruppen_merken(&conn, std::slice::from_ref(&id), jetzt())
                .unwrap()
                .is_empty()
        );
        assert!(liste::<Gruppe>(&conn, GRUPPEN).unwrap().is_empty());
        let liste = entra_gruppen_liste(&conn).unwrap();
        assert_eq!(liste.len(), 1);
        assert!(liste[0].geloescht);
        assert!(liste[0].gruppe.is_none());

        // Wieder aufnehmen: eine Gruppe, ohne Sätze, nicht mehr gelöscht.
        let gruppe = entra_gruppe_aufnehmen(&conn, &id, None, jetzt()).unwrap();
        assert!(gruppe.saetze.is_empty());
        assert_eq!(gruppe.entra, vec![id.clone()]);
        let liste = entra_gruppen_liste(&conn).unwrap();
        assert!(!liste[0].geloescht);
        assert_eq!(
            liste[0].gruppe.as_ref().map(|g| g.als_str()),
            Some(id.as_str())
        );
        assert_eq!(
            liste[0].anzeigename.as_deref(),
            Some("Entra-Gruppe abcdefab")
        );
    }

    #[test]
    fn hoechstens_zweihundert_gruppen_aus_entra() {
        let conn = gedaechtnis();
        for n in 0..MAX_ENTRA_GRUPPEN {
            let id = format!("00000000-0000-0000-0000-{n:012x}");
            let gruppe = Gruppe {
                id: GruppeId::neu(id.clone()),
                titel: format!("G{n}"),
                saetze: Vec::new(),
                entra: vec![id],
            };
            setze(&conn, GRUPPEN, gruppe.id.als_str(), &gruppe).unwrap();
        }
        let zuviel = vec![String::from("ffffffff-0000-0000-0000-000000000001")];
        assert!(entra_gruppen_merken(&conn, &zuviel, jetzt())
            .unwrap()
            .is_empty());
        assert_eq!(
            liste::<Gruppe>(&conn, GRUPPEN).unwrap().len(),
            MAX_ENTRA_GRUPPEN
        );
    }

    #[test]
    fn alte_entra_gruppen_tabelle_bekommt_das_loeschmerkmal() {
        let ordner = ordner("entra-migration");
        let pfad = ordner.join("quiver.db");
        {
            let conn = Connection::open(&pfad).unwrap();
            conn.execute_batch(
                "CREATE TABLE entra_gruppen (id TEXT PRIMARY KEY, anzeigename TEXT,
                    zuletzt TEXT NOT NULL);
                 PRAGMA user_version = 1;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO entra_gruppen (id, anzeigename, zuletzt) VALUES ('alt', NULL, 'x')",
                [],
            )
            .unwrap();
        }
        let conn = oeffnen(&pfad).unwrap();
        let liste = entra_gruppen_liste(&conn).unwrap();
        assert_eq!(liste.len(), 1, "alte Zeile bleibt");
        assert_eq!(liste[0].id, "alt");
        assert!(!liste[0].geloescht);
        assert!(
            ordner
                .join(format!("quiver.db.vor-{SCHEMA_VERSION}"))
                .exists(),
            "Sicherung liegt da"
        );
        let _ = std::fs::remove_dir_all(&ordner);
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
            None,
            &[GruppeId::neu("b")],
            &["entra-a".to_string(), "entra-fremd".to_string()],
        )
        .unwrap();
        assert_eq!(wirksam, vec![GruppeId::neu("a"), GruppeId::neu("b")]);
    }

    #[test]
    fn name_aus_entra_setzt_nur_den_automatischen_titel() {
        let conn = gedaechtnis();
        let id = String::from("12345678-1234-1234-1234-123456789012");
        entra_gruppen_merken(&conn, std::slice::from_ref(&id), jetzt()).unwrap();
        assert_eq!(
            liste::<Gruppe>(&conn, GRUPPEN).unwrap()[0].titel,
            "Entra-Gruppe 12345678"
        );

        // Der automatische Titel folgt dem Namen aus Entra.
        assert!(entra_name_uebernehmen(&conn, &id, "Team Alpha", jetzt()).unwrap());
        assert_eq!(
            liste::<Gruppe>(&conn, GRUPPEN).unwrap()[0].titel,
            "Team Alpha"
        );
        let zeile = &entra_gruppen_liste(&conn).unwrap()[0];
        assert_eq!(zeile.name_entra.as_deref(), Some("Team Alpha"));
        assert!(zeile.name_entra_am.is_some());

        // Ein weiterer Name folgt, solange der Titel der übernommene ist.
        assert!(entra_name_uebernehmen(&conn, &id, "Team Beta", jetzt()).unwrap());
        assert_eq!(
            liste::<Gruppe>(&conn, GRUPPEN).unwrap()[0].titel,
            "Team Beta"
        );

        // Ein Titel des Admins bleibt – der neue Name wird nur gemerkt.
        let mut gruppe = liste::<Gruppe>(&conn, GRUPPEN).unwrap()[0].clone();
        gruppe.titel = String::from("Mein Team");
        setze(&conn, GRUPPEN, gruppe.id.als_str(), &gruppe).unwrap();
        assert!(!entra_name_uebernehmen(&conn, &id, "Team Gamma", jetzt()).unwrap());
        assert_eq!(
            liste::<Gruppe>(&conn, GRUPPEN).unwrap()[0].titel,
            "Mein Team"
        );
        assert_eq!(
            entra_gruppen_liste(&conn).unwrap()[0].name_entra.as_deref(),
            Some("Team Gamma")
        );
        // Auch der nächste Name überschreibt ihn nicht.
        assert!(!entra_name_uebernehmen(&conn, &id, "Team Delta", jetzt()).unwrap());
        assert_eq!(
            liste::<Gruppe>(&conn, GRUPPEN).unwrap()[0].titel,
            "Mein Team"
        );
    }

    #[test]
    fn neue_gruppe_bekommt_den_schon_bekannten_namen() {
        let conn = gedaechtnis();
        let id = String::from("abcdefab-1234-1234-1234-123456789012");
        // Die Id wurde gesehen (Zeile ohne Gruppe), der Name kam zuerst an.
        entra_zeile_merken(&conn, &id, jetzt()).unwrap();
        entra_name_uebernehmen(&conn, &id, "Team Zuerst", jetzt()).unwrap();
        let neu = entra_gruppen_merken(&conn, std::slice::from_ref(&id), jetzt()).unwrap();
        assert_eq!(neu.len(), 1);
        assert_eq!(neu[0].titel, "Team Zuerst");
    }

    #[test]
    fn namen_fehlen_erst_nach_vierundzwanzig_stunden() {
        let conn = gedaechtnis();
        let id = String::from("12345678-1234-1234-1234-123456789012");
        entra_gruppen_merken(&conn, std::slice::from_ref(&id), jetzt()).unwrap();
        assert_eq!(
            entra_namen_fehlen(&conn, std::slice::from_ref(&id), jetzt()).unwrap(),
            vec![id.clone()]
        );

        entra_name_uebernehmen(&conn, &id, "Team Alpha", jetzt()).unwrap();
        assert!(
            entra_namen_fehlen(&conn, std::slice::from_ref(&id), jetzt())
                .unwrap()
                .is_empty(),
            "frischer Name wird nicht neu geholt"
        );
        assert_eq!(
            entra_namen_fehlen(
                &conn,
                std::slice::from_ref(&id),
                jetzt() + time::Duration::hours(25)
            )
            .unwrap(),
            vec![id.clone()],
            "nach 24 Stunden wieder fällig"
        );
        // Eine fremde Id wird nicht abgefragt.
        assert!(entra_namen_fehlen(
            &conn,
            &[String::from("ffffffff-0000-0000-0000-000000000001")],
            jetzt()
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn alte_entra_gruppen_tabelle_bekommt_die_namensspalten() {
        let ordner = ordner("entra-migration-30");
        let pfad = ordner.join("quiver.db");
        {
            let conn = Connection::open(&pfad).unwrap();
            conn.execute_batch(
                "CREATE TABLE entra_gruppen (id TEXT PRIMARY KEY, anzeigename TEXT,
                    zuletzt TEXT NOT NULL, geloescht_am TEXT);
                 PRAGMA user_version = 2;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO entra_gruppen (id, anzeigename, zuletzt) VALUES ('alt', NULL, 'x')",
                [],
            )
            .unwrap();
        }
        let conn = oeffnen(&pfad).unwrap();
        let liste = entra_gruppen_liste(&conn).unwrap();
        assert_eq!(liste.len(), 1, "alte Zeile bleibt");
        assert!(liste[0].name_entra.is_none());
        assert!(liste[0].name_entra_am.is_none());
        assert!(
            ordner
                .join(format!("quiver.db.vor-{SCHEMA_VERSION}"))
                .exists(),
            "Sicherung liegt da"
        );
        let _ = std::fs::remove_dir_all(&ordner);
    }
}
