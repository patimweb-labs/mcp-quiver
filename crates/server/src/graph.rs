// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Anzeigenamen der Entra-Gruppen (Aufgabe 30).
//!
//! Rechte laufen weiter nur über die Objekt-Id; der Name ist reine Anzeige.
//! Ohne `QUIVER_ENTRA_GRUPPENNAMEN=an` gibt es weder den Lesebereich noch eine
//! Anfrage. Mit Schalter holt der Server für die Gruppen aus dem ID-Token, deren
//! Name fehlt oder älter als 24 Stunden ist, `displayName` über Graph – mit dem
//! Zugriffstoken **dieses** Benutzers.
//!
//! Alles läuft im Hintergrund: Anmeldung und Nachprüfen warten nie darauf, ein
//! Fehler ändert nichts. Das Zugriffstoken lebt nur in dieser Aufgabe, wird nie
//! gespeichert, nie protokolliert und steht in keinem `Debug`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use futures_util::stream::{self, StreamExt};

use crate::config::{ist_guid, GRAPH_BASIS};
use crate::db;
use crate::speicher;
use crate::zustand::Zustand;

/// Höchstens so viele Namen je Lauf.
pub const MAX_JE_LAUF: usize = 20;
/// Höchstens so viele Anfragen gleichzeitig.
pub const GLEICHZEITIG: usize = 4;
/// Zeitlimit je Anfrage.
pub const ZEITLIMIT: Duration = Duration::from_secs(5);
/// Obergrenze für eine Antwort.
pub const MAX_ANTWORT: usize = 64 * 1024;
/// Längster übernommener Name.
pub const NAME_MAX: usize = 120;
/// So lange gilt ein geholter Name, bevor er aufgefrischt wird.
pub const NAME_STUNDEN: i64 = 24;
/// Nach einem 403 fragt der Server einen Tag nicht mehr.
pub const PAUSE_STUNDEN: i64 = 24;
/// Fehler stehen höchstens einmal je Stunde im Log.
pub const LOG_STUNDEN: i64 = 1;

/// Warum ein Name nicht kam. Enthält nie das Token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphFehler {
    /// 403: Rechte fehlen (Zustimmung des Mandanten).
    Verboten,
    /// 404: Gruppe gibt es nicht (mehr) oder nicht sichtbar.
    NichtGefunden,
    /// Netz, Zeitlimit, Weiterleitung, zu groß, fremde Id, unbrauchbarer Name.
    Antwort(String),
}

impl GraphFehler {
    /// Zählt dieser Fehler als Netz-/Serverfehler für das Log?
    pub fn ist_fehler(&self) -> bool {
        !matches!(self, GraphFehler::NichtGefunden)
    }
}

/// Der Graph-Client; `basis` ist im Betrieb fest, in Tests Loopback.
pub struct Graph {
    basis: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph").field("basis", &self.basis).finish()
    }
}

impl Graph {
    pub fn neu(basis: &str) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            // Keine Weiterleitungen: das Ziel steht fest, es wird nicht
            // umgebogen (Aufgabe 30).
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ZEITLIMIT)
            .build()
            .context("HTTP-Client für Graph")?;
        Ok(Self {
            basis: basis.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// Die Adresse für genau diese Id; nur eine GUID wird eingesetzt.
    pub fn url(&self, id: &str) -> anyhow::Result<String> {
        if !ist_guid(id) {
            anyhow::bail!("keine Objekt-Id: {id}");
        }
        Ok(format!(
            "{}/v1.0/groups/{}?$select=id,displayName",
            if self.basis.is_empty() {
                GRAPH_BASIS
            } else {
                self.basis.as_str()
            },
            id.to_ascii_lowercase()
        ))
    }

    /// `GET {graph}/v1.0/groups/{id}?$select=id,displayName`.
    ///
    /// Die Antwort muss dieselbe Id tragen; der Name wird gesäubert.
    pub async fn name_holen(&self, token: &str, id: &str) -> Result<String, GraphFehler> {
        let url = self
            .url(id)
            .map_err(|fehler| GraphFehler::Antwort(format!("{fehler:#}")))?;
        let antwort = self
            .client
            .get(&url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|fehler| GraphFehler::Antwort(format!("nicht erreichbar: {fehler}")))?;
        match antwort.status().as_u16() {
            200..=299 => {}
            403 => return Err(GraphFehler::Verboten),
            404 => return Err(GraphFehler::NichtGefunden),
            status => {
                return Err(GraphFehler::Antwort(format!("Status {status}")));
            }
        }
        if let Some(laenge) = antwort.content_length() {
            if laenge > MAX_ANTWORT as u64 {
                return Err(GraphFehler::Antwort(String::from("Antwort zu groß")));
            }
        }
        let bytes = antwort
            .bytes()
            .await
            .map_err(|fehler| GraphFehler::Antwort(format!("nicht lesbar: {fehler}")))?;
        if bytes.len() > MAX_ANTWORT {
            return Err(GraphFehler::Antwort(String::from("Antwort zu groß")));
        }
        name_aus_antwort(&bytes, id)
    }
}

/// Die Id muss passen und der Name brauchbar sein.
fn name_aus_antwort(bytes: &[u8], erwartet: &str) -> Result<String, GraphFehler> {
    let wert: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|fehler| GraphFehler::Antwort(format!("kein JSON: {fehler}")))?;
    let id = wert
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| GraphFehler::Antwort(String::from("Antwort ohne id")))?;
    if !id.eq_ignore_ascii_case(erwartet) {
        return Err(GraphFehler::Antwort(format!(
            "Antwort gehört zu einer anderen Id ({id})"
        )));
    }
    let roh = wert
        .get("displayName")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| GraphFehler::Antwort(String::from("Antwort ohne displayName")))?;
    name_saeubern(roh).ok_or_else(|| GraphFehler::Antwort(String::from("Name unbrauchbar")))
}

/// Steuer- und Richtungszeichen heraus, trimmen, auf 120 Zeichen begrenzen.
///
/// `None`, wenn nichts übrig bleibt oder der Name zu lang ist – ein
/// abgeschnittener Name wäre eine Behauptung über die Gruppe.
pub fn name_saeubern(roh: &str) -> Option<String> {
    let gesaeubert: String = roh
        .chars()
        .filter(|zeichen| !zeichen.is_control() && !ist_richtungszeichen(*zeichen))
        .collect();
    let name = gesaeubert.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return None;
    }
    Some(name.to_string())
}

/// Bidi-Steuerzeichen (Trojan Source, U+202E und Verwandte).
fn ist_richtungszeichen(zeichen: char) -> bool {
    matches!(
        zeichen as u32,
        0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069
    )
}

/// Namen im Hintergrund holen: nie warten, nie scheitern lassen.
///
/// `token` ist das Zugriffstoken des Benutzers aus dieser Anmeldung; es wird
/// hier verbraucht und nirgends abgelegt.
pub fn starte(zustand: &Arc<Zustand>, gruppen: &[String], token: Option<String>) {
    let (Some(graph), Some(token)) = (zustand.graph.clone(), token) else {
        return;
    };
    if gruppen.is_empty() {
        return;
    }
    let zustand = zustand.clone();
    let ids = gruppen.to_vec();
    tokio::spawn(async move {
        if let Err(fehler) = lauf(&zustand, graph, ids, token).await {
            tracing::debug!("Gruppennamen: nicht möglich ({fehler:#})");
        }
    });
}

async fn lauf(
    zustand: &Arc<Zustand>,
    graph: Arc<Graph>,
    ids: Vec<String>,
    token: String,
) -> anyhow::Result<()> {
    let jetzt = zustand.uhr.jetzt();
    if zustand.graph_pausiert(jetzt) {
        return Ok(());
    }
    let offen = db(zustand, move |conn| {
        speicher::entra_namen_fehlen(conn, &ids, jetzt)
    })
    .await
    .map_err(|fehler| anyhow::anyhow!("{}", fehler.text))?;
    let offen: Vec<String> = offen.into_iter().take(MAX_JE_LAUF).collect();
    if offen.is_empty() {
        return Ok(());
    }

    let anzahl = offen.len();
    let ergebnisse = stream::iter(offen)
        .map(|id| {
            let graph = graph.clone();
            let token = token.clone();
            async move {
                let name = graph.name_holen(&token, &id).await;
                (id, name)
            }
        })
        .buffer_unordered(GLEICHZEITIG)
        .collect::<Vec<_>>()
        .await;

    let mut namen = Vec::new();
    let mut verboten = false;
    let mut fehler = 0usize;
    for (id, ergebnis) in ergebnisse {
        match ergebnis {
            Ok(name) => namen.push((id, name)),
            Err(GraphFehler::Verboten) => {
                verboten = true;
                fehler += 1;
            }
            Err(fehler_wert) => {
                if fehler_wert.ist_fehler() {
                    fehler += 1;
                }
            }
        }
    }

    if !namen.is_empty() {
        let geaendert = db(zustand, move |conn| {
            let mut geaendert = false;
            for (id, name) in namen {
                if speicher::entra_name_uebernehmen(conn, &id, &name, jetzt)? {
                    speicher::pruefspur_schreiben(
                        conn,
                        jetzt,
                        "gruppe",
                        "Entra",
                        &format!("{id}: Titel aus Entra übernommen"),
                    )?;
                    geaendert = true;
                }
            }
            Ok(geaendert)
        })
        .await
        .map_err(|fehler| anyhow::anyhow!("{}", fehler.text))?;
        if geaendert {
            zustand.melde_aenderung();
        }
    }

    if verboten {
        zustand.graph_pausieren(jetzt + time::Duration::hours(PAUSE_STUNDEN));
    }
    if fehler > 0 && zustand.graph_log_faellig(jetzt) {
        // Ohne Token und ohne Id-Liste: nur die Zahl.
        tracing::warn!("Gruppennamen: {fehler} von {anzahl} Abfragen ohne Ergebnis");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namen_werden_gesaeubert() {
        assert_eq!(
            name_saeubern("  Team Alpha  ").as_deref(),
            Some("Team Alpha")
        );
        // Steuerzeichen und Richtungszeichen fliegen raus.
        assert_eq!(
            name_saeubern("Team\u{0000}\u{202E} Alpha\u{200F}").as_deref(),
            Some("Team Alpha")
        );
        assert_eq!(name_saeubern("\u{202E}").as_deref(), None);
        assert_eq!(name_saeubern("   ").as_deref(), None);
        assert_eq!(
            name_saeubern(&"x".repeat(NAME_MAX)).map(|n| n.len()),
            Some(NAME_MAX)
        );
        assert_eq!(name_saeubern(&"x".repeat(NAME_MAX + 1)), None);
    }

    #[test]
    fn nur_eine_objekt_id_wird_eingesetzt() {
        let graph = Graph::neu("http://127.0.0.1:5123/").unwrap();
        let url = graph
            .url("12345678-1234-1234-1234-123456789012")
            .expect("GUID");
        assert_eq!(
            url,
            "http://127.0.0.1:5123/v1.0/groups/12345678-1234-1234-1234-123456789012\
             ?$select=id,displayName"
        );
        assert!(graph.url("../../token").is_err());
        assert!(graph.url("12345678").is_err());
    }

    #[test]
    fn antwort_muss_zur_id_passen() {
        let id = "12345678-1234-1234-1234-123456789012";
        let gut = format!(r#"{{"id":"{id}","displayName":"Team"}}"#);
        assert_eq!(
            name_aus_antwort(gut.as_bytes(), id).unwrap(),
            "Team".to_string()
        );
        // Groß-/Kleinschreibung der Id ist egal (Entra liefert klein).
        let gross = format!(r#"{{"id":"{}","displayName":"Team"}}"#, id.to_uppercase());
        assert!(name_aus_antwort(gross.as_bytes(), id).is_ok());

        let fremd = r#"{"id":"abcdefab-1234-1234-1234-123456789012","displayName":"Team"}"#;
        assert!(matches!(
            name_aus_antwort(fremd.as_bytes(), id),
            Err(GraphFehler::Antwort(_))
        ));
        assert!(name_aus_antwort(b"kein json", id).is_err());
        assert!(name_aus_antwort(br#"{"id":"12345678-1234-1234-1234-123456789012"}"#, id).is_err());
        assert!(name_aus_antwort(
            format!(r#"{{"id":"{id}","displayName":""}}"#).as_bytes(),
            id
        )
        .is_err());
    }
}
