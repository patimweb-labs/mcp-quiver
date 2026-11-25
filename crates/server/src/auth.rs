// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wer darf was: Geräte-Pfade mit dem Geräte-Token (in der Datenbank steht nur
//! sein Hash). Die Verwaltung wird über Sitzungen geprüft (`sitzung.rs`).

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use quiver_protocol::AnmeldeGrund;
use time::OffsetDateTime;

use crate::db;
use crate::fehler::{Ergebnis, FehlerAntwort};
use crate::hilfen;
use crate::speicher::{self, GeraetZeile};
use crate::zustand::Zustand;

/// `Authorization: Bearer <token>` auslesen.
fn bearer(req: &Request) -> Option<String> {
    let wert = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let (art, token) = wert.split_once(' ')?;
    if !art.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// So lange darf ein Geräte-Token ungenutzt bleiben (Härtung, Punkt 11).
pub const TOKEN_UNBENUTZT_TAGE: i64 = 60;
/// So alt darf ein Geräte-Token höchstens werden.
pub const TOKEN_ALTER_TAGE: i64 = 365;

fn fehlt() -> FehlerAntwort {
    FehlerAntwort::nicht_angemeldet("Token fehlt oder ist kein Bearer-Token")
}

/// Ist das Token zu alt oder zu lange ungenutzt? Dann muss sich das Gerät neu
/// anmelden.
pub(crate) fn token_abgelaufen(zeile: &GeraetZeile, jetzt: OffsetDateTime) -> bool {
    let zu_alt = zeile
        .token_erstellt
        .as_deref()
        .and_then(hilfen::zeit_lesen)
        .is_some_and(|erstellt| jetzt - erstellt > time::Duration::days(TOKEN_ALTER_TAGE));
    let ungenutzt = zeile
        .token_zuletzt
        .as_deref()
        .and_then(hilfen::zeit_lesen)
        .is_some_and(|zuletzt| jetzt - zuletzt > time::Duration::days(TOKEN_UNBENUTZT_TAGE));
    zu_alt || ungenutzt
}

/// Geräte-Pfade: sha256(Token) muss zu einem Gerät passen.
pub(crate) async fn geraete_wache(
    State(zustand): State<Arc<Zustand>>,
    mut req: Request,
    next: Next,
) -> Ergebnis<Response> {
    let token = bearer(&req).ok_or_else(fehlt)?;
    let hash = hilfen::hash(&token);
    let zeile = db(&zustand, move |conn| speicher::geraet_per_hash(conn, &hash))
        .await?
        .ok_or_else(|| FehlerAntwort::nicht_angemeldet("Token gehört zu keinem Gerät"))?;
    geraet_pruefen(&zustand, &zeile).await?;
    // Die letzte Nutzung nachziehen (höchstens stündlich geschrieben).
    let kennung = zeile.geraet.id.clone();
    let jetzt = zustand.uhr.jetzt();
    db(&zustand, move |conn| {
        speicher::geraet_token_nutzen(conn, kennung.als_str(), jetzt)
    })
    .await?;
    req.extensions_mut().insert(zeile.geraet.id);
    Ok(next.run(req).await)
}

/// Darf dieses Gerät noch? Die Antwort trägt bei Verweigerung einen
/// maschinenlesbaren Grund (`gesperrt`, `neu_anmelden`, `kein_zugriff`).
pub(crate) async fn geraet_pruefen(zustand: &Arc<Zustand>, zeile: &GeraetZeile) -> Ergebnis<()> {
    // Alter und letzte Nutzung des Tokens: beides ist eine harte Grenze, nach
    // der sich das Gerät neu anmelden muss (Härtung, Punkt 11).
    if token_abgelaufen(zeile, zustand.uhr.jetzt()) {
        return Err(FehlerAntwort::mit_grund(
            axum::http::StatusCode::FORBIDDEN,
            "das Geräte-Token ist zu alt oder zu lange ungenutzt; bitte neu anmelden",
            AnmeldeGrund::NeuAnmelden,
        ));
    }
    if zeile.geraet.gesperrt {
        return Err(FehlerAntwort::mit_grund(
            axum::http::StatusCode::FORBIDDEN,
            format!("Gerät {} ist gesperrt", zeile.geraet.id),
            AnmeldeGrund::Gesperrt,
        ));
    }
    let Some(oid) = zeile.oid.clone() else {
        // Altgerät aus der Zeit vor Entra.
        if !zustand.konfig.altgeraete_erlaubt {
            return Err(FehlerAntwort::mit_grund(
                axum::http::StatusCode::FORBIDDEN,
                "Altgerät ohne Benutzer; bitte neu anmelden",
                AnmeldeGrund::NeuAnmelden,
            ));
        }
        return Ok(());
    };
    let manuell = zeile.manuell.clone();
    let zeile_geraet = zeile.geraet.clone();
    let zeile_tid = zeile.tid.clone();
    let stand = db(zustand, move |conn| {
        let Some(benutzer) = speicher::benutzer_holen(conn, &oid)? else {
            return Ok(Stand::NeuAnmelden);
        };
        if benutzer.gesperrt {
            return Ok(Stand::Gesperrt);
        }
        if let Some(tid) = zeile_tid.as_deref() {
            if tid != benutzer.tid {
                return Ok(Stand::NeuAnmelden);
            }
        }
        if benutzer.token_stand != "gueltig" {
            return Ok(Stand::NeuAnmelden);
        }
        let wirksam = speicher::wirksame_gruppen(conn, &manuell, &benutzer.entra)?;
        if wirksam.is_empty() {
            return Ok(Stand::KeinZugriff);
        }
        if wirksam != zeile_geraet.gruppen {
            let mut geraet = zeile_geraet;
            geraet.gruppen = wirksam;
            speicher::aendere_geraet(conn, &geraet)?;
        }
        Ok(Stand::Erlaubt)
    })
    .await?;
    match stand {
        Stand::Erlaubt => Ok(()),
        Stand::Gesperrt => Err(FehlerAntwort::mit_grund(
            axum::http::StatusCode::FORBIDDEN,
            "Benutzer ist gesperrt",
            AnmeldeGrund::Gesperrt,
        )),
        Stand::NeuAnmelden => Err(FehlerAntwort::mit_grund(
            axum::http::StatusCode::FORBIDDEN,
            "Gerät muss sich neu anmelden",
            AnmeldeGrund::NeuAnmelden,
        )),
        Stand::KeinZugriff => Err(FehlerAntwort::mit_grund(
            axum::http::StatusCode::FORBIDDEN,
            "dem Benutzer ist keine Gruppe zugeordnet",
            AnmeldeGrund::KeinZugriff,
        )),
    }
}

enum Stand {
    Erlaubt,
    Gesperrt,
    NeuAnmelden,
    KeinZugriff,
}

/// Kurzform für den Ereignis-Strom: endet der Strom?
pub(crate) async fn geraet_erlaubt(zustand: &Arc<Zustand>, id: &quiver_domain::GeraetId) -> bool {
    let kennung = id.clone();
    let zeile = match db(zustand, move |conn| {
        speicher::hole_geraet(conn, kennung.als_str())
    })
    .await
    {
        Ok(Some(zeile)) => zeile,
        Ok(None) => return false,
        // Datenbank hakt: lieber offen lassen als grundlos trennen.
        Err(_) => return true,
    };
    geraet_pruefen(zustand, &zeile).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;

    fn anfrage(kopf: Option<&str>) -> Request {
        let mut bauer = HttpRequest::builder().uri("/api/v1/geraet/soll");
        if let Some(kopf) = kopf {
            bauer = bauer.header(header::AUTHORIZATION, kopf);
        }
        bauer.body(Body::empty()).unwrap()
    }

    #[test]
    fn bearer_wird_gelesen() {
        assert_eq!(bearer(&anfrage(Some("Bearer abc"))).as_deref(), Some("abc"));
        assert_eq!(
            bearer(&anfrage(Some("bearer  abc "))).as_deref(),
            Some("abc")
        );
        assert_eq!(bearer(&anfrage(Some("Basic abc"))), None);
        assert_eq!(bearer(&anfrage(Some("Bearer "))), None);
        assert_eq!(bearer(&anfrage(None)), None);
    }

    fn geraet_mit(erstellt: Option<&str>, zuletzt: Option<&str>) -> GeraetZeile {
        GeraetZeile {
            geraet: quiver_domain::Geraet {
                id: quiver_domain::GeraetId::neu("d1"),
                name: String::from("Gerät"),
                gruppen: Vec::new(),
                gesperrt: false,
            },
            zuletzt_gesehen: None,
            meldung: None,
            oid: None,
            tid: None,
            manuell: Vec::new(),
            token_erstellt: erstellt.map(String::from),
            token_zuletzt: zuletzt.map(String::from),
        }
    }

    #[test]
    fn zu_altes_oder_ungenutztes_token_verlangt_neue_anmeldung() {
        let jetzt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let frisch = hilfen::als_text(jetzt - time::Duration::days(10));
        assert!(!token_abgelaufen(
            &geraet_mit(Some(&frisch), Some(&frisch)),
            jetzt
        ));
        // 60 Tage ungenutzt: neu anmelden.
        let alt = hilfen::als_text(jetzt - time::Duration::days(TOKEN_UNBENUTZT_TAGE + 1));
        assert!(token_abgelaufen(
            &geraet_mit(Some(&frisch), Some(&alt)),
            jetzt
        ));
        // 365 Tage alt: neu anmelden, auch bei frischer Nutzung.
        let alt = hilfen::als_text(jetzt - time::Duration::days(TOKEN_ALTER_TAGE + 1));
        assert!(token_abgelaufen(
            &geraet_mit(Some(&alt), Some(&frisch)),
            jetzt
        ));
        // Ohne Angaben (Altbestand) gilt nichts als abgelaufen.
        assert!(!token_abgelaufen(&geraet_mit(None, None), jetzt));
    }
}
