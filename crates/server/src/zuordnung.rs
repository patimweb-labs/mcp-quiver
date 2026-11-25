// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Benutzer und Gruppen von Hand zuordnen (Aufgabe 33).
//!
//! Bisher kamen die Gruppen eines Benutzers nur aus Entra. Jetzt gibt es
//! Zugehörigkeiten **von Hand**, und zwar von beiden Seiten bedienbar: beim
//! Benutzer („Gruppen“) und bei der Gruppe („Mitglieder“). Beide Wege ändern
//! dieselbe Zeile in `benutzer_gruppen` – deshalb gibt es hier auch nur eine
//! Schreibfunktion ([`setzen`]) und zwei Lesesichten darauf.
//!
//! Regeln:
//! - Eine Zugehörigkeit von Hand gehört dem **Benutzer** und überlebt
//!   Entra-Auffrischung, Neuanmeldung und neue Geräte.
//! - Wirksame Gruppen sind Entra ∪ von Hand ∪ Gerät (alt) – gerechnet an
//!   **einer** Stelle ([`crate::speicher::wirksame_gruppen`]).
//! - Der Root bekommt keine Gruppen (er ist der Notzugang ohne Geräte).
//! - Die Kennung zählt, der Name ist Anzeige: zwei Entra-Gruppen mit gleichem
//!   Namen bleiben zwei Gruppen.
//! - Wer sich selbst die letzte Gruppe nimmt, muss bestätigen.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Extension;
use axum::Json;
use quiver_domain::{Gruppe, GruppeId};
use quiver_protocol::{BenutzerGruppe, Gruppenmitglied, ZuordnungAendern};

use crate::db;
use crate::fehler::{Ergebnis, FehlerAntwort, Koerper};
use crate::sitzung::SitzungKontext;
use crate::speicher::{self, Benutzer};
use crate::zustand::Zustand;

// ------------------------------------------------------------------- Sichten

/// Die Gruppen eines Benutzers mit Herkunft, für die Verwaltung.
///
/// Gezeigt wird, was **wirkt**: aus Entra, von Hand oder am Gerät gesetzt.
/// Gruppen, die es nicht mehr gibt, erscheinen nicht (gelöschte Gruppe →
/// Zuordnung verschwindet).
pub(crate) fn benutzer_gruppen_sicht(
    conn: &rusqlite::Connection,
    benutzer: &Benutzer,
) -> anyhow::Result<Vec<BenutzerGruppe>> {
    let gruppen: Vec<Gruppe> = speicher::liste(conn, speicher::GRUPPEN)?;
    let hand: BTreeMap<String, speicher::Handzuordnung> =
        speicher::handzuordnungen_von(conn, &benutzer.oid)?
            .into_iter()
            .map(|zuordnung| (zuordnung.gruppe.als_str().to_string(), zuordnung))
            .collect();
    // Am Gerät gesetzte Gruppen (Altweg, Aufgabe 20): sie gehören zum Gerät,
    // wirken aber für den Benutzer – also gehören sie in seine Sicht.
    let mut am_geraet = Vec::new();
    for zeile in speicher::geraete_von_user(conn, &benutzer.oid)? {
        for id in zeile.manuell {
            if !am_geraet.contains(&id) {
                am_geraet.push(id);
            }
        }
    }

    let mut aus = Vec::new();
    for gruppe in gruppen {
        let entra_ids: Vec<String> = gruppe
            .entra
            .iter()
            .filter(|id| benutzer.entra.iter().any(|e| e.eq_ignore_ascii_case(id)))
            .cloned()
            .collect();
        let zuordnung = hand.get(gruppe.id.als_str());
        let geraet = am_geraet.contains(&gruppe.id);
        if entra_ids.is_empty() && zuordnung.is_none() && !geraet {
            continue;
        }
        aus.push(BenutzerGruppe {
            titel: titel_von(&gruppe),
            gruppe: gruppe.id.als_str().to_string(),
            aus_entra: !entra_ids.is_empty(),
            von_hand: zuordnung.is_some(),
            aus_geraet: geraet,
            von: zuordnung.map(|zuordnung| zuordnung.von.clone()),
            seit: zuordnung.map(|zuordnung| zuordnung.seit.clone()),
            entra_ids,
        });
    }
    aus.sort_by(|a, b| {
        a.titel
            .to_lowercase()
            .cmp(&b.titel.to_lowercase())
            .then_with(|| a.gruppe.cmp(&b.gruppe))
    });
    Ok(aus)
}

/// Die Mitglieder einer Gruppe mit Herkunft (aus Entra oder von Hand).
///
/// Am **Gerät** gesetzte Gruppen erscheinen hier nicht: sie gehören einem
/// Gerät, nicht dem Benutzer, und stehen in der Geräteliste.
pub(crate) fn mitglieder_sicht(
    conn: &rusqlite::Connection,
    gruppe: &Gruppe,
) -> anyhow::Result<Vec<Gruppenmitglied>> {
    let hand: BTreeMap<String, speicher::Handzuordnung> =
        speicher::handzuordnungen_zu_gruppe(conn, gruppe.id.als_str())?
            .into_iter()
            .map(|zuordnung| (zuordnung.oid.clone(), zuordnung))
            .collect();
    let mut aus = Vec::new();
    for benutzer in speicher::benutzer_liste(conn)? {
        if benutzer.oid == speicher::ROOT_OID {
            continue;
        }
        let aus_entra = gruppe
            .entra
            .iter()
            .any(|id| benutzer.entra.iter().any(|e| e.eq_ignore_ascii_case(id)));
        let zuordnung = hand.get(&benutzer.oid);
        if !aus_entra && zuordnung.is_none() {
            continue;
        }
        aus.push(Gruppenmitglied {
            oid: benutzer.oid,
            email: benutzer.email,
            name: benutzer.name,
            gesperrt: benutzer.gesperrt,
            aus_entra,
            von_hand: zuordnung.is_some(),
            von: zuordnung.map(|zuordnung| zuordnung.von.clone()),
            seit: zuordnung.map(|zuordnung| zuordnung.seit.clone()),
        });
    }
    aus.sort_by(|a, b| {
        anzeige(a)
            .to_lowercase()
            .cmp(&anzeige(b).to_lowercase())
            .then_with(|| a.oid.cmp(&b.oid))
    });
    Ok(aus)
}

fn anzeige(mitglied: &Gruppenmitglied) -> String {
    mitglied
        .name
        .clone()
        .or_else(|| mitglied.email.clone())
        .unwrap_or_else(|| mitglied.oid.clone())
}

/// Der Titel einer Gruppe; ohne Titel bleibt die Kennung.
fn titel_von(gruppe: &Gruppe) -> String {
    let titel = gruppe.titel.trim();
    if titel.is_empty() {
        gruppe.id.als_str().to_string()
    } else {
        titel.to_string()
    }
}

/// Wessen Name in Prüfspur und Zuordnung steht: E-Mail, sonst Name, sonst
/// Objekt-Id (wie in [`crate::spur`]).
fn akteur_name(kontext: &SitzungKontext) -> String {
    kontext
        .benutzer
        .email
        .clone()
        .or_else(|| kontext.benutzer.name.clone())
        .unwrap_or_else(|| kontext.benutzer.oid.clone())
}

// ------------------------------------------------------------------ Lesen

/// `GET /admin/benutzer/{oid}/gruppen`
pub(crate) async fn benutzer_gruppen(
    State(zustand): State<Arc<Zustand>>,
    Path(oid): Path<String>,
) -> Ergebnis<Json<Vec<BenutzerGruppe>>> {
    let kennung = oid.clone();
    let liste = db(&zustand, move |conn| {
        let Some(benutzer) = speicher::benutzer_holen(conn, &kennung)? else {
            return Ok(Err(FehlerAntwort::nicht_gefunden(format!(
                "Benutzer {kennung} gibt es nicht"
            ))));
        };
        Ok(Ok(benutzer_gruppen_sicht(conn, &benutzer)?))
    })
    .await??;
    Ok(Json(liste))
}

/// `GET /admin/gruppen/{id}/mitglieder`
pub(crate) async fn gruppe_mitglieder(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<Json<Vec<Gruppenmitglied>>> {
    let kennung = id.clone();
    let liste = db(&zustand, move |conn| {
        let Some(gruppe) = speicher::hole(conn, speicher::GRUPPEN, &kennung)? else {
            return Ok(Err(FehlerAntwort::nicht_gefunden(format!(
                "Gruppe {kennung} gibt es nicht"
            ))));
        };
        Ok(Ok(mitglieder_sicht(conn, &gruppe)?))
    })
    .await??;
    Ok(Json(liste))
}

// ---------------------------------------------------------------- Schreiben

/// `PUT /admin/benutzer/{oid}/gruppen/{gruppe}` – von der Benutzerseite.
pub(crate) async fn benutzer_gruppe_setzen(
    State(zustand): State<Arc<Zustand>>,
    Extension(akteur): Extension<SitzungKontext>,
    Path((oid, gruppe)): Path<(String, String)>,
    Koerper(aenderung): Koerper<ZuordnungAendern>,
) -> Ergebnis<Json<Vec<BenutzerGruppe>>> {
    setzen(&zustand, &akteur, &oid, &gruppe, &aenderung).await?;
    benutzer_gruppen(State(zustand), Path(oid)).await
}

/// `PUT /admin/gruppen/{id}/mitglieder/{oid}` – von der Gruppenseite.
pub(crate) async fn mitglied_setzen(
    State(zustand): State<Arc<Zustand>>,
    Extension(akteur): Extension<SitzungKontext>,
    Path((gruppe, oid)): Path<(String, String)>,
    Koerper(aenderung): Koerper<ZuordnungAendern>,
) -> Ergebnis<Json<Vec<Gruppenmitglied>>> {
    setzen(&zustand, &akteur, &oid, &gruppe, &aenderung).await?;
    gruppe_mitglieder(State(zustand), Path(gruppe)).await
}

/// Die eine Schreibfunktion: dieselbe Zeile, ob von der Benutzer- oder von der
/// Gruppenseite aufgerufen.
async fn setzen(
    zustand: &Arc<Zustand>,
    akteur: &SitzungKontext,
    oid: &str,
    gruppe: &str,
    aenderung: &ZuordnungAendern,
) -> Ergebnis<()> {
    let jetzt = zustand.uhr.jetzt();
    let wer_oid = akteur.benutzer.oid.clone();
    let wer = akteur_name(akteur);
    let kennung = oid.to_string();
    let gruppe_kennung = gruppe.to_string();
    let gruppe_id = GruppeId::neu(gruppe.to_string());
    let vorhanden = aenderung.vorhanden;
    let selbst_bestaetigt = aenderung.selbst_bestaetigt;

    let geaendert = db(zustand, move |conn| {
        // Die Gruppe muss es geben; eine Zuordnung auf eine gelöschte Gruppe
        // wäre eine Zeile, die nichts bewirkt.
        let Some(gruppe_objekt): Option<Gruppe> =
            speicher::hole(conn, speicher::GRUPPEN, &gruppe_kennung)?
        else {
            return Ok(Err(FehlerAntwort::nicht_gefunden(format!(
                "Gruppe {gruppe_kennung} gibt es nicht"
            ))));
        };
        let Some(benutzer) = speicher::benutzer_holen(conn, &kennung)? else {
            return Ok(Err(FehlerAntwort::nicht_gefunden(format!(
                "Benutzer {kennung} gibt es nicht"
            ))));
        };
        if benutzer.oid == speicher::ROOT_OID {
            return Ok(Err(FehlerAntwort::konflikt(
                "der Root bekommt keine Gruppen; er ist der Notzugang ohne Geräte",
            )));
        }

        let geaendert = if vorhanden {
            speicher::handzuordnung_setzen(conn, &kennung, &benutzer.tid, &gruppe_id, &wer, jetzt)?
        } else {
            // Wer sich selbst aussperrt, muss bestätigen: ohne wirksame Gruppe
            // gilt „kein Zugriff“ (ADR 0002, Punkt 8).
            if kennung == wer_oid && !selbst_bestaetigt {
                let wirksam = speicher::wirksame_gruppen_benutzer(conn, &kennung)?;
                let bleibt = gruppe_objekt
                    .entra
                    .iter()
                    .any(|id| benutzer.entra.iter().any(|e| e.eq_ignore_ascii_case(id)))
                    || speicher::geraete_von_user(conn, &kennung)?
                        .iter()
                        .any(|zeile| {
                            zeile
                                .manuell
                                .iter()
                                .any(|id| id.als_str() == gruppe_kennung)
                        });
                let uebrig = wirksam
                    .iter()
                    .filter(|id| id.als_str() != gruppe_kennung || bleibt)
                    .count();
                if uebrig == 0 {
                    return Ok(Err(FehlerAntwort::konflikt(
                        "das ist deine letzte Gruppe; danach gilt „kein Zugriff“. \
                         Zum Entfernen selbst_bestaetigt setzen",
                    )));
                }
            }
            speicher::handzuordnung_entfernen(conn, &kennung, &gruppe_kennung)?
        };

        if geaendert {
            speicher::pruefspur_schreiben(
                conn,
                jetzt,
                "zuordnung",
                &wer,
                &format!(
                    "{kennung}: Gruppe {gruppe_kennung} {} (von Hand)",
                    if vorhanden {
                        "hinzugefügt"
                    } else {
                        "entfernt"
                    }
                ),
            )?;
            // Sofort wirksam: die Geräte des Benutzers rechnen neu, und die
            // Ströme melden die Änderung (unten).
            speicher::geraete_gruppen_neu(conn, &kennung, &benutzer.entra)?;
        }
        Ok(Ok(geaendert))
    })
    .await??;

    if geaendert {
        zustand.melde_aenderung();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titel_ohne_wert_ist_die_kennung() {
        let gruppe = Gruppe {
            id: GruppeId::neu("team"),
            titel: "   ".into(),
            saetze: Vec::new(),
            entra: Vec::new(),
        };
        assert_eq!(titel_von(&gruppe), "team");
        let gruppe = Gruppe {
            titel: "Team".into(),
            ..gruppe
        };
        assert_eq!(titel_von(&gruppe), "Team");
    }
}
