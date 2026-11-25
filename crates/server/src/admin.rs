// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Admin-Seite: Bibliothek pflegen, Sätze und Gruppen zuweisen, Codes, Überblick.
//!
//! Jede Schreiboperation meldet über den Änderungs-Kanal, dass die Ströme der
//! Geräte neu rechnen sollen.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use quiver_domain::{Eintrag, Geraet, Gruppe, Satz};
use quiver_protocol::{Code, CodeAnlegen, GeraetAendern, GeraetStand, Ueberblick};

use crate::fehler::{Ergebnis, FehlerAntwort, Koerper};
use crate::speicher;
use crate::zustand::Zustand;
use crate::{db, hilfen, soll_aus};

/// Ergebnis einer Löschprüfung.
enum Loeschstand {
    WirdBenutzt,
    Fehlt,
    Geloescht,
}

// -------------------------------------------------- Gleichzeitiges Bearbeiten

/// Der `If-Match`-Kopf einer Anfrage, falls gesetzt.
fn if_match(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::IF_MATCH)
        .and_then(|wert| wert.to_str().ok())
        .map(str::to_string)
}

/// Paßt `If-Match` zum aktuellen Stand? `*` und ein fehlender Kopf passen.
fn etag_passt(if_match: &str, aktuell: &str) -> bool {
    let if_match = if_match.trim();
    if if_match == "*" {
        return true;
    }
    if_match.trim_matches('"') == aktuell.trim_matches('"')
}

/// Eine Antwort mit `ETag` (der Stand, den der Aufrufer gesehen hat).
fn mit_etag<T: serde::Serialize>(wert: T) -> Ergebnis<Response> {
    let etag = hilfen::etag(&wert);
    let mut antwort = Json(wert).into_response();
    antwort.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag)
            .map_err(|_| FehlerAntwort::intern("ETag nicht darstellbar"))?,
    );
    Ok(antwort)
}

/// Prüft `If-Match` gegen den gespeicherten Stand; `Err` ist der 409.
fn stand_pruefen<T: serde::Serialize>(
    headers: &HeaderMap,
    gespeichert: Option<&T>,
    was: &str,
) -> Ergebnis<()> {
    let Some(if_match) = if_match(headers) else {
        return Ok(());
    };
    let Some(gespeichert) = gespeichert else {
        return Err(FehlerAntwort::inzwischen_geaendert(was));
    };
    if etag_passt(&if_match, &hilfen::etag(gespeichert)) {
        return Ok(());
    }
    Err(FehlerAntwort::inzwischen_geaendert(was))
}

// ---------------------------------------------------------------- Einträge

pub(crate) async fn eintraege_liste(
    State(zustand): State<Arc<Zustand>>,
) -> Ergebnis<Json<Vec<Eintrag>>> {
    Ok(Json(
        db(&zustand, |conn| speicher::liste(conn, speicher::EINTRAEGE)).await?,
    ))
}

pub(crate) async fn eintrag_holen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<Response> {
    let kennung = id.clone();
    let eintrag: Eintrag = db(&zustand, move |conn| {
        speicher::hole(conn, speicher::EINTRAEGE, &kennung)
    })
    .await?
    .ok_or_else(|| FehlerAntwort::nicht_gefunden(format!("Eintrag {id} gibt es nicht")))?;
    mit_etag(eintrag)
}

pub(crate) async fn eintrag_setzen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Koerper(eintrag): Koerper<Eintrag>,
) -> Ergebnis<Response> {
    if eintrag.id.als_str() != id {
        return Err(FehlerAntwort::ungueltig(format!(
            "Kennung im Pfad ({id}) und im Rumpf ({}) stimmen nicht überein",
            eintrag.id
        )));
    }
    crate::grenzen::eintrag_pruefen(&eintrag).map_err(FehlerAntwort::unverarbeitbar)?;
    let zu_speichern = eintrag.clone();
    let kennung = id.clone();
    let konflikt = db(&zustand, move |conn| {
        let alt: Option<Eintrag> = speicher::hole(conn, speicher::EINTRAEGE, &kennung)?;
        if stand_pruefen(&headers, alt.as_ref(), &format!("Eintrag {kennung}")).is_err() {
            return Ok(true);
        }
        speicher::setze(conn, speicher::EINTRAEGE, &kennung, &zu_speichern)?;
        Ok(false)
    })
    .await?;
    if konflikt {
        return Err(FehlerAntwort::inzwischen_geaendert(&format!(
            "Eintrag {id}"
        )));
    }
    zustand.melde_aenderung();
    mit_etag(eintrag)
}

pub(crate) async fn eintrag_loeschen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<StatusCode> {
    let kennung = id.clone();
    let stand = db(&zustand, move |conn| {
        if speicher::eintrag_benutzt(conn, &kennung)? {
            return Ok(Loeschstand::WirdBenutzt);
        }
        let weg = speicher::loesche(conn, speicher::EINTRAEGE, &kennung)?;
        Ok(if weg {
            Loeschstand::Geloescht
        } else {
            Loeschstand::Fehlt
        })
    })
    .await?;
    match stand {
        Loeschstand::WirdBenutzt => Err(FehlerAntwort::konflikt(format!(
            "Eintrag {id} wird noch von einem Satz benutzt"
        ))),
        Loeschstand::Fehlt => Err(FehlerAntwort::nicht_gefunden(format!(
            "Eintrag {id} gibt es nicht"
        ))),
        Loeschstand::Geloescht => {
            zustand.melde_aenderung();
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

pub(crate) async fn eintrag_importieren(
    State(zustand): State<Arc<Zustand>>,
    Koerper(server_json): Koerper<serde_json::Value>,
) -> Ergebnis<Json<Eintrag>> {
    let eintrag = quiver_bibliothek::importieren(&server_json)
        .map_err(|fehler| FehlerAntwort::ungueltig(format!("server.json: {fehler}")))?;
    crate::grenzen::eintrag_pruefen(&eintrag).map_err(FehlerAntwort::unverarbeitbar)?;
    let zu_speichern = eintrag.clone();
    let vorhanden = db(&zustand, move |conn| {
        let kennung = zu_speichern.id.als_str();
        if speicher::hole::<Eintrag>(conn, speicher::EINTRAEGE, kennung)?.is_some() {
            return Ok(true);
        }
        speicher::setze(conn, speicher::EINTRAEGE, kennung, &zu_speichern)?;
        Ok(false)
    })
    .await?;
    if vorhanden {
        return Err(FehlerAntwort::konflikt(format!(
            "Eintrag {} gibt es schon",
            eintrag.id
        )));
    }
    zustand.melde_aenderung();
    Ok(Json(eintrag))
}

// ------------------------------------------------------------------- Sätze

pub(crate) async fn saetze_liste(State(zustand): State<Arc<Zustand>>) -> Ergebnis<Json<Vec<Satz>>> {
    Ok(Json(
        db(&zustand, |conn| speicher::liste(conn, speicher::SAETZE)).await?,
    ))
}

pub(crate) async fn satz_holen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<Response> {
    let kennung = id.clone();
    let satz: Satz = db(&zustand, move |conn| {
        speicher::hole(conn, speicher::SAETZE, &kennung)
    })
    .await?
    .ok_or_else(|| FehlerAntwort::nicht_gefunden(format!("Satz {id} gibt es nicht")))?;
    mit_etag(satz)
}

pub(crate) async fn satz_setzen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Koerper(satz): Koerper<Satz>,
) -> Ergebnis<Response> {
    if satz.id.als_str() != id {
        return Err(FehlerAntwort::ungueltig(format!(
            "Kennung im Pfad ({id}) und im Rumpf ({}) stimmen nicht überein",
            satz.id
        )));
    }
    crate::grenzen::satz_pruefen(&satz).map_err(FehlerAntwort::unverarbeitbar)?;
    let zu_speichern = satz.clone();
    let kennung = id.clone();
    let (unbekannt, konflikt) = db(&zustand, move |conn| {
        let alt: Option<Satz> = speicher::hole(conn, speicher::SAETZE, &kennung)?;
        if stand_pruefen(&headers, alt.as_ref(), &format!("Satz {kennung}")).is_err() {
            return Ok((Vec::new(), true));
        }
        let unbekannt = speicher::unbekannte_eintraege(conn, &zu_speichern)?;
        if unbekannt.is_empty() {
            speicher::setze(conn, speicher::SAETZE, &kennung, &zu_speichern)?;
        }
        Ok((unbekannt, false))
    })
    .await?;
    if konflikt {
        return Err(FehlerAntwort::inzwischen_geaendert(&format!("Satz {id}")));
    }
    if !unbekannt.is_empty() {
        let liste = unbekannt
            .iter()
            .map(|e| e.als_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(FehlerAntwort::unverarbeitbar(format!(
            "Satz {id} verweist auf unbekannte Einträge: {liste}"
        )));
    }
    zustand.melde_aenderung();
    mit_etag(satz)
}

pub(crate) async fn satz_loeschen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<StatusCode> {
    let kennung = id.clone();
    let stand = db(&zustand, move |conn| {
        if speicher::satz_benutzt(conn, &kennung)? {
            return Ok(Loeschstand::WirdBenutzt);
        }
        let weg = speicher::loesche(conn, speicher::SAETZE, &kennung)?;
        Ok(if weg {
            Loeschstand::Geloescht
        } else {
            Loeschstand::Fehlt
        })
    })
    .await?;
    match stand {
        Loeschstand::WirdBenutzt => Err(FehlerAntwort::konflikt(format!(
            "Satz {id} wird noch von einer Gruppe benutzt"
        ))),
        Loeschstand::Fehlt => Err(FehlerAntwort::nicht_gefunden(format!(
            "Satz {id} gibt es nicht"
        ))),
        Loeschstand::Geloescht => {
            zustand.melde_aenderung();
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

// ----------------------------------------------------------------- Gruppen

pub(crate) async fn gruppen_liste(
    State(zustand): State<Arc<Zustand>>,
) -> Ergebnis<Json<Vec<Gruppe>>> {
    Ok(Json(
        db(&zustand, |conn| speicher::liste(conn, speicher::GRUPPEN)).await?,
    ))
}

pub(crate) async fn gruppe_holen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<Response> {
    let kennung = id.clone();
    let gruppe: Gruppe = db(&zustand, move |conn| {
        speicher::hole(conn, speicher::GRUPPEN, &kennung)
    })
    .await?
    .ok_or_else(|| FehlerAntwort::nicht_gefunden(format!("Gruppe {id} gibt es nicht")))?;
    mit_etag(gruppe)
}

pub(crate) async fn gruppe_setzen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Koerper(gruppe): Koerper<Gruppe>,
) -> Ergebnis<Response> {
    if gruppe.id.als_str() != id {
        return Err(FehlerAntwort::ungueltig(format!(
            "Kennung im Pfad ({id}) und im Rumpf ({}) stimmen nicht überein",
            gruppe.id
        )));
    }
    let mut gruppe = gruppe;
    for id in &mut gruppe.entra {
        if !crate::config::ist_guid(id) {
            return Err(FehlerAntwort::ungueltig(format!(
                "Gruppe {id}: entra enthält keine Objekt-Id (GUID)"
            )));
        }
        *id = id.to_ascii_lowercase();
    }
    crate::grenzen::gruppe_pruefen(&gruppe).map_err(FehlerAntwort::unverarbeitbar)?;
    let zu_speichern = gruppe.clone();
    let kennung = id.clone();
    let (unbekannt, konflikt) = db(&zustand, move |conn| {
        let alt: Option<Gruppe> = speicher::hole(conn, speicher::GRUPPEN, &kennung)?;
        if stand_pruefen(&headers, alt.as_ref(), &format!("Gruppe {kennung}")).is_err() {
            return Ok((Vec::new(), true));
        }
        // Unbekannte Sätze werden benannt und abgelehnt (422) – eine Gruppe,
        // die auf nichts zeigt, wäre still wirkungslos.
        let unbekannt = speicher::unbekannte_saetze(conn, &zu_speichern)?;
        if unbekannt.is_empty() {
            speicher::setze(conn, speicher::GRUPPEN, &kennung, &zu_speichern)?;
        }
        Ok((unbekannt, false))
    })
    .await?;
    if konflikt {
        return Err(FehlerAntwort::inzwischen_geaendert(&format!("Gruppe {id}")));
    }
    if !unbekannt.is_empty() {
        return Err(FehlerAntwort::unverarbeitbar(format!(
            "Gruppe {id} verweist auf unbekannte Sätze: {}",
            unbekannt.join(", ")
        )));
    }
    zustand.melde_aenderung();
    mit_etag(gruppe)
}

pub(crate) async fn gruppe_loeschen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<StatusCode> {
    let jetzt = zustand.uhr.jetzt();
    let kennung = id.clone();
    let weg = db(&zustand, move |conn| {
        let gruppe: Option<Gruppe> = speicher::hole(conn, speicher::GRUPPEN, &kennung)?;
        let weg = speicher::loesche(conn, speicher::GRUPPEN, &kennung)?;
        if let (true, Some(gruppe)) = (weg, gruppe) {
            // Eine automatisch angelegte Gruppe bleibt gelöscht: die Id wird
            // gemerkt, bis der Admin sie wieder aufnimmt (Aufgabe 23).
            if !gruppe.entra.is_empty() {
                speicher::entra_gruppe_loeschen_merken(conn, &gruppe.entra, jetzt)?;
            }
        }
        Ok(weg)
    })
    .await?;
    if !weg {
        return Err(FehlerAntwort::nicht_gefunden(format!(
            "Gruppe {id} gibt es nicht"
        )));
    }
    zustand.melde_aenderung();
    Ok(StatusCode::NO_CONTENT)
}

// ----------------------------------------------------------------- Geräte

/// Der Besitzer zum Anzeigen; die Objekt-Id bleibt der Schlüssel.
fn besitzer_von(conn: &rusqlite::Connection, oid: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(oid) = oid else {
        return Ok(None);
    };
    Ok(Some(match speicher::benutzer_holen(conn, oid)? {
        Some(benutzer) => benutzer
            .email
            .or(benutzer.name)
            .unwrap_or_else(|| oid.to_string()),
        None => oid.to_string(),
    }))
}

pub(crate) async fn geraete_liste(
    State(zustand): State<Arc<Zustand>>,
) -> Ergebnis<Json<Vec<GeraetStand>>> {
    let mut staende = db(&zustand, |conn| {
        let bestand = speicher::bestand(conn)?;
        let mut aus = Vec::new();
        for zeile in speicher::alle_geraete(conn)? {
            let besitzer = besitzer_von(conn, zeile.oid.as_deref())?;
            aus.push(GeraetStand {
                besitzer,
                soll_revision: soll_aus(&bestand, &zeile.geraet)?.revision,
                geraet: zeile.geraet,
                meldung: zeile.meldung,
                zuletzt_gesehen: zeile.zuletzt_gesehen,
                verbunden: false,
            });
        }
        Ok(aus)
    })
    .await?;
    let verbunden = zustand.verbundene();
    for stand in &mut staende {
        stand.verbunden = verbunden.contains(&stand.geraet.id);
    }
    Ok(Json(staende))
}

pub(crate) async fn geraet_setzen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Koerper(aenderung): Koerper<GeraetAendern>,
) -> Ergebnis<Json<GeraetStand>> {
    let name = aenderung.name.trim();
    if name.is_empty() {
        return Err(FehlerAntwort::unverarbeitbar("Name fehlt"));
    }
    if name.chars().count() > crate::grenzen::NAME_MAX {
        return Err(FehlerAntwort::unverarbeitbar(format!(
            "Name ist zu lang (höchstens {} Zeichen)",
            crate::grenzen::NAME_MAX
        )));
    }
    let kennung = id.clone();
    let stand = db(&zustand, move |conn| {
        let Some(zeile) = speicher::hole_geraet(conn, &kennung)? else {
            return Ok(Err(FehlerAntwort::nicht_gefunden(format!(
                "Gerät {kennung} gibt es nicht"
            ))));
        };
        // Gleichzeitiges Bearbeiten: der Stand der Geräte-Daten (Name, Gruppen,
        // Sperre) ist der Vergleichswert.
        if stand_pruefen(&headers, Some(&zeile.geraet), &format!("Gerät {kennung}")).is_err() {
            return Ok(Err(FehlerAntwort::inzwischen_geaendert(&format!(
                "Gerät {kennung}"
            ))));
        }
        let manuell = aenderung.gruppen.clone();
        let entra = match zeile.oid.as_deref() {
            Some(oid) => speicher::benutzer_holen(conn, oid)?
                .map(|benutzer| benutzer.entra)
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let wirksam = speicher::wirksame_gruppen(conn, &manuell, &entra)?;
        let geraet = Geraet {
            id: zeile.geraet.id,
            name: aenderung.name,
            gruppen: wirksam,
            gesperrt: aenderung.gesperrt,
        };
        speicher::setze_manuell(conn, geraet.id.als_str(), &geraet, &manuell)?;
        let bestand = speicher::bestand(conn)?;
        let soll = soll_aus(&bestand, &geraet)?;
        let besitzer = besitzer_von(conn, zeile.oid.as_deref())?;
        Ok(Ok(GeraetStand {
            besitzer,
            soll_revision: soll.revision,
            geraet,
            meldung: zeile.meldung,
            zuletzt_gesehen: zeile.zuletzt_gesehen,
            verbunden: false,
        }))
    })
    .await??;
    zustand.melde_aenderung();
    let mut stand = stand;
    stand.verbunden = zustand.verbundene().contains(&stand.geraet.id);
    Ok(Json(stand))
}

pub(crate) async fn geraet_loeschen(
    State(zustand): State<Arc<Zustand>>,
    Path(id): Path<String>,
) -> Ergebnis<StatusCode> {
    let kennung = id.clone();
    let weg = db(&zustand, move |conn| {
        speicher::loesche_geraet(conn, &kennung)
    })
    .await?;
    if !weg {
        return Err(FehlerAntwort::nicht_gefunden(format!(
            "Gerät {id} gibt es nicht"
        )));
    }
    zustand.melde_aenderung();
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ Codes, Rest

pub(crate) async fn code_anlegen(
    State(zustand): State<Arc<Zustand>>,
    Koerper(anfrage): Koerper<CodeAnlegen>,
) -> Ergebnis<Json<Code>> {
    if zustand.oidc.is_some() {
        return Err(FehlerAntwort::nicht_gefunden(
            "dieser Server nutzt Entra; Einmal-Codes gibt es hier nicht",
        ));
    }
    let gueltig_bis = if anfrage.unbegrenzt {
        None
    } else {
        let minuten = anfrage.minuten.unwrap_or(60);
        Some(hilfen::als_text(
            hilfen::jetzt() + time::Duration::minutes(i64::from(minuten)),
        ))
    };
    let gruppen = anfrage.gruppen;
    let bis = gueltig_bis
        .clone()
        .unwrap_or_else(|| speicher::UNBEGRENZT.to_string());
    let code = db(&zustand, move |conn| {
        for _ in 0..8 {
            let anzeige = hilfen::code_erzeugen();
            let normal =
                hilfen::code_normalisieren(&anzeige).expect("ein erzeugter Code ist immer gültig");
            if speicher::code_existiert(conn, &normal)? {
                continue;
            }
            speicher::lege_code_an(conn, &normal, &gruppen, &bis)?;
            return Ok(anzeige);
        }
        anyhow::bail!("kein freier Code gefunden");
    })
    .await?;
    zustand.melde_aenderung();
    Ok(Json(Code { code, gueltig_bis }))
}

pub(crate) async fn ueberblick(State(zustand): State<Arc<Zustand>>) -> Ergebnis<Json<Ueberblick>> {
    let (eintraege, saetze, gruppen, geraete) = db(&zustand, |conn| {
        Ok((
            speicher::anzahl(conn, speicher::EINTRAEGE)?,
            speicher::anzahl(conn, speicher::SAETZE)?,
            speicher::anzahl(conn, speicher::GRUPPEN)?,
            speicher::anzahl(conn, speicher::GERAETE)?,
        ))
    })
    .await?;
    Ok(Json(Ueberblick {
        eintraege,
        saetze,
        gruppen,
        geraete,
        geraete_verbunden: u32::try_from(zustand.verbundene().len()).unwrap_or(u32::MAX),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}
