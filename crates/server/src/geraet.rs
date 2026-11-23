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
use tokio_stream::wrappers::BroadcastStream;

use crate::fehler::{Ergebnis, FehlerAntwort, Koerper};
use crate::hilfen;
use crate::speicher;
use crate::zustand::{Stromwaechter, Zustand};
use crate::{db, soll_fuer};

/// Gerät mit Einmal-Code einbinden; Antwort enthält das Geräte-Token (nur diesmal).
pub(crate) async fn anmelden(
    State(zustand): State<Arc<Zustand>>,
    Koerper(anfrage): Koerper<Anmelden>,
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

    let antwort = db(&zustand, move |conn| {
        let gruppen = match speicher::code_einloesen(conn, &code, hilfen::jetzt())? {
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
        speicher::lege_geraet_an(conn, &geraet, &hash)?;
        Ok(Ok(AnmeldenAntwort { geraet, token }))
    })
    .await??;
    Ok(Json(antwort))
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
pub(crate) async fn meldung(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Koerper(meldung): Koerper<Meldung>,
) -> Ergebnis<StatusCode> {
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

/// Ereignis-Strom: sofort die aktuelle Revision, danach nur bei Änderung.
pub(crate) async fn ereignisse(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
) -> Ergebnis<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>> {
    let start = soll_fuer(&zustand, &id).await?.revision;
    let empfaenger = zustand.geaendert.subscribe();
    let waechter = Stromwaechter::neu(zustand.clone(), id.clone());

    let weitere = futures_util::stream::unfold(
        (
            BroadcastStream::new(empfaenger),
            zustand.clone(),
            id.clone(),
            start.clone(),
            waechter,
        ),
        |(mut empfaenger, zustand, id, letzte, waechter)| async move {
            loop {
                // Jede Änderung (auch ein Rückstand) ist ein Anlass, neu zu rechnen.
                empfaenger.next().await.as_ref()?;
                match soll_fuer(&zustand, &id).await {
                    Ok(soll) if soll.revision != letzte => {
                        let ereignis = Event::default()
                            .event(EREIGNIS_SOLL)
                            .data(soll.revision.clone());
                        let zustand_strom = (empfaenger, zustand, id, soll.revision, waechter);
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
