// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Geräte-Seite: Anmelden mit Einmal-Code, Soll holen, melden, Ereignis-Strom.

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::StreamExt;
use quiver_domain::{Geraet, GeraetId, Soll};
use quiver_protocol::{Anmelden, AnmeldenAntwort, Meldung, EREIGNIS_SOLL};
use time::OffsetDateTime;
use tokio_stream::wrappers::BroadcastStream;

use crate::fehler::{Ergebnis, FehlerAntwort, Koerper};
use crate::hilfen;
use crate::speicher;
use crate::zustand::{Stromwaechter, Zustand};
use crate::{db, soll_fuer};

/// Gerät mit Einmal-Code einbinden; Antwort enthält das Geräte-Token (nur diesmal).
pub(crate) async fn anmelden(
    State(zustand): State<Arc<Zustand>>,
    req: axum::extract::Request,
) -> Ergebnis<Json<AnmeldenAntwort>> {
    if zustand.oidc.is_some() {
        return Err(FehlerAntwort::nicht_gefunden(
            "dieser Server nutzt Entra; Einmal-Codes gibt es hier nicht (siehe /auth/geraet/start)",
        ));
    }
    let ip = crate::sitzung::aufrufer(&req, &zustand);
    let (_, rumpf) = req.into_parts();
    let bytes = axum::body::to_bytes(rumpf, crate::RUMPF_STANDARD)
        .await
        .map_err(|fehler| {
            if ist_zu_gross(&fehler) {
                FehlerAntwort::zu_gross()
            } else if crate::fehler::ursache_ist::<tower_http::timeout::TimeoutError>(&fehler) {
                FehlerAntwort::zeitlimit()
            } else {
                FehlerAntwort::ungueltig("Rumpf nicht lesbar")
            }
        })?;
    let anfrage: Anmelden = serde_json::from_slice(&bytes)
        .map_err(|_| FehlerAntwort::ungueltig("Rumpf ist kein JSON mit code und name"))?;

    let jetzt = zustand.uhr.jetzt();
    let ip_schluessel = crate::drossel::ip_schluessel(ip);
    let code_schluessel = format!(
        "code:{}",
        anfrage
            .code
            .trim()
            .to_ascii_uppercase()
            .replace(['-', ' '], "")
    );
    for schluessel in [&ip_schluessel, &code_schluessel] {
        if let Some(sekunden) = zustand.drossel.wartezeit(schluessel, jetzt) {
            return Err(FehlerAntwort::neu(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                format!("zu viele Versuche; bitte {sekunden} Sekunden warten"),
            ));
        }
    }

    let ergebnis = anmelden_intern(&zustand, anfrage, jetzt).await;
    for schluessel in [&ip_schluessel, &code_schluessel] {
        if ergebnis.is_ok() {
            zustand.drossel.erfolg(schluessel);
        } else {
            zustand.drossel.fehlschlag(schluessel, jetzt);
        }
    }
    ergebnis
}

/// Ist der Rumpf an der Grenze gescheitert (und nicht an der Leitung)?
fn ist_zu_gross(fehler: &axum::Error) -> bool {
    use std::error::Error as _;
    fehler
        .source()
        .is_some_and(|quelle| quelle.is::<http_body_util::LengthLimitError>())
}

async fn anmelden_intern(
    zustand: &Arc<Zustand>,
    anfrage: Anmelden,
    jetzt: OffsetDateTime,
) -> Ergebnis<Json<AnmeldenAntwort>> {
    let code = hilfen::code_normalisieren(&anfrage.code)
        .ok_or_else(|| FehlerAntwort::ungueltig("Code hat nicht die Form XXXX-XXXX"))?;
    let name = anfrage.name.trim().to_string();
    if name.is_empty() {
        return Err(FehlerAntwort::ungueltig("Name fehlt"));
    }
    let token = hilfen::token_erzeugen();
    let hash = hilfen::hash(&token);
    let kennung = hilfen::geraet_id_erzeugen();

    let antwort = db(zustand, move |conn| {
        let gruppen = match speicher::code_einloesen(conn, &code, jetzt)? {
            speicher::CodeStand::Unbekannt => {
                return Ok(Err(FehlerAntwort::ungueltig(
                    "Code unbekannt oder schon benutzt",
                )))
            }
            speicher::CodeStand::Abgelaufen => {
                return Ok(Err(FehlerAntwort::abgelaufen("Code ist abgelaufen")))
            }
            speicher::CodeStand::Gueltig(gruppen) => gruppen,
        };
        let geraet = Geraet {
            id: GeraetId::neu(kennung),
            name,
            gruppen,
            gesperrt: false,
        };
        speicher::lege_geraet_an(conn, &geraet, &hash, None, None, None)?;
        speicher::pruefspur_schreiben(
            conn,
            jetzt,
            "geraet",
            "Einmal-Code",
            &format!("Gerät {} eingebunden", geraet.id),
        )?;
        Ok(Ok(AnmeldenAntwort {
            geraet,
            token,
            benutzer: None,
        }))
    })
    .await??;
    Ok(Json(antwort))
}

/// `POST /geraet/abmelden`: das eigene Gerät am Server löschen.
///
/// Das Gegenstück zu „Gerät abmelden“ im Client. Es ist absichtlich **kein**
/// Verwaltungsweg: nur das Gerät selbst kann sich mit seinem Token löschen.
/// Ein bereits gelöschtes Gerät ist kein Fehler – der Client kommt ohnehin
/// nicht mehr an diesen Punkt.
pub(crate) async fn abmelden(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
) -> Ergebnis<StatusCode> {
    let kennung = id.clone();
    db(&zustand, move |conn| {
        speicher::loesche_geraet(conn, kennung.als_str())?;
        Ok(())
    })
    .await?;
    zustand.melde_aenderung();
    tracing::info!(geraet = %id, "Gerät hat sich abgemeldet");
    Ok(StatusCode::NO_CONTENT)
}

/// Soll-Zustand holen und den Zeitstempel „zuletzt gesehen" nachziehen.
pub(crate) async fn soll(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
) -> Ergebnis<Json<Soll>> {
    let soll = soll_fuer(&zustand, &id).await?;
    let gesehen = hilfen::als_text(hilfen::jetzt());
    let kennung = id.clone();
    db(&zustand, move |conn| {
        speicher::setze_gesehen(conn, kennung.als_str(), &gesehen)?;
        Ok(())
    })
    .await?;
    Ok(Json(soll))
}

/// Letzte Meldung des Clients speichern.
///
/// Die Meldung wird vor dem Speichern gekürzt (Härtung, Punkt 13): höchstens
/// 200 Einträge, Texte auf 500 Zeichen, Client-Fassung auf 64 Zeichen. Der
/// Rumpf selbst ist bereits auf 64 KB begrenzt.
pub(crate) async fn meldung(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Koerper(mut meldung): Koerper<Meldung>,
) -> Ergebnis<StatusCode> {
    crate::grenzen::meldung_kuerzen(&mut meldung);
    let gesehen = hilfen::als_text(hilfen::jetzt());
    let kennung = id.clone();
    db(&zustand, move |conn| {
        if !speicher::setze_meldung(conn, kennung.als_str(), &meldung, &gesehen)? {
            anyhow::bail!("Gerät {kennung} gibt es nicht");
        }
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Erst abonnieren, dann rechnen.
///
/// Diese Reihenfolge ist der Kern von Punkt 8: wird die Revision vor dem
/// Abonnement berechnet, geht eine Änderung zwischen beiden Schritten
/// verloren – das Gerät bliebe bis zur nächsten Nachfrage auf dem alten Stand.
fn abonnieren_dann_rechnen<T>(
    kanal: &tokio::sync::broadcast::Sender<()>,
    rechnen: impl FnOnce() -> T,
) -> (tokio::sync::broadcast::Receiver<()>, T) {
    let empfaenger = kanal.subscribe();
    let wert = rechnen();
    (empfaenger, wert)
}

/// Ereignis-Strom: sofort die aktuelle Revision, danach nur bei Änderung.
///
/// Die Zahl der offenen Ströme ist begrenzt (je Gerät und insgesamt): ein
/// Client, der immer neu verbindet, ohne zu schließen, bekommt 429.
pub(crate) async fn ereignisse(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
) -> Ergebnis<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>> {
    let Some(waechter) = Stromwaechter::neu(zustand.clone(), id.clone()) else {
        return Err(FehlerAntwort::neu(
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "zu viele offene Ereignis-Ströme (je Gerät höchstens {}, insgesamt {})",
                crate::zustand::STROEME_JE_GERAET,
                crate::zustand::STROEME_GESAMT
            ),
        ));
    };
    // Erst abonnieren, dann die Revision rechnen (siehe
    // [`abonnieren_dann_rechnen`]); sonst fällt eine Änderung dazwischen aus.
    let (empfaenger, start) = abonnieren_dann_rechnen(&zustand.geaendert, || zustand.clone());
    let start = soll_fuer(&start, &id).await?.revision;
    let ende = zustand.ende_abo();

    let weitere = futures_util::stream::unfold(
        (
            BroadcastStream::new(empfaenger),
            zustand.clone(),
            id.clone(),
            start.clone(),
            waechter,
            ende,
        ),
        |(mut empfaenger, zustand, id, letzte, waechter, mut ende)| async move {
            loop {
                // Jede Änderung (auch ein Rückstand) ist ein Anlass, neu zu rechnen.
                let ereignis = tokio::select! {
                    // Der Server fährt herunter: der Strom endet sofort.
                    _ = ende.recv() => None,
                    eintrag = empfaenger.next() => Some(eintrag),
                };
                match ereignis {
                    None | Some(None) => return None,
                    // Ein Rückstand ist kein Ende: neu rechnen genügt.
                    // Auch ein Rückstand ist ein Anlass, neu zu rechnen: dabei
                    // gingen Meldungen verloren.
                    Some(Some(Err(_) | Ok(()))) => {}
                }
                // Entfernt oder gesperrt: der Strom endet. Der Client verbindet
                // neu, bekommt 401/403 und stoppt alle Tools.
                if !crate::auth::geraet_erlaubt(&zustand, &id).await {
                    tracing::info!("Gerät {id} entfernt oder gesperrt – Ereignis-Strom endet");
                    return None;
                }
                match soll_fuer(&zustand, &id).await {
                    Ok(soll) if soll.revision != letzte => {
                        let ereignis = Event::default()
                            .event(EREIGNIS_SOLL)
                            .data(soll.revision.clone());
                        let zustand_strom =
                            (empfaenger, zustand, id, soll.revision, waechter, ende);
                        return Some((Ok::<_, Infallible>(ereignis), zustand_strom));
                    }
                    Ok(_) => {}
                    Err(fehler) => {
                        tracing::warn!("Soll für Gerät {id} nicht berechenbar: {}", fehler.text);
                    }
                }
            }
        },
    );

    let erste = futures_util::stream::once(async move {
        Ok::<_, Infallible>(Event::default().event(EREIGNIS_SOLL).data(start))
    });
    Ok(Sse::new(erste.chain(weitere))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Der Haken zwischen den Schritten: eine Änderung, die genau zwischen
    /// „abonnieren“ und „rechnen“ eintrifft, darf nicht verloren gehen.
    #[test]
    fn ereignis_zwischen_abonnieren_und_rechnen_geht_nicht_verloren() {
        let (sender, _) = tokio::sync::broadcast::channel(8);
        let (mut empfaenger, gesehen) = abonnieren_dann_rechnen(&sender, || {
            // Genau hier passiert die Änderung – nach dem Abonnieren.
            sender.send(()).expect("Empfänger ist da");
            42
        });
        assert_eq!(gesehen, 42);
        assert!(
            empfaenger.try_recv().is_ok(),
            "das Ereignis ist im Abonnement angekommen"
        );
    }

    /// Zum Vergleich: rechnen **vor** dem Abonnieren verliert das Ereignis.
    #[test]
    fn die_alte_reihenfolge_wuerde_das_ereignis_verlieren() {
        let (sender, _) = tokio::sync::broadcast::channel(8);
        // Ein anderer Empfänger hält den Kanal offen; das neue Abonnement
        // bekommt trotzdem nichts von dem, was vorher gesendet wurde.
        let _frueher = sender.subscribe();
        assert!(sender.send(()).is_ok());
        let mut empfaenger = sender.subscribe();
        assert!(empfaenger.try_recv().is_err(), "nichts mehr da");
    }
}
