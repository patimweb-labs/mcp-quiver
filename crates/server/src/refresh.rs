// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Nachprüfen statt einmal glauben (ADR 0002): das Refresh-Token jedes
//! Benutzers wird stündlich eingelöst, das neue ID-Token bestimmt die Gruppen.
//!
//! Regeln aus Aufgabe 20:
//! - Rotation wird **sofort** gespeichert.
//! - `invalid_grant`, `interaction_required` oder eine fehlende Zuweisung →
//!   `neu_anmelden`.
//! - Netz- und 5xx-Fehler → letzter Stand, höchstens 24 Stunden.
//! - Kein gleichzeitiges Einlösen je Benutzer.
//! - Ein neues ID-Token muss dieselbe `(tid, oid)` haben.

use std::sync::Arc;
use std::time::Duration;

use quiver_protocol::AnmeldeGrund;
use time::OffsetDateTime;

use crate::db;
use crate::krypto;
use crate::oidc::{Erwartet, TokenFehler};
use crate::speicher;
use crate::zustand::Zustand;

/// Nach dieser Zeit wird beim Verbinden eines Geräts nachgeprüft.
pub const BEI_VERBINDUNG_MINUTEN: i64 = 60;
/// Nach dieser Zeit ohne erfolgreiche Prüfung gilt die Anmeldung nicht mehr.
pub const HOECHSTENS_STUNDEN: i64 = 24;
/// Streuung, damit nicht alle Benutzer zur selben Sekunde prüfen.
pub const STREUUNG_SEKUNDEN: i64 = 600;

/// Prüft beim Verbinden eines Geräts, aber nur, wenn die letzte Prüfung länger
/// als 60 Minuten her ist.
pub async fn bei_verbindung(zustand: &Arc<Zustand>, oid: &str) -> Result<(), AnmeldeGrund> {
    let jetzt = zustand.uhr.jetzt();
    let kennung = oid.to_string();
    let frist = db(zustand, move |conn| {
        let Some(benutzer) = speicher::benutzer_holen(conn, &kennung)? else {
            return Ok(None);
        };
        Ok(Some(benutzer))
    })
    .await
    .map_err(|_| AnmeldeGrund::NeuAnmelden)?;
    let Some(benutzer) = frist else {
        return Err(AnmeldeGrund::NeuAnmelden);
    };
    if benutzer.gesperrt {
        return Err(AnmeldeGrund::Gesperrt);
    }
    if benutzer.token_stand != "gueltig" {
        return Err(AnmeldeGrund::NeuAnmelden);
    }
    let faellig = benutzer
        .token_erneuert
        .as_deref()
        .and_then(crate::hilfen::zeit_lesen)
        .is_none_or(|zuletzt| jetzt - zuletzt > time::Duration::minutes(BEI_VERBINDUNG_MINUTEN));
    if !faellig {
        return Ok(());
    }
    pruefen(zustand, oid, jetzt).await
}

/// Auf Knopf (Verwaltung) oder aus dem Hintergrund: sofort prüfen.
pub async fn jetzt_pruefen(zustand: &Arc<Zustand>, oid: &str) -> Result<(), AnmeldeGrund> {
    pruefen(zustand, oid, zustand.uhr.jetzt()).await
}

/// Eine Prüfung, mit Sperre je Benutzer.
async fn pruefen(
    zustand: &Arc<Zustand>,
    oid: &str,
    jetzt: OffsetDateTime,
) -> Result<(), AnmeldeGrund> {
    let Some(oidc) = zustand.oidc.clone() else {
        return Ok(());
    };
    let sperre = zustand.refresh_sperre(oid);
    let _wache = sperre.lock().await;

    let kennung = oid.to_string();
    let benutzer = db(zustand, move |conn| {
        speicher::benutzer_holen(conn, &kennung)
    })
    .await
    .map_err(|_| AnmeldeGrund::NeuAnmelden)?;
    let Some(benutzer) = benutzer else {
        return Err(AnmeldeGrund::NeuAnmelden);
    };
    if benutzer.gesperrt {
        return Err(AnmeldeGrund::Gesperrt);
    }
    if benutzer.token_stand != "gueltig" {
        return Err(AnmeldeGrund::NeuAnmelden);
    }
    let Some(verschluesselt) = benutzer.token.clone() else {
        neu_anmelden(zustand, oid, jetzt, "kein Refresh-Token").await?;
        return Err(AnmeldeGrund::NeuAnmelden);
    };
    let aad = krypto::aad(&benutzer.tid, &benutzer.oid);
    let refresh = match krypto::entschluesseln(&zustand.schluessel, &aad, &verschluesselt) {
        Ok(klar) => match String::from_utf8(klar) {
            Ok(text) => text,
            Err(_) => {
                neu_anmelden(zustand, oid, jetzt, "Refresh-Token unlesbar").await?;
                return Err(AnmeldeGrund::NeuAnmelden);
            }
        },
        Err(_) => {
            neu_anmelden(zustand, oid, jetzt, "Refresh-Token nicht entschlüsselbar").await?;
            return Err(AnmeldeGrund::NeuAnmelden);
        }
    };

    let erwartet = Erwartet {
        issuer: oidc.konfig.issuer.clone(),
        audience: oidc.konfig.client_id.clone(),
        tenant: benutzer.tid.clone(),
        nonce: String::new(),
    };
    let ergebnis = oidc.erneuern(&refresh, &erwartet, jetzt).await;
    let (identitaet, tokens) = match ergebnis {
        Ok(werte) => werte,
        Err(TokenFehler::Oauth(code)) if code == "interaction_required" => {
            neu_anmelden(zustand, oid, jetzt, "Entra verlangt eine neue Anmeldung").await?;
            return Err(AnmeldeGrund::NeuAnmelden);
        }
        Err(fehler) if fehler.neu_anmelden() => {
            neu_anmelden(zustand, oid, jetzt, "Refresh-Token ungültig").await?;
            return Err(AnmeldeGrund::NeuAnmelden);
        }
        Err(TokenFehler::Oauth(code)) => {
            // Zuweisung entzogen o. Ä. sieht Entra als OAuth-Fehler.
            return netzfehler(zustand, oid, jetzt, &format!("OAuth {code}"), &benutzer).await;
        }
        Err(TokenFehler::Netz(text)) => {
            return netzfehler(zustand, oid, jetzt, &text, &benutzer).await;
        }
    };

    // Ein neues ID-Token muss dieselbe Identität haben.
    if identitaet.oid != benutzer.oid || identitaet.tid != benutzer.tid {
        neu_anmelden(
            zustand,
            oid,
            jetzt,
            "ID-Token gehört zu einer anderen Identität",
        )
        .await?;
        return Err(AnmeldeGrund::NeuAnmelden);
    }
    if identitaet.gruppen.is_empty() {
        // Ohne zugeordnete Gruppe keine Tools (ADR 0002, Punkt 8).
        db(zustand, {
            let oid = oid.to_string();
            let identitaet = identitaet.clone();
            move |conn| {
                speicher::benutzer_gruppen(
                    conn,
                    &oid,
                    &identitaet.gruppen,
                    identitaet.email.as_deref(),
                    identitaet.name.as_deref(),
                    jetzt,
                )?;
                speicher::entra_gruppen_merken(conn, &identitaet.gruppen, jetzt)?;
                Ok(())
            }
        })
        .await
        .map_err(|_| AnmeldeGrund::NeuAnmelden)?;
        neu_anmelden(zustand, oid, jetzt, "keine Gruppe mehr zugeordnet").await?;
        return Err(AnmeldeGrund::KeinZugriff);
    }

    let neues_token = match tokens.refresh_token.as_deref() {
        Some(refresh) => krypto::verschluesseln(&zustand.schluessel, &aad, refresh.as_bytes())
            .map_err(|_| AnmeldeGrund::NeuAnmelden)?,
        // Rotation ohne neues Token: das alte bleibt gültig, bis es scheitert.
        None => verschluesselt,
    };

    let identitaet_fuer_db = identitaet.clone();
    db(zustand, {
        let oid = oid.to_string();
        move |conn| {
            speicher::benutzer_gruppen(
                conn,
                &oid,
                &identitaet_fuer_db.gruppen,
                identitaet_fuer_db.email.as_deref(),
                identitaet_fuer_db.name.as_deref(),
                jetzt,
            )?;
            speicher::benutzer_token(
                conn,
                &oid,
                Some(neues_token.as_str()),
                Some(jetzt),
                "gueltig",
            )?;
            speicher::entra_gruppen_merken(conn, &identitaet_fuer_db.gruppen, jetzt)?;
            // Geräte des Benutzers neu rechnen: Zuordnung kann sich geändert haben.
            let geraete = speicher::geraete_von_user(conn, &oid)?;
            for zeile in geraete {
                let wirksam =
                    speicher::wirksame_gruppen(conn, &zeile.manuell, &identitaet_fuer_db.gruppen)?;
                if wirksam != zeile.geraet.gruppen {
                    let mut geraet = zeile.geraet;
                    geraet.gruppen = wirksam;
                    speicher::aendere_geraet(conn, &geraet)?;
                }
            }
            speicher::pruefspur_schreiben(
                conn,
                jetzt,
                "nachpruefen",
                identitaet_fuer_db
                    .email
                    .as_deref()
                    .unwrap_or(&identitaet_fuer_db.oid),
                &format!("Gruppen: {}", identitaet_fuer_db.gruppen.len()),
            )?;
            Ok(())
        }
    })
    .await
    .map_err(|_| AnmeldeGrund::NeuAnmelden)?;

    zustand.melde_aenderung();
    Ok(())
}

/// Netz-/5xx-Fehler: letzten Stand behalten, aber höchstens 24 Stunden.
async fn netzfehler(
    zustand: &Arc<Zustand>,
    oid: &str,
    jetzt: OffsetDateTime,
    text: &str,
    benutzer: &speicher::Benutzer,
) -> Result<(), AnmeldeGrund> {
    let zuletzt = benutzer
        .token_erneuert
        .as_deref()
        .and_then(crate::hilfen::zeit_lesen);
    let zu_alt =
        zuletzt.is_none_or(|zeit| jetzt - zeit > time::Duration::hours(HOECHSTENS_STUNDEN));
    if zu_alt {
        neu_anmelden(zustand, oid, jetzt, "länger als 24 Stunden nicht geprüft").await?;
        return Err(AnmeldeGrund::NeuAnmelden);
    }
    tracing::warn!("Nachprüfen für {} gescheitert (Netz): {}", oid, text);
    // Der letzte Stand bleibt; kein Fehler für das Gerät.
    Ok(())
}

/// Benutzer auf `neu_anmelden` setzen: Token weg, Prüfspur, Geräte werden beim
/// nächsten Zugriff abgewiesen.
async fn neu_anmelden(
    zustand: &Arc<Zustand>,
    oid: &str,
    jetzt: OffsetDateTime,
    grund: &str,
) -> Result<(), AnmeldeGrund> {
    let oid = oid.to_string();
    let grund = grund.to_string();
    db(zustand, move |conn| {
        speicher::benutzer_token(conn, &oid, None, None, "neu_anmelden")?;
        speicher::pruefspur_schreiben(
            conn,
            jetzt,
            "nachpruefen",
            &oid,
            &format!("neu anmelden: {grund}"),
        )?;
        Ok(())
    })
    .await
    .map_err(|_| AnmeldeGrund::NeuAnmelden)?;
    zustand.melde_aenderung();
    Ok(())
}

/// Ist eine Prüfung fällig? Stündlich, mit Streuung je Benutzer.
pub fn faellig(benutzer: &speicher::Benutzer, jetzt: OffsetDateTime) -> bool {
    let streuung = streuung(&benutzer.oid);
    let zuletzt = benutzer
        .token_erneuert
        .as_deref()
        .and_then(crate::hilfen::zeit_lesen);
    match zuletzt {
        None => true,
        Some(zeit) => jetzt - zeit > time::Duration::hours(1) + time::Duration::seconds(streuung),
    }
}

/// 0 .. 600 Sekunden, stabil je Benutzer.
pub fn streuung(oid: &str) -> i64 {
    let hash = crate::hilfen::hash(oid);
    i64::from(u8::from_str_radix(&hash[..2], 16).unwrap_or(0)) * STREUUNG_SEKUNDEN / 255
}

/// Alle fälligen Benutzer prüfen (vom Hintergrundlauf aufgerufen).
pub async fn faellige_pruefen(zustand: &Arc<Zustand>) -> usize {
    let jetzt = zustand.uhr.jetzt();
    let kandidaten = match db(zustand, move |conn| {
        Ok(speicher::benutzer_liste(conn)?
            .into_iter()
            .filter(|b| b.rolle != quiver_protocol::Rolle::Root && faellig(b, jetzt))
            .map(|b| b.oid)
            .collect::<Vec<_>>())
    })
    .await
    {
        Ok(liste) => liste,
        Err(fehler) => {
            tracing::warn!("Nachprüfen: Benutzer nicht lesbar: {}", fehler.text);
            return 0;
        }
    };
    let mut anzahl = 0;
    for oid in kandidaten {
        if jetzt_pruefen(zustand, &oid).await.is_ok() {
            anzahl += 1;
        }
    }
    anzahl
}

/// Der Hintergrundlauf: alle 60 Sekunden nachsehen, wer fällig ist.
pub async fn hintergrund(zustand: Arc<Zustand>) {
    let mut takt = tokio::time::interval(Duration::from_secs(60));
    takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        takt.tick().await;
        let anzahl = faellige_pruefen(&zustand).await;
        if anzahl > 0 {
            tracing::info!("Nachprüfen: {anzahl} Benutzer aufgefrischt");
        }
        zustand.drossel.aufraeumen(zustand.uhr.jetzt());
        {
            let jetzt = zustand.uhr.jetzt();
            let conn = crate::sperre(&zustand.db);
            let _ = speicher::sitzungen_aufraeumen(&conn, jetzt);
            let _ = speicher::oidc_aufraeumen(&conn, jetzt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streuung_ist_stabil_und_begrenzt() {
        let a = streuung("11111111-2222-3333-4444-555555555555");
        assert_eq!(a, streuung("11111111-2222-3333-4444-555555555555"));
        assert!((0..=STREUUNG_SEKUNDEN).contains(&a));
    }

    #[test]
    fn faelligkeit_haengt_an_der_zeit() {
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let mut benutzer = speicher::Benutzer {
            oid: "11111111-2222-3333-4444-555555555555".into(),
            tid: "tid".into(),
            email: None,
            name: None,
            rolle: quiver_protocol::Rolle::Benutzer,
            gesperrt: false,
            entra: Vec::new(),
            token_stand: "gueltig".into(),
            token_erneuert: Some(crate::hilfen::als_text(jetzt)),
            erstellt: crate::hilfen::als_text(jetzt),
            zuletzt: None,
            token: Some("x".into()),
        };
        assert!(!faellig(&benutzer, jetzt));
        assert!(faellig(&benutzer, jetzt + time::Duration::hours(2)));
        benutzer.token_erneuert = None;
        assert!(faellig(&benutzer, jetzt));
    }
}
