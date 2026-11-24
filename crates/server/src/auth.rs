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

fn fehlt() -> FehlerAntwort {
    FehlerAntwort::nicht_angemeldet("Token fehlt oder ist kein Bearer-Token")
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
    req.extensions_mut().insert(zeile.geraet.id);
    Ok(next.run(req).await)
}

/// Darf dieses Gerät noch? Die Antwort trägt bei Verweigerung einen
/// maschinenlesbaren Grund (`gesperrt`, `neu_anmelden`, `kein_zugriff`).
pub(crate) async fn geraet_pruefen(zustand: &Arc<Zustand>, zeile: &GeraetZeile) -> Ergebnis<()> {
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
}
