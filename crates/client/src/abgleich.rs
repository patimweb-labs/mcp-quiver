// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Abgleich mit dem Server (SPEC.md §7, Aufgabe 04):
//!
//! 1. eingebunden → `GET /geraet/soll`, dann `GET /geraet/ereignisse` offen halten,
//! 2. neue Revision → `GET /geraet/soll` → [`quiver_domain::abgleichen`] → Aktionen,
//! 3. Verbindung weg → `Getrennt`, der letzte Soll läuft weiter, neu verbinden
//!    mit Backoff 1 s … 30 s.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use quiver_domain::Soll;
use quiver_protocol::{
    AnmeldeGrund, Anmelden, AnmeldenAntwort, Fehler, Meldung, EREIGNIS_SOLL, PRAEFIX,
};

use crate::netz;
use crate::sse;
use crate::{anwenden, Innen, Verbindung};

/// Wie lange der Ereignis-Strom schweigen darf, bevor wir die Verbindung
/// für tot halten. Der Server schickt alle 15 s ein Keep-Alive.
const STROM_STILLE: Duration = crate::netz::STROM_FRIST;

/// So oft wird der Soll-Zustand zusätzlich geholt, auch wenn der Server
/// nichts meldet (Paket 3): ± Streuung, damit viele Clients nicht im Takt
/// anfragen.
const NACHFRAGE_ABSTAND: Duration = Duration::from_secs(5 * 60);
/// Die Streuung um [`NACHFRAGE_ABSTAND`].
const NACHFRAGE_STREUUNG: Duration = Duration::from_secs(30);

/// Wie der Ereignis-Strom endet.
enum StromEnde {
    /// Der Kern fährt herunter.
    Abgebrochen,
    /// Neu eingebunden – sofort wieder verbinden.
    NeuVerbinden,
    /// Verbindung weg.
    Fehler(anyhow::Error),
}

/// Die Aufsicht über die Server-Verbindung.
pub(crate) async fn lauf(innen: Arc<Innen>) {
    let mut backoff = 1u64;
    loop {
        if innen.ende.ist_abgebrochen() {
            return;
        }
        let (server, token) = innen.zugang();
        let (Some(server), Some(token)) = (server, token) else {
            innen.verbindung_setzen(Verbindung::NichtEingebunden);
            tokio::select! {
                _ = innen.ende.warten() => return,
                _ = innen.erneut.notified() => continue,
            }
        };

        innen.verbindung_setzen(Verbindung::Verbindet {
            server: server.clone(),
        });
        match soll_holen(&innen.http, &server, &token).await {
            Ok(soll) => {
                // Zwischen Holen und Anwenden kann sich abgemeldet oder neu
                // angemeldet worden sein — dann gehört dieses Ergebnis zu
                // einem alten Token und wird verworfen (Aufgabe 22).
                if !gilt_noch(&innen, &token) {
                    continue;
                }
                innen.verbunden_setzen(&server);
                anwenden(&innen, soll).await;
                backoff = 1;
                match strom(&innen, &server, &token).await {
                    StromEnde::Abgebrochen => return,
                    StromEnde::NeuVerbinden => continue,
                    StromEnde::Fehler(fehler) if ist_abgelehnt(&fehler) => {
                        if !gilt_noch(&innen, &token) {
                            continue;
                        }
                        tracing::warn!("{fehler:#} – alle Tools gestoppt");
                        innen.ablehnung_behandeln(&server, &fehler).await;
                        tokio::select! {
                            _ = innen.ende.warten() => return,
                            _ = innen.erneut.notified() => {}
                        }
                        continue;
                    }
                    StromEnde::Fehler(fehler) => {
                        if !gilt_noch(&innen, &token) {
                            continue;
                        }
                        let text = format!("{fehler:#}");
                        tracing::warn!("Ereignis-Strom weg: {text}");
                        innen.verbindung_setzen(Verbindung::Getrennt {
                            server: server.clone(),
                            fehler: text,
                        });
                    }
                }
            }
            Err(fehler) if ist_abgelehnt(&fehler) => {
                if !gilt_noch(&innen, &token) {
                    continue;
                }
                tracing::warn!("{fehler:#} – alle Tools gestoppt");
                innen.ablehnung_behandeln(&server, &fehler).await;
                tokio::select! {
                    _ = innen.ende.warten() => return,
                    _ = innen.erneut.notified() => {}
                }
                continue;
            }
            Err(fehler) => {
                if !gilt_noch(&innen, &token) {
                    continue;
                }
                let text = format!("{fehler:#}");
                tracing::warn!("Soll nicht abholbar: {text}");
                innen.verbindung_setzen(Verbindung::Getrennt {
                    server: server.clone(),
                    fehler: text,
                });
            }
        }

        tokio::select! {
            _ = innen.ende.warten() => return,
            _ = innen.erneut.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {
                backoff = (backoff * 2).min(30);
            }
        }
    }
}

/// Gilt das Token dieses Durchlaufs noch? Nach Abmelden oder Neu-Anmelden
/// nicht mehr; dann darf ein altes Ergebnis nichts überschreiben.
fn gilt_noch(innen: &Arc<Innen>, token: &str) -> bool {
    innen.zugang().1.as_deref() == Some(token)
}

/// Der Ereignis-Strom: sofort die aktuelle Revision, danach bei jeder Änderung.
///
/// Zusätzlich wird der Soll-Zustand alle [`NACHFRAGE_ABSTAND`] (± Streuung)
/// nachgefragt (Paket 3): ein verlorenes Ereignis – etwa weil der Server neu
/// gestartet ist oder eine Meldung unterging – fällt dann spätestens nach
/// fünf Minuten auf.
async fn strom(innen: &Arc<Innen>, server: &str, token: &str) -> StromEnde {
    let anfrage = innen
        .http
        .get(format!("{server}{PRAEFIX}/geraet/ereignisse"))
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "text/event-stream");
    let antwort = match anfrage.send().await {
        Ok(antwort) => antwort,
        Err(fehler) => return StromEnde::Fehler(fehler.into()),
    };
    let antwort = match pruefen(antwort).await {
        Ok(antwort) => antwort,
        Err(fehler) => return StromEnde::Fehler(fehler),
    };

    let mut koerper = antwort.bytes_stream();
    let mut leser = sse::Leser::default();
    let mut gemeldet_verworfen = 0u64;
    let mut nachfrage = tokio::time::interval(nachfrage_abstand());
    nachfrage.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    nachfrage.tick().await;
    loop {
        let stueck = tokio::select! {
            _ = innen.ende.warten() => return StromEnde::Abgebrochen,
            _ = innen.erneut.notified() => return StromEnde::NeuVerbinden,
            _ = tokio::time::sleep(STROM_STILLE) => {
                return StromEnde::Fehler(anyhow::anyhow!("Der Ereignis-Strom schweigt"));
            }
            _ = nachfrage.tick() => {
                // Von selbst nachfragen: nur, wenn sich die Revision ändert,
                // wird etwas angewendet.
                let letzte = innen.stand().soll.revision.clone();
                match soll_holen(&innen.http, server, token).await {
                    Ok(soll) if gilt_noch(innen, token) => {
                        if soll.revision != letzte {
                            anwenden(innen, soll).await;
                        }
                    }
                    Ok(_) => return StromEnde::NeuVerbinden,
                    Err(fehler) => return StromEnde::Fehler(fehler),
                }
                continue;
            }
            stueck = koerper.next() => match stueck {
                Some(Ok(stueck)) => stueck,
                Some(Err(fehler)) => return StromEnde::Fehler(fehler.into()),
                None => return StromEnde::Fehler(anyhow::anyhow!("Der Ereignis-Strom ist zu")),
            },
        };

        let ereignisse = leser.schieben(&String::from_utf8_lossy(&stueck));
        // Eine Zeile über 64 KB wird verworfen (Paket 3): sagen, nicht
        // stillschweigend schlucken.
        if leser.verworfene_zeilen() > gemeldet_verworfen {
            gemeldet_verworfen = leser.verworfene_zeilen();
            tracing::warn!(
                "Eine Zeile des Ereignis-Stroms war länger als 64 KB und wurde verworfen"
            );
        }
        for ereignis in ereignisse {
            if ereignis.name != EREIGNIS_SOLL {
                continue;
            }
            // Nach Abmelden oder Neu-Anmelden gehört dieses Ereignis zu einem
            // alten Token (Aufgabe 22).
            if !gilt_noch(innen, token) {
                return StromEnde::NeuVerbinden;
            }
            let letzte = innen.stand().soll.revision.clone();
            if ereignis.daten == letzte {
                continue;
            }
            match soll_holen(&innen.http, server, token).await {
                Ok(soll) if gilt_noch(innen, token) => anwenden(innen, soll).await,
                Ok(_) => return StromEnde::NeuVerbinden,
                Err(fehler) => return StromEnde::Fehler(fehler),
            }
        }
    }
}

/// Der Abstand der zusätzlichen Nachfrage (für Prüfungen).
#[cfg(test)]
pub(crate) fn nachfrage_abstand_fuer_test() -> Duration {
    nachfrage_abstand()
}

/// Der Abstand der zusätzlichen Nachfrage: fünf Minuten ± Streuung.
fn nachfrage_abstand() -> Duration {
    use rand::Rng;
    let streuung = rand::rng().random_range(0..=NACHFRAGE_STREUUNG.as_secs());
    NACHFRAGE_ABSTAND + Duration::from_secs(streuung)
}

/// Gerät mit Einmal-Code beim Server einbinden.
pub(crate) async fn anmelden(
    client: &reqwest::Client,
    server: &str,
    code: &str,
    name: &str,
) -> anyhow::Result<AnmeldenAntwort> {
    let antwort = netz::frist(
        client
            .post(format!("{server}{PRAEFIX}/anmelden"))
            .json(&Anmelden {
                code: code.to_string(),
                name: name.to_string(),
            }),
    )
    .send()
    .await
    .context("Server nicht erreichbar")?;
    let antwort = pruefen(antwort).await?;
    netz::json_begrenzt(antwort, netz::ANTWORT_BYTES)
        .await
        .context("Antwort der Anmeldung")
}

/// Den Soll-Zustand des eigenen Geräts holen.
pub(crate) async fn soll_holen(
    client: &reqwest::Client,
    server: &str,
    token: &str,
) -> anyhow::Result<Soll> {
    let antwort = netz::frist(
        client
            .get(format!("{server}{PRAEFIX}/geraet/soll"))
            .bearer_auth(token),
    )
    .send()
    .await
    .context("Soll nicht abholbar")?;
    let antwort = pruefen(antwort).await?;
    // Der Soll-Zustand darf größer sein als andere Antworten (viele Einträge,
    // viele Werte), aber nicht unbegrenzt.
    netz::json_begrenzt(antwort, netz::SOLL_BYTES)
        .await
        .context("Antwort des Solls")
}

/// Das eigene Gerät am Server löschen (Aufgabe 22: „Abmelden“).
///
/// Der Server löscht es; danach gilt das Token nicht mehr. Ist der Server
/// nicht erreichbar, bleibt das Gerät dort stehen — der Aufrufer sagt das.
pub(crate) async fn geraet_abmelden(
    client: &reqwest::Client,
    server: &str,
    token: &str,
) -> anyhow::Result<()> {
    let antwort = netz::frist(
        client
            .post(format!("{server}{PRAEFIX}/geraet/abmelden"))
            .bearer_auth(token),
    )
    .send()
    .await
    .context("Server nicht erreichbar")?;
    pruefen(antwort).await?;
    Ok(())
}

/// Den eigenen Stand an den Server melden.
pub(crate) async fn meldung_senden(
    client: &reqwest::Client,
    server: &str,
    token: &str,
    meldung: &Meldung,
) -> anyhow::Result<()> {
    let antwort = netz::frist(
        client
            .post(format!("{server}{PRAEFIX}/geraet/meldung"))
            .bearer_auth(token)
            .json(meldung),
    )
    .send()
    .await
    .context("Meldung nicht zustellbar")?;
    pruefen(antwort).await?;
    Ok(())
}

/// Der Server nimmt das Gerät nicht (mehr) an: 401 oder 403 auf eine
/// Geräte-Anfrage. Dann läuft kein Tool mehr, bis das Gerät neu eingebunden ist.
///
/// Seit Aufgabe 20 trägt `grund` den maschinenlesbaren Grund mit (`gesperrt`,
/// `neu-anmelden`, `kein-zugriff`). Der Client entscheidet daran, ob das
/// Geräte-Token bleibt (nur bei `neu-anmelden`).
#[derive(Debug, thiserror::Error)]
#[error("Der Server nimmt dieses Gerät nicht mehr an ({status}): {meldung}")]
pub(crate) struct GeraetAbgelehnt {
    status: reqwest::StatusCode,
    meldung: String,
    grund: Option<AnmeldeGrund>,
}

/// Ist dieser Fehler eine Ablehnung des Geräts?
pub(crate) fn ist_abgelehnt(fehler: &anyhow::Error) -> bool {
    fehler.chain().any(|teil| teil.is::<GeraetAbgelehnt>())
}

/// Der Status einer Geräte-Ablehnung (401 oder 403).
pub(crate) fn status_von(fehler: &anyhow::Error) -> Option<reqwest::StatusCode> {
    fehler
        .chain()
        .find_map(|teil| teil.downcast_ref::<GeraetAbgelehnt>())
        .map(|abgelehnt| abgelehnt.status)
}

/// Der maschinenlesbare Grund einer Geräte-Ablehnung, falls der Server einen
/// genannt hat. Ein unbekannter Grund bleibt `None` und wird wie
/// `neu-anmelden` behandelt.
pub(crate) fn grund_von(fehler: &anyhow::Error) -> Option<AnmeldeGrund> {
    fehler
        .chain()
        .find_map(|teil| teil.downcast_ref::<GeraetAbgelehnt>())
        .and_then(|abgelehnt| abgelehnt.grund)
}

/// Erfolg durchlassen, sonst den Fehlertext des Servers weitergeben.
pub(crate) async fn pruefen(antwort: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = antwort.status();
    if status.is_success() {
        return Ok(antwort);
    }
    // Auch der Fehlertext ist begrenzt: ein Server darf hier nicht beliebig
    // viel schicken.
    let bytes = netz::rumpf_begrenzt(antwort, 64 * 1024)
        .await
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).to_string();
    let (meldung, grund) = fehler_aus_text(&text);
    if matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        return Err(GeraetAbgelehnt {
            status,
            meldung,
            grund,
        }
        .into());
    }
    anyhow::bail!("Server antwortet mit {status}: {meldung}")
}

/// Den Fehlertext des Servers lesen. Ein unbekannter `grund` lässt die
/// **ganze** Antwort unlesbar werden – dann gilt `None`, und der Client
/// behandelt das wie `neu-anmelden` (Aufgabe 22: „Unbekannter Grund →
/// wie neu_anmelden“).
fn fehler_aus_text(text: &str) -> (String, Option<AnmeldeGrund>) {
    match serde_json::from_str::<Fehler>(text) {
        Ok(fehler) => (fehler.fehler, fehler.grund),
        Err(_) => (text.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_grund_wird_gelesen_und_ein_unbekannter_bleibt_neutral() {
        assert_eq!(
            fehler_aus_text(r#"{"fehler":"gesperrt","grund":"gesperrt"}"#),
            ("gesperrt".to_string(), Some(AnmeldeGrund::Gesperrt))
        );
        assert_eq!(
            fehler_aus_text(r#"{"fehler":"neu","grund":"neu-anmelden"}"#),
            ("neu".to_string(), Some(AnmeldeGrund::NeuAnmelden))
        );
        // Unbekannt: die Antwort ist nicht lesbar, kein Grund. Der Aufrufer
        // behandelt das wie `neu-anmelden`.
        assert_eq!(
            fehler_aus_text(r#"{"fehler":"x","grund":"erfunden"}"#),
            (r#"{"fehler":"x","grund":"erfunden"}"#.to_string(), None)
        );
        // Alter Server ohne `grund`.
        assert_eq!(
            fehler_aus_text(r#"{"fehler":"weg"}"#),
            ("weg".to_string(), None)
        );
        // Kein JSON.
        assert_eq!(fehler_aus_text("kaputt"), ("kaputt".to_string(), None));
    }
}
