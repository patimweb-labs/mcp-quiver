// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wer darf was: Admin-Pfade mit dem Admin-Token, Geräte-Pfade mit dem
//! Geräte-Token (in der Datenbank steht nur sein Hash).

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

use crate::db;
use crate::fehler::{Ergebnis, FehlerAntwort};
use crate::hilfen;
use crate::speicher;
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

/// Admin-Pfade: Vergleich mit dem Admin-Token in konstanter Zeit.
pub(crate) async fn admin_wache(
    State(zustand): State<Arc<Zustand>>,
    req: Request,
    next: Next,
) -> Ergebnis<Response> {
    let token = bearer(&req).ok_or_else(fehlt)?;
    if !hilfen::gleich_konstant(&token, &zustand.admin_token) {
        return Err(FehlerAntwort::nicht_angemeldet("Token passt nicht"));
    }
    Ok(next.run(req).await)
}

/// Geräte-Pfade: sha256(Token) muss zu einem Gerät passen, gesperrte Geräte 403.
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
    if zeile.geraet.gesperrt {
        return Err(FehlerAntwort::verboten(format!(
            "Gerät {} ist gesperrt",
            zeile.geraet.id
        )));
    }
    req.extensions_mut().insert(zeile.geraet.id);
    Ok(next.run(req).await)
}
