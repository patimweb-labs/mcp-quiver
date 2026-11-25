// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Server von mcp-quiver: Bibliothek, Zuweisung, Verteilung, Anmeldung.
//!
//! Der Vertrag (Pfade, Rümpfe) steht in `quiver-protocol`, die Regeln (Soll
//! berechnen, Widersprüche, Prioritäten) in `quiver-domain`. Hier steht nur,
//! wie beides über HTTP und SQLite zusammenkommt.
//!
//! Seit Aufgabe 20 gibt es zwei Wege:
//! - **Entra** (ADR 0002): der Server ist der einzige OIDC-Client, Geräte
//!   bekommen nach der Bestätigung im Browser einen Einmal-Code.
//! - **Einmal-Codes** ohne Entra (MVP): Admin erzeugt Codes in der Verwaltung.
//!
//! Die Verwaltung meldet sich mit einer Sitzung an (Root per Passwort oder ein
//! Entra-Benutzer mit Rolle `admin`). Einen Admin-Schlüssel gibt es nicht mehr.
//!
//! ```no_run
//! # async fn beispiel() -> anyhow::Result<()> {
//! use quiver_server::{app, Konfig};
//!
//! let router = app(Konfig::neu("./daten")).await?;
//! # Ok(())
//! # }
//! ```

mod admin;
mod anmeldung;
mod auth;
pub mod benutzer;
pub mod betrieb;
mod config;
mod drossel;
mod einrichtung;
pub mod entra_schluessel;
mod fehler;
mod geraet;
mod graph;
mod grenzen;
mod hilfen;
mod host;
mod kopfzeilen;
mod krypto;
mod oidc;
mod programme;
pub mod rechte;
mod refresh;
mod seiten;
mod sitzung;
mod speicher;
pub mod spur;
pub mod unterbefehl;
mod zuordnung;
mod zustand;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post, put};
use axum::Router;
use quiver_domain::{Geraet, GeraetId, Soll};
use rusqlite::Connection;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::timeout::RequestBodyTimeoutLayer;
use tower_http::trace::TraceLayer;

use zustand::Zustand;

pub use config::{Ausweis, EntraKonfig, Konfig, Uhr, GRAPH_BASIS, GRAPH_SCOPE};
pub use entra_schluessel::{EntraSchluessel, ZertifikatStand};
pub use fehler::{Ergebnis, FehlerAntwort};
pub use host::GESUND;
pub use krypto::Schluessel;
pub use zustand::{STROEME_GESAMT, STROEME_JE_GERAET};

/// Rumpfgrenze für alle Wege außer Einträgen.
pub const RUMPF_STANDARD: usize = 64 * 1024;
/// Rumpfgrenze für Einträge: `server.json` ist größer als 64 KB.
pub const RUMPF_EINTRAEGE: usize = 256 * 1024;
/// So lange darf ein Client an einem Rumpf senden.
pub const RUMPF_ZEIT: Duration = Duration::from_secs(30);

/// Alles aufbauen, was der Server braucht; der Aufrufer bindet selbst.
///
/// Startet **keine** Hintergrundläufe – das macht [`app_mit_hintergrund`],
/// damit Tests nicht nebenbei Netzaufrufe auslösen.
pub async fn app(konfig: Konfig) -> anyhow::Result<Router> {
    let (router, _) = bauen(konfig).await?;
    Ok(router)
}

/// Wie [`app`], plus stündliche Nachprüfung und Beobachtung der Datenbank
/// (`PRAGMA data_version`, höchstens 2 s Verzug).
pub async fn app_mit_hintergrund(konfig: Konfig) -> anyhow::Result<Router> {
    let (router, _) = app_mit_hintergrund_und_abschalter(konfig).await?;
    Ok(router)
}

/// Wie [`app_mit_hintergrund`], plus [`Abschalter`] für ein geordnetes Ende.
///
/// Der Abschalter beendet die Ereignis-Ströme sofort und schreibt beim
/// Herunterfahren das WAL in die Datenbank zurück (siehe [`Abschalter::abschluss`]).
pub async fn app_mit_hintergrund_und_abschalter(
    konfig: Konfig,
) -> anyhow::Result<(Router, Abschalter)> {
    let (router, zustand) = bauen(konfig).await?;
    let aufgaben = vec![
        tokio::spawn(refresh::hintergrund(zustand.clone())),
        tokio::spawn(beobachte_datenbank(zustand.clone())),
    ];
    Ok((router, Abschalter { zustand, aufgaben }))
}

/// Das geordnete Ende des Servers.
///
/// `ende_jetzt` beendet alle offenen Ereignis-Ströme (sie sollen nicht auf ein
/// Zeitlimit warten müssen). `abschluss` beendet die Hintergrundläufe und
/// schreibt das WAL zurück.
pub struct Abschalter {
    zustand: Arc<Zustand>,
    aufgaben: Vec<tokio::task::JoinHandle<()>>,
}

impl Abschalter {
    /// Alle Ereignis-Ströme sofort beenden (SIGTERM, Strg+C).
    pub fn ende_jetzt(&self) {
        self.zustand.beenden();
    }

    /// Nach dem letzten Aufruf: Hintergrundläufe anhalten und WAL zurückführen.
    pub async fn abschluss(mut self) {
        for aufgabe in self.aufgaben.drain(..) {
            aufgabe.abort();
        }
        // `abort` wirkt beim nächsten Haltepunkt; aufräumen dauert nicht lang.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let conn = sperre(&self.zustand.db);
        match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |zeile| {
            Ok((zeile.get::<_, i64>(0)?, zeile.get::<_, i64>(1)?))
        }) {
            Ok((0, _)) => tracing::info!("WAL in die Datenbank zurückgeschrieben"),
            Ok((_, seiten)) => {
                tracing::warn!("WAL-Checkpoint unvollständig ({seiten} Seiten offen)")
            }
            Err(fehler) => tracing::warn!("WAL-Checkpoint nicht möglich: {fehler}"),
        }
    }
}

async fn bauen(konfig: Konfig) -> anyhow::Result<(Router, Arc<Zustand>)> {
    rechte::ordner_anlegen(&konfig.daten)?;
    let conn = speicher::oeffnen(&konfig.daten.join("quiver.db"))?;
    let schluessel = Schluessel::laden(&konfig.daten)?;
    let oidc = match konfig.entra.clone() {
        Some(entra) => Some(Arc::new(oidc::Oidc::neu(entra)?)),
        None => None,
    };
    // Gruppennamen gibt es nur mit Schalter (Aufgabe 30); sonst kein Client
    // und damit auch keine Anfrage.
    let graph = match konfig.entra.as_ref().filter(|entra| entra.gruppen_namen) {
        Some(entra) => Some(Arc::new(graph::Graph::neu(&entra.graph_basis)?)),
        None => None,
    };
    let ist_https = konfig.ist_https();
    let dist = konfig.admin_dist.clone().filter(|pfad| {
        let da = pfad.is_dir();
        if !da {
            tracing::warn!(
                "Admin-Oberfläche {} fehlt, es gibt nur die API",
                pfad.display()
            );
        }
        da
    });
    // Der Dummy-Hash für unbekannte E-Mails entsteht **beim Start** und auf
    // einer blockierenden Aufgabe (Aufgabe 26, Punkt 4) – nicht beim ersten
    // Rateversuch und nicht im Async-Faden.
    let dummy = tokio::task::spawn_blocking(benutzer::dummy_hash_erzeugen)
        .await
        .map_err(|fehler| anyhow::anyhow!("Dummy-Hash nicht baubar: {fehler}"))??;
    let daten = konfig.daten.clone();
    let rumpf_zeit = konfig.rumpf_zeit;
    let zustand = Arc::new(Zustand::neu(conn, konfig, schluessel, oidc, graph, dummy));
    // Aufgabe 37: was das Abbild mitbringt und was der Betreiber abgelegt hat.
    // Fehlt alles, sagt der Start das deutlich – die Geräte bekommen dann kein
    // Programm, und der Grund steht im Log.
    let stand = zustand.programme.stand();
    if stand.starts_with("keine Programme") {
        tracing::warn!(
            "Es liegen keine Programme bereit: {stand}. Geräte, deren Einträge ein \
             quiver-Programm nennen, bekommen nichts (die Konfiguration bleibt)."
        );
    } else {
        tracing::info!("Programme bereit: {stand}");
    }
    // Erst jetzt, wenn Datenbank, `-wal`, `-shm` und Schlüssel da sind: die
    // Rechte nachziehen (Symlinks werden nicht verfolgt).
    if let Err(fehler) = rechte::daten_pruefen(&daten) {
        tracing::warn!("Rechte in {} nicht prüfbar: {fehler}", daten.display());
    }
    let wache = zustand.clone();
    let router = router(zustand.clone(), dist)
        .layer(DefaultBodyLimit::max(RUMPF_STANDARD))
        .layer(RequestBodyTimeoutLayer::new(rumpf_zeit))
        .layer(middleware::from_fn_with_state(wache, host::host_wache))
        .layer(middleware::from_fn(move |req, next| {
            kopfzeilen::sicherheits_kopfzeilen(ist_https, req, next)
        }));
    Ok((router, zustand))
}

fn router(zustand: Arc<Zustand>, dist: Option<PathBuf>) -> Router {
    // Der Browser-Weg der Anmeldung (kein /api: der Browser folgt den
    // Weiterleitungen von Entra).
    let browser = Router::new()
        // Offen und ohne /api/v1, genau wie die Verwaltung sie aufruft
        // (Aufgabe 25/26): Stand der Einrichtung und das Einrichten selbst.
        .route("/auth/stand", get(einrichtung::stand))
        .route("/auth/einrichten", post(einrichtung::einrichten))
        // Stylesheet und Zeichen der Seiten (Aufgabe 26, Punkt 5).
        .route("/auth/stil.css", get(seiten::stil))
        .route("/auth/marke-dunkel.png", get(seiten::marke_dunkel))
        .route("/auth/marke-hell.png", get(seiten::marke_hell))
        .route("/auth/geraet/start", get(anmeldung::geraet_start))
        .route("/auth/admin/start", get(anmeldung::admin_start))
        .route("/auth/callback", get(anmeldung::callback))
        .route(
            "/auth/geraet/bestaetigen",
            post(anmeldung::geraet_bestaetigen),
        )
        .route("/auth/geraet/abbrechen", post(anmeldung::geraet_abbrechen))
        .method_not_allowed_fallback(methode_falsch);

    let geraete = Router::new()
        .route("/soll", get(geraet::soll))
        .route("/tools/{name}", get(geraet::tools))
        .route("/ereignisse", get(geraet::ereignisse))
        .route("/meldung", post(geraet::meldung))
        .route("/abmelden", post(geraet::abmelden))
        .method_not_allowed_fallback(methode_falsch)
        .route_layer(middleware::from_fn_with_state(
            zustand.clone(),
            auth::geraete_wache,
        ));

    let angemeldet = Router::new()
        .route("/v1/auth/ich", get(benutzer::ich))
        .route("/v1/auth/abmelden", post(benutzer::abmelden))
        .method_not_allowed_fallback(methode_falsch)
        .route_layer(middleware::from_fn_with_state(
            zustand.clone(),
            sitzung::angemeldet_wache,
        ));

    let verwaltung = Router::new()
        .route("/eintraege", get(admin::eintraege_liste))
        // Einträge dürfen größer sein als alles andere (server.json).
        .route(
            "/eintraege/import",
            post(admin::eintrag_importieren).layer(DefaultBodyLimit::max(RUMPF_EINTRAEGE)),
        )
        .route(
            "/eintraege/{id}",
            get(admin::eintrag_holen)
                .put(admin::eintrag_setzen)
                .delete(admin::eintrag_loeschen)
                .layer(DefaultBodyLimit::max(RUMPF_EINTRAEGE)),
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
        .route("/programme", get(admin::programme))
        .route("/benutzer", get(benutzer::benutzer_liste))
        .route(
            "/benutzer/{oid}",
            put(benutzer::benutzer_setzen).delete(benutzer::benutzer_loeschen),
        )
        .route("/benutzer/{oid}/pruefen", post(benutzer::benutzer_pruefen))
        .route("/benutzer/{oid}/gruppen", get(zuordnung::benutzer_gruppen))
        .route(
            "/benutzer/{oid}/gruppen/{gruppe}",
            put(zuordnung::benutzer_gruppe_setzen),
        )
        .route(
            "/gruppen/{id}/mitglieder",
            get(zuordnung::gruppe_mitglieder),
        )
        .route(
            "/gruppen/{id}/mitglieder/{oid}",
            put(zuordnung::mitglied_setzen),
        )
        .route("/entra-schluessel", get(benutzer::entra_ausweis))
        .route("/entra-gruppen", get(benutzer::entra_gruppen))
        .route("/entra-gruppen/{id}", put(benutzer::entra_gruppe_setzen))
        .route("/pruefspur", get(benutzer::pruefspur))
        .method_not_allowed_fallback(methode_falsch)
        // Erst die Spur, dann die Sitzung: die Sitzung ist außen und legt den
        // Benutzer in die Anfrage, den die Spur braucht.
        .route_layer(middleware::from_fn_with_state(
            zustand.clone(),
            spur::spur_wache,
        ))
        .route_layer(middleware::from_fn_with_state(
            zustand.clone(),
            sitzung::verwaltung_wache,
        ));

    let offen = Router::new()
        .route("/v1/anmelden", post(geraet::anmelden))
        .route("/v1/auth/root", post(benutzer::root_anmelden))
        .route("/v1/auth/geraet/tausch", post(anmeldung::geraet_tausch))
        .route("/v1/anmeldearten", get(benutzer::anmeldearten));

    let api = Router::new()
        .merge(offen)
        .merge(angemeldet)
        .nest("/v1/geraet", geraete)
        .nest("/v1/admin", verwaltung)
        .fallback(api_unbekannt)
        .method_not_allowed_fallback(methode_falsch)
        // Nur die API wird protokolliert: der Browser-Weg trägt Einmal-Codes
        // und `state` in der Abfrage, die gehören nicht ins Log.
        .layer(TraceLayer::new_for_http());

    let app = Router::new()
        .merge(browser)
        .nest("/api", api)
        // Der Weg für den Container selbst; nur über Loopback (siehe `host`).
        .route(host::GESUND, get(host::gesund))
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
    .map_err(|fehler| FehlerAntwort::intern_mit_kennung(&fehler))?
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

/// Die Programme, die zu diesem Soll gehören (Aufgabe 37).
///
/// Nur die, die ein Eintrag wirklich braucht, und nur für das System, das das
/// Gerät gemeldet hat. Nennt das Gerät keins (älterer Client), bleibt die Liste
/// leer – es bekommt dann keine Programme, sondern sucht sie wie bisher.
fn mit_programmen(
    soll: Soll,
    system: Option<&quiver_domain::System>,
    ablage: &crate::programme::Ablage,
) -> Soll {
    let Some(system) = system else {
        return soll.mit_programmen(Vec::new());
    };
    let gebraucht: std::collections::BTreeSet<&str> = soll
        .eintraege
        .iter()
        .filter_map(|eintrag| match &eintrag.eintrag.laufzeit {
            quiver_domain::Laufzeit::Stdio { befehl, .. }
                if quiver_domain::ist_verteilt(befehl.trim()) =>
            {
                Some(befehl.trim())
            }
            _ => None,
        })
        .collect();
    let programme = ablage
        .fuer(system)
        .into_iter()
        .filter(|programm| gebraucht.contains(programm.name.as_str()))
        .collect();
    soll.mit_programmen(programme)
}

/// Soll eines Geräts aus der Datenbank; Fehler daraus werden zu 500 mit Text.
///
/// Mit dem System des Geräts nennt das Soll auch die Programme, die der Server
/// dafür hat (Aufgabe 37). Ohne System (`None`, ein älterer Client) bleibt die
/// Programmliste leer.
pub(crate) async fn soll_fuer_system(
    zustand: &Arc<Zustand>,
    id: &GeraetId,
    system: Option<quiver_domain::System>,
) -> Ergebnis<Soll> {
    let kennung = id.clone();
    let soll = db(zustand, move |conn| {
        let zeile = speicher::hole_geraet(conn, kennung.als_str())?
            .ok_or_else(|| anyhow::anyhow!("Gerät {kennung} gibt es nicht"))?;
        let bestand = speicher::bestand(conn)?;
        soll_aus(&bestand, &zeile.geraet)
    })
    .await?;
    Ok(mit_programmen(soll, system.as_ref(), &zustand.programme))
}

/// Läuft, wenn Unterbefehle die Datenbank geändert haben: `PRAGMA data_version`
/// ändert sich bei Schreibzugriffen anderer Verbindungen. Abfrage alle 500 ms,
/// damit die Geräte in höchstens zwei Sekunden angestoßen werden.
async fn beobachte_datenbank(zustand: Arc<Zustand>) {
    let pfad = zustand.konfig.daten.join("quiver.db");
    let conn = match Connection::open(&pfad) {
        Ok(conn) => conn,
        Err(fehler) => {
            tracing::warn!("Datenbank {} nicht beobachtbar: {fehler}", pfad.display());
            return;
        }
    };
    let mut letzte = speicher::data_version(&conn).unwrap_or(0);
    let mut takt = tokio::time::interval(std::time::Duration::from_millis(500));
    takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        takt.tick().await;
        match speicher::data_version(&conn) {
            Ok(neu) if neu != letzte => {
                letzte = neu;
                zustand.melde_aenderung();
            }
            Ok(_) => {}
            Err(fehler) => tracing::warn!("data_version nicht lesbar: {fehler}"),
        }
    }
}
