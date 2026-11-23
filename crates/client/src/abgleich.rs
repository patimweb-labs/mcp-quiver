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
use quiver_protocol::{Anmelden, AnmeldenAntwort, Fehler, Meldung, EREIGNIS_SOLL, PRAEFIX};

use crate::sse;
use crate::{anwenden, Innen, Verbindung};

/// Wie lange der Ereignis-Strom schweigen darf, bevor wir die Verbindung
/// für tot halten. Der Server schickt alle 15 s ein Keep-Alive.
const STROM_STILLE: Duration = Duration::from_secs(40);

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
                innen.verbunden_setzen(&server);
                anwenden(&innen, soll).await;
                backoff = 1;
                match strom(&innen, &server, &token).await {
                    StromEnde::Abgebrochen => return,
                    StromEnde::NeuVerbinden => continue,
                    StromEnde::Fehler(fehler) => {
                        let text = format!("{fehler:#}");
                        tracing::warn!("Ereignis-Strom weg: {text}");
                        innen.verbindung_setzen(Verbindung::Getrennt {
                            server: server.clone(),
                            fehler: text,
                        });
                    }
                }
            }
            Err(fehler) => {
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

/// Der Ereignis-Strom: sofort die aktuelle Revision, danach bei jeder Änderung.
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
    loop {
        let stueck = tokio::select! {
            _ = innen.ende.warten() => return StromEnde::Abgebrochen,
            _ = innen.erneut.notified() => return StromEnde::NeuVerbinden,
            _ = tokio::time::sleep(STROM_STILLE) => {
                return StromEnde::Fehler(anyhow::anyhow!("Der Ereignis-Strom schweigt"));
            }
            stueck = koerper.next() => match stueck {
                Some(Ok(stueck)) => stueck,
                Some(Err(fehler)) => return StromEnde::Fehler(fehler.into()),
                None => return StromEnde::Fehler(anyhow::anyhow!("Der Ereignis-Strom ist zu")),
            },
        };

        for ereignis in leser.schieben(&String::from_utf8_lossy(&stueck)) {
            if ereignis.name != EREIGNIS_SOLL {
                continue;
            }
            let letzte = innen.stand().soll.revision.clone();
            if ereignis.daten == letzte {
                continue;
            }
            match soll_holen(&innen.http, server, token).await {
                Ok(soll) => anwenden(innen, soll).await,
                Err(fehler) => return StromEnde::Fehler(fehler),
            }
        }
    }
}

/// Gerät mit Einmal-Code beim Server einbinden.
pub(crate) async fn anmelden(
    client: &reqwest::Client,
    server: &str,
    code: &str,
    name: &str,
) -> anyhow::Result<AnmeldenAntwort> {
    let antwort = client
        .post(format!("{server}{PRAEFIX}/anmelden"))
        .json(&Anmelden {
            code: code.to_string(),
            name: name.to_string(),
        })
        .send()
        .await
        .context("Server nicht erreichbar")?;
    let antwort = pruefen(antwort).await?;
    antwort.json().await.context("Antwort der Anmeldung")
}

/// Den Soll-Zustand des eigenen Geräts holen.
pub(crate) async fn soll_holen(
    client: &reqwest::Client,
    server: &str,
    token: &str,
) -> anyhow::Result<Soll> {
    let antwort = client
        .get(format!("{server}{PRAEFIX}/geraet/soll"))
        .bearer_auth(token)
        .send()
        .await
        .context("Soll nicht abholbar")?;
    let antwort = pruefen(antwort).await?;
    antwort.json().await.context("Antwort des Solls")
}

/// Den eigenen Stand an den Server melden.
pub(crate) async fn meldung_senden(
    client: &reqwest::Client,
    server: &str,
    token: &str,
    meldung: &Meldung,
) -> anyhow::Result<()> {
    let antwort = client
        .post(format!("{server}{PRAEFIX}/geraet/meldung"))
        .bearer_auth(token)
        .json(meldung)
        .send()
        .await
        .context("Meldung nicht zustellbar")?;
    pruefen(antwort).await?;
    Ok(())
}

/// Erfolg durchlassen, sonst den Fehlertext des Servers weitergeben.
async fn pruefen(antwort: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = antwort.status();
    if status.is_success() {
        return Ok(antwort);
    }
    let text = antwort.text().await.unwrap_or_default();
    let meldung = serde_json::from_str::<Fehler>(&text)
        .map(|fehler| fehler.fehler)
        .unwrap_or(text);
    anyhow::bail!("Server antwortet mit {status}: {meldung}")
}
