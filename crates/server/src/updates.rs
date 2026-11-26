// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Updates auf dem Server (Aufgabe 34): **nur anzeigen**.
//!
//! Der Server fragt höchstens alle sechs Stunden die Veröffentlichung ab
//! (GitHub-API, ohne Kennung, ohne Nutzdaten; abschaltbar mit
//! `QUIVER_UPDATES=aus`) und merkt sich das Ergebnis in `update_stand`. Aus
//! diesem Stand beantwortet er zwei Fragen:
//!
//! * **Gerät** (`GET /api/v1/geraet/updates`): ist eine neuere Version
//!   veröffentlicht als die, die du meldest? Der Client zeigt dann einen
//!   ruhigen Hinweis – er lädt und installiert nichts.
//! * **Verwaltung** (`GET /api/v1/admin/updates`): welche Version ist
//!   veröffentlicht, ist der Server selbst älter, und welche Geräte melden
//!   einen älteren Client?
//!
//! Eingespielt wird durch die Geräteverwaltung (Intune) oder von Hand aus den
//! Paketen des Releases – siehe `docs/intune.md`.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Extension, Query, State};
use axum::Json;
use quiver_domain::updates::{self, Veroeffentlichung, ANTWORT_BYTES_MAX};
use quiver_domain::GeraetId;
use quiver_protocol::{UpdateAngebot, UpdateStand, VeralteterClient, VERALTETE_MAX};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::fehler::Ergebnis;
use crate::speicher::{self, UpdateStandZeile};
use crate::zustand::Zustand;
use crate::{db, hilfen};

/// So lange darf eine Antwort der Veröffentlichung höchstens dauern.
const ABRUF_FRIST: Duration = Duration::from_secs(30);

/// Was der Server über die Veröffentlichung weiß.
#[derive(Default)]
pub(crate) struct Lage {
    /// Die zuletzt gelesene Veröffentlichung.
    pub(crate) veroeffentlichung: Option<Veroeffentlichung>,
    /// Wann zuletzt erfolgreich gefragt wurde.
    pub(crate) zuletzt: Option<OffsetDateTime>,
    /// Warum die letzte Abfrage scheiterte (ohne Geheimnisse).
    pub(crate) fehler: Option<String>,
}

impl Lage {
    pub(crate) fn neu() -> Self {
        Self::default()
    }

    /// Den gemerkten Stand übernehmen (nach einem Neustart).
    pub(crate) fn aus_zeile(&mut self, zeile: UpdateStandZeile) {
        self.veroeffentlichung = zeile.veroeffentlichung;
        self.zuletzt = zeile.zuletzt.as_deref().and_then(|text| {
            OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()
        });
    }

    fn als_zeile(&self) -> UpdateStandZeile {
        UpdateStandZeile {
            veroeffentlichung: self.veroeffentlichung.clone(),
            zuletzt: self.zuletzt.map(hilfen::als_text),
        }
    }
}

// ------------------------------------------------------------- Hintergrund

/// Fragt die Veröffentlichung im Hintergrund ab (höchstens alle sechs Stunden,
/// mit Streuung). Mit `QUIVER_UPDATES=aus` wird **nicht** gefragt.
pub(crate) async fn hintergrund(zustand: Arc<Zustand>) {
    let konfig = zustand.konfig.updates.clone();
    if !konfig.an {
        tracing::info!("Update-Prüfung ist aus (QUIVER_UPDATES=aus)");
        return;
    }
    // Den gemerkten Stand übernehmen und den Abstand ab der letzten Frage
    // rechnen: ein Neustart fragt nicht sofort wieder.
    let gemerkt = db(&zustand, |conn| speicher::update_stand(conn))
        .await
        .ok()
        .flatten();
    if let Some(zeile) = gemerkt {
        crate::sperre(&zustand.updates).aus_zeile(zeile);
    }
    loop {
        if let Err(fehler) = pruefen(&zustand).await {
            tracing::warn!("Update-Prüfung: {fehler:#}");
            let mut lage = crate::sperre(&zustand.updates);
            lage.fehler = Some(format!("{fehler:#}"));
            lage.zuletzt = Some(zustand.uhr.jetzt());
        }
        let streuung = if konfig.streuung.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_secs(rand::random::<u64>() % konfig.streuung.as_secs().max(1))
        };
        let warte = konfig.abstand + streuung;
        let mut ende = zustand.ende_abo();
        tokio::select! {
            _ = ende.recv() => return,
            _ = tokio::time::sleep(warte) => {}
        }
    }
}

/// Die Adresse der Veröffentlichung, aus der Basis der Quelle.
///
/// Für GitHub wird die API gefragt (JSON statt HTML): `github.com/<owner>/<repo>`
/// wird zu `api.github.com/repos/<owner>/<repo>/releases/latest`. Eine
/// Loopback-Quelle (Tests) wird unverändert genommen, damit der Test einen
/// eigenen Server stellen kann.
pub fn api_adresse(quelle: &str) -> String {
    let basis = quelle.trim_end_matches('/');
    if let Some(rest) = basis.strip_prefix("https://github.com/") {
        return format!("https://api.github.com/repos/{rest}/releases/latest");
    }
    format!("{basis}/releases/latest")
}

/// Einmal fragen: Antwort holen, lesen, merken (im Speicher und in der Datenbank).
async fn pruefen(zustand: &Arc<Zustand>) -> anyhow::Result<()> {
    let url = api_adresse(&zustand.konfig.updates.quelle);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        // Ohne Kennung: kein Gerät, kein Benutzer, keine Instanz.
        .user_agent("quiver")
        .timeout(ABRUF_FRIST)
        .build()
        .unwrap_or_default();
    let antwort = client
        .get(&url)
        .send()
        .await
        .map_err(|fehler| anyhow::anyhow!("{url} nicht erreichbar: {fehler}"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("{url} antwortet mit {}", antwort.status());
    }
    let text = antwort
        .text()
        .await
        .map_err(|fehler| anyhow::anyhow!("{url} nicht lesbar: {fehler}"))?;
    if text.len() > ANTWORT_BYTES_MAX {
        anyhow::bail!("{url} ist zu groß ({} Bytes)", text.len());
    }
    let veroeffentlichung =
        updates::veroeffentlichung_lesen(&text).map_err(|fehler| anyhow::anyhow!("{fehler}"))?;
    let jetzt = zustand.uhr.jetzt();
    let zeile = {
        let mut lage = crate::sperre(&zustand.updates);
        tracing::info!(
            "Veröffentlichte Version gelesen: {}",
            veroeffentlichung.version
        );
        lage.veroeffentlichung = Some(veroeffentlichung);
        lage.zuletzt = Some(jetzt);
        lage.fehler = None;
        lage.als_zeile()
    };
    let gemerkt = zeile.clone();
    db(zustand, move |conn| {
        speicher::update_stand_setzen(conn, &gemerkt)
    })
    .await
    .map_err(|_| anyhow::anyhow!("der Stand der Veröffentlichung ließ sich nicht ablegen"))?;
    Ok(())
}

// ------------------------------------------------------------ Geräteseite

/// Abfrage des Geräts: die laufende Version (aus der Abfrage oder der letzten
/// Meldung).
#[derive(Debug, Deserialize)]
pub(crate) struct AngebotAbfrage {
    #[serde(default)]
    pub version: String,
}

/// `GET /api/v1/geraet/updates`: was der Server über die Veröffentlichung weiß.
pub(crate) async fn angebot(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Query(abfrage): Query<AngebotAbfrage>,
) -> Ergebnis<Json<UpdateAngebot>> {
    let kennung = id.clone();
    let gemeldet = db(&zustand, move |conn| {
        Ok(speicher::hole_geraet(conn, kennung.als_str())?.and_then(|zeile| zeile.meldung))
    })
    .await?;
    let aktuelle = abfrage_version(&abfrage.version, gemeldet.as_ref());
    let lage = crate::sperre(&zustand.updates);
    let (neueste_version, aenderungen) = lage
        .veroeffentlichung
        .as_ref()
        .map(|veroeffentlichung| {
            (
                Some(veroeffentlichung.version.clone()),
                veroeffentlichung.aenderungen.clone(),
            )
        })
        .unwrap_or((None, None));
    Ok(Json(UpdateAngebot {
        aktuelle_version: aktuelle,
        neueste_version,
        aenderungen,
    }))
}

/// Die Version aus der Abfrage, sonst die zuletzt gemeldete.
fn abfrage_version(aus_abfrage: &str, gemeldet: Option<&quiver_protocol::Meldung>) -> String {
    let aus_abfrage = aus_abfrage.trim();
    if !aus_abfrage.is_empty() {
        return aus_abfrage.chars().take(64).collect();
    }
    gemeldet
        .map(|meldung| meldung.client_version.clone())
        .unwrap_or_default()
}

// ----------------------------------------------------------- Verwaltung

/// Die Geräte, deren gemeldeter Client älter ist als `neueste`.
///
/// Reine Rechnung über dem, was die Geräte gemeldet haben: eine unlesbare
/// Version ist nie älter (sie erscheint also nicht als veraltet), und die Liste
/// ist bei [`VERALTETE_MAX`] gedeckelt.
pub fn veraltete_aus(
    neueste: &str,
    gemeldet: impl IntoIterator<Item = (GeraetId, String, String)>,
) -> Vec<VeralteterClient> {
    gemeldet
        .into_iter()
        .filter(|(_, _, version)| updates::version_neuer(neueste, version))
        .take(VERALTETE_MAX)
        .map(|(geraet, name, version)| VeralteterClient {
            geraet,
            name,
            version,
        })
        .collect()
}

/// `GET /api/v1/admin/updates`: Stand der Abfrage und was daraus folgt.
pub(crate) async fn stand(State(zustand): State<Arc<Zustand>>) -> Ergebnis<Json<UpdateStand>> {
    let server_version = env!("CARGO_PKG_VERSION").to_string();
    let konfig = &zustand.konfig.updates;
    let (veroeffentlichung, zuletzt, mut fehler) = {
        let lage = crate::sperre(&zustand.updates);
        (
            lage.veroeffentlichung.clone(),
            lage.zuletzt.map(hilfen::als_text),
            lage.fehler.clone(),
        )
    };
    if !konfig.an {
        fehler = Some(String::from(
            "Die Update-Prüfung ist aus (QUIVER_UPDATES=aus).",
        ));
    }
    let neueste_version = veroeffentlichung.as_ref().map(|v| v.version.clone());
    let server_veraltet = neueste_version
        .as_deref()
        .is_some_and(|neueste| updates::version_neuer(neueste, &server_version));
    let veraltete_clients = match neueste_version.clone() {
        Some(neueste) => {
            let gemeldet = db(&zustand, |conn| {
                Ok(speicher::alle_geraete(conn)?
                    .into_iter()
                    .filter_map(|zeile| {
                        let meldung = zeile.meldung?;
                        Some((zeile.geraet.id, zeile.geraet.name, meldung.client_version))
                    })
                    .collect::<Vec<_>>())
            })
            .await?;
            veraltete_aus(&neueste, gemeldet)
        }
        None => Vec::new(),
    };
    Ok(Json(UpdateStand {
        server_version,
        abfrage_an: konfig.an,
        zuletzt_geprueft: zuletzt,
        neueste_version,
        aenderungen: veroeffentlichung
            .as_ref()
            .and_then(|v| v.aenderungen.clone()),
        server_veraltet,
        veraltete_clients,
        fehler,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::UPDATE_QUELLE;

    #[test]
    fn die_github_quelle_wird_zur_api() {
        assert_eq!(
            api_adresse(UPDATE_QUELLE),
            "https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest"
        );
        // Loopback (Tests) bleibt, wie es ist.
        assert_eq!(
            api_adresse("http://127.0.0.1:8080/veroeffentlichung"),
            "http://127.0.0.1:8080/veroeffentlichung/releases/latest"
        );
        assert_eq!(
            api_adresse("https://github.com/patimweb-labs/mcp-quiver/"),
            "https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest"
        );
    }

    #[test]
    fn die_abfrage_geht_vor_der_gemeldeten_version() {
        let meldung = quiver_protocol::Meldung {
            revision: String::from("r"),
            client_version: String::from("0.1.0"),
            eintraege: Vec::new(),
        };
        assert_eq!(abfrage_version(" 0.2.0 ", Some(&meldung)), "0.2.0");
        assert_eq!(abfrage_version("", Some(&meldung)), "0.1.0");
        assert_eq!(abfrage_version("", None), "");
    }

    #[test]
    fn nur_aeltere_clients_zaehlen_als_veraltet() {
        let geraete = vec![
            (
                GeraetId::neu("alt"),
                String::from("Rechner"),
                String::from("0.2.0"),
            ),
            (
                GeraetId::neu("neu"),
                String::from("Notebook"),
                String::from("0.3.0"),
            ),
            (
                GeraetId::neu("alt2"),
                String::from("Tablet"),
                String::from("0.2.9"),
            ),
            (GeraetId::neu("leer"), String::from("Kiosk"), String::new()),
        ];
        let veraltet = veraltete_aus("v0.3.0", geraete);
        let namen: Vec<&str> = veraltet.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(namen, vec!["Rechner", "Tablet"]);
        // Gleiche Version ist nicht veraltet.
        assert!(veraltete_aus(
            "0.3.0",
            vec![(
                GeraetId::neu("gleich"),
                String::from("Rechner"),
                String::from("0.3.0")
            )]
        )
        .is_empty());
    }

    #[test]
    fn die_liste_der_veralteten_ist_gedeckelt() {
        let viele = (0..VERALTETE_MAX + 10).map(|i| {
            (
                GeraetId::neu(format!("g{i}")),
                String::from("Rechner"),
                String::from("0.1.0"),
            )
        });
        assert_eq!(veraltete_aus("0.3.0", viele).len(), VERALTETE_MAX);
    }

    #[test]
    fn der_gemerkte_stand_ueberlebt_einen_neustart() {
        let veroeffentlichung = Veroeffentlichung {
            version: String::from("0.3.0"),
            veroeffentlicht: None,
            aenderungen: Some(String::from("https://example.test/changes")),
        };
        let mut lage = Lage::neu();
        lage.aus_zeile(UpdateStandZeile {
            veroeffentlichung: Some(veroeffentlichung.clone()),
            zuletzt: Some(String::from("2026-03-01T10:00:00Z")),
        });
        assert_eq!(lage.veroeffentlichung, Some(veroeffentlichung));
        assert!(lage.zuletzt.is_some());
        // Und er lässt sich wieder als Zeile schreiben.
        let zeile = lage.als_zeile();
        assert_eq!(zeile.zuletzt.as_deref(), Some("2026-03-01T10:00:00Z"));
    }
}
