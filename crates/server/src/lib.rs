// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Server von mcp-quiver: Bibliothek, Zuweisung und Verteilung.
//!
//! Der Vertrag (Pfade, Rümpfe) steht in `quiver-protocol`, die Regeln (Soll
//! berechnen, Widersprüche, Prioritäten) in `quiver-domain`. Hier steht nur,
//! wie beides über HTTP und SQLite zusammenkommt.
//!
//! ```no_run
//! # async fn beispiel() -> anyhow::Result<()> {
//! use quiver_server::{app, Konfig};
//! use std::path::PathBuf;
//!
//! let router = app(Konfig {
//!     daten: PathBuf::from("./daten"),
//!     admin_token: "geheim".into(),
//!     admin_dist: None,
//! }).await?;
//! # Ok(())
//! # }
//! ```
//!
//! `main.rs` liest die Umgebung (`QUIVER_ADRESSE`, `QUIVER_DATEN`,
//! `QUIVER_ADMIN_TOKEN`, `QUIVER_ADMIN_DIST`), bindet und fährt bei Strg+C
//! sauber herunter.

mod admin;
mod auth;
mod fehler;
mod geraet;
mod hilfen;
mod speicher;
mod zustand;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::middleware;
use axum::routing::{get, post, put};
use axum::Router;
use quiver_domain::{Geraet, GeraetId, Soll};
use rusqlite::Connection;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use zustand::Zustand;

pub use fehler::{Ergebnis, FehlerAntwort};

/// Woher der Server seine Daten nimmt (aus der Umgebung, siehe `main.rs`).
#[derive(Debug, Clone)]
pub struct Konfig {
    /// Ordner für `quiver.db` und `admin-token`.
    pub daten: PathBuf,
    /// Admin-Token (Vergleich in konstanter Zeit).
    pub admin_token: String,
    /// Gebaute Admin-Oberfläche; fehlt sie, gibt es nur die API.
    pub admin_dist: Option<PathBuf>,
}

/// Alles aufbauen, was der Server braucht; der Aufrufer bindet selbst.
pub async fn app(konfig: Konfig) -> anyhow::Result<Router> {
    std::fs::create_dir_all(&konfig.daten)?;
    let conn = speicher::oeffnen(&konfig.daten.join("quiver.db"))?;
    let zustand = Arc::new(Zustand::neu(conn, konfig.admin_token));
    let dist = konfig.admin_dist.filter(|pfad| {
        let da = pfad.is_dir();
        if !da {
            tracing::warn!(
                "Admin-Oberfläche {} fehlt, es gibt nur die API",
                pfad.display()
            );
        }
        da
    });
    Ok(router(zustand, dist))
}

/// Der Admin-Token: aus der Umgebung, sonst aus `<daten>/admin-token`, sonst neu.
///
/// Das zweite Feld sagt, ob gerade einer erzeugt wurde – nur dann darf er
/// (einmal) ins Log.
pub fn admin_token_ermitteln(daten: &Path) -> anyhow::Result<(String, bool)> {
    if let Ok(token) = std::env::var("QUIVER_ADMIN_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok((token, false));
        }
    }
    std::fs::create_dir_all(daten)?;
    let pfad = daten.join("admin-token");
    if let Ok(text) = std::fs::read_to_string(&pfad) {
        let token = text.trim().to_string();
        if !token.is_empty() {
            return Ok((token, false));
        }
    }
    let token = hilfen::token_erzeugen();
    std::fs::write(&pfad, &token)?;
    nur_eigentuemer_darf_lesen(&pfad)?;
    Ok((token, true))
}

#[cfg(unix)]
fn nur_eigentuemer_darf_lesen(pfad: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(pfad, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn nur_eigentuemer_darf_lesen(_pfad: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn router(zustand: Arc<Zustand>, dist: Option<PathBuf>) -> Router {
    let geraete = Router::new()
        .route("/soll", get(geraet::soll))
        .route("/ereignisse", get(geraet::ereignisse))
        .route("/meldung", post(geraet::meldung))
        .method_not_allowed_fallback(methode_falsch)
        .route_layer(middleware::from_fn_with_state(
            zustand.clone(),
            auth::geraete_wache,
        ));

    let verwaltung = Router::new()
        .route("/eintraege", get(admin::eintraege_liste))
        .route("/eintraege/import", post(admin::eintrag_importieren))
        .route(
            "/eintraege/{id}",
            get(admin::eintrag_holen)
                .put(admin::eintrag_setzen)
                .delete(admin::eintrag_loeschen),
        )
        .route("/saetze", get(admin::saetze_liste))
        .route(
            "/saetze/{id}",
            get(admin::satz_holen)
                .put(admin::satz_setzen)
                .delete(admin::satz_loeschen),
        )
        .route("/gruppen", get(admin::gruppen_liste))
        .route(
            "/gruppen/{id}",
            get(admin::gruppe_holen)
                .put(admin::gruppe_setzen)
                .delete(admin::gruppe_loeschen),
        )
        .route("/geraete", get(admin::geraete_liste))
        .route(
            "/geraete/{id}",
            put(admin::geraet_setzen).delete(admin::geraet_loeschen),
        )
        .route("/codes", post(admin::code_anlegen))
        .route("/ueberblick", get(admin::ueberblick))
        .method_not_allowed_fallback(methode_falsch)
        .route_layer(middleware::from_fn_with_state(
            zustand.clone(),
            auth::admin_wache,
        ));

    let api = Router::new()
        .route("/v1/anmelden", post(geraet::anmelden))
        .nest("/v1/geraet", geraete)
        .nest("/v1/admin", verwaltung)
        .fallback(api_unbekannt)
        .method_not_allowed_fallback(methode_falsch);

    let app = Router::new()
        .nest("/api", api)
        .layer(TraceLayer::new_for_http())
        .with_state(zustand);

    match dist {
        // Alles, was nicht zur API gehört, ist die Admin-Oberfläche (SPA):
        // unbekannte Pfade liefern `index.html` mit 200, die App routet selbst.
        Some(pfad) => app.fallback_service(
            ServeDir::new(pfad.clone()).fallback(ServeFile::new(pfad.join("index.html"))),
        ),
        None => app.fallback(api_unbekannt),
    }
}

async fn api_unbekannt() -> FehlerAntwort {
    FehlerAntwort::nicht_gefunden("diesen Pfad gibt es nicht")
}

/// Auch eine falsche Methode antwortet im Vertrag, nicht mit leerem Rumpf.
async fn methode_falsch() -> FehlerAntwort {
    FehlerAntwort::neu(
        axum::http::StatusCode::METHOD_NOT_ALLOWED,
        "diese Methode ist an diesem Pfad nicht vorgesehen",
    )
}

/// Datenbankarbeit aus dem async-Code: auf einer blockierenden Aufgabe, damit
/// die Verbindung nie über ein `.await` gehalten wird.
pub(crate) async fn db<T, F>(zustand: &Arc<Zustand>, arbeit: F) -> Ergebnis<T>
where
    F: FnOnce(&mut Connection) -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    let zustand = zustand.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = sperre(&zustand.db);
        arbeit(&mut conn).map_err(|fehler| FehlerAntwort::aus_fehler(&fehler))
    })
    .await
    .map_err(|fehler| FehlerAntwort::intern(format!("Hintergrundaufgabe gescheitert: {fehler}")))?
}

/// Sperre holen, auch wenn ein anderer Aufruf panisch war.
pub(crate) fn sperre<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
}

/// Der Soll-Zustand eines Geräts aus dem aktuellen Stand (Regeln in `domain`).
pub(crate) fn soll_aus(bestand: &speicher::Bestand, geraet: &Geraet) -> anyhow::Result<Soll> {
    quiver_domain::soll_berechnen(
        geraet,
        &bestand.gruppen,
        &bestand.saetze,
        &bestand.eintraege,
    )
    .map_err(|fehler| anyhow::anyhow!(fehler))
}

/// Soll eines Geräts aus der Datenbank; Fehler daraus werden zu 500 mit Text.
pub(crate) async fn soll_fuer(zustand: &Arc<Zustand>, id: &GeraetId) -> Ergebnis<Soll> {
    let kennung = id.clone();
    db(zustand, move |conn| {
        let zeile = speicher::hole_geraet(conn, kennung.als_str())?
            .ok_or_else(|| anyhow::anyhow!("Gerät {kennung} gibt es nicht"))?;
        let bestand = speicher::bestand(conn)?;
        soll_aus(&bestand, &zeile.geraet)
    })
    .await
}
