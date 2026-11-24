// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Root-Benutzer, Rollen, Sitzungen, Prüfspur – die Verwaltung ohne
//! Admin-Schlüssel (ADR 0002, Aufgabe 20).
//!
//! Das Passwort wird mit Argon2id geprüft (64 MiB, t=3, p=1); höchstens zwei
//! Prüfungen laufen gleichzeitig. Rateversuche werden je IP **und** je Konto
//! gedrosselt, vor der Prüfung. Unbekannte E-Mail und falsches Passwort sehen
//! gleich aus und brauchen gleich lang.

use std::path::Path;
use std::sync::Arc;

use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use axum::extract::{Path as AxumPath, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use quiver_protocol::{
    AnmeldeGrund as QuiverGrund, Anmeldeart, Anmeldearten, BenutzerAendern, BenutzerSicht,
    EntraGruppe, Ich, PruefspurEintrag, Rolle, RootAnmelden,
};
use time::OffsetDateTime;

use crate::db;
use crate::drossel::{email_normalisieren, ip_schluessel, konto_schluessel};
use crate::fehler::{Ergebnis, FehlerAntwort};
use crate::sitzung::{self, SitzungKontext};
use crate::speicher::{self, Benutzer};
use crate::zustand::Zustand;
use serde::Deserialize;

/// Kürzeste Länge eines Root-Passworts.
pub const PASSWORT_MIN: usize = 12;
/// Längste Länge eines Root-Passworts.
pub const PASSWORT_MAX: usize = 256;
/// Höchstens so viele Argon2-Prüfungen gleichzeitig.
pub const ARGON2_GLEICHZEITIG: usize = 2;

/// Argon2id mit den Parametern aus der Aufgabe.
pub fn argon2() -> Argon2<'static> {
    let params = Params::new(64 * 1024, 3, 1, None).expect("Argon2-Parameter sind gültig");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Passwort hashen (PHC-Text).
pub fn passwort_hashen(passwort: &str) -> anyhow::Result<String> {
    let hash = argon2()
        .hash_password(passwort.as_bytes())
        .map_err(|fehler| anyhow::anyhow!("Passwort nicht hashbar: {fehler}"))?;
    Ok(hash.to_string())
}

/// Passwort gegen einen PHC-Text prüfen.
pub fn passwort_pruefen(passwort: &str, hash: &str) -> bool {
    argon2().verify_password(passwort.as_bytes(), hash).is_ok()
}

/// Ein Dummy-Hash für unbekannte E-Mails: gleiche Arbeit, gleiche Dauer.
///
/// Er wird **beim Start** erzeugt (Aufgabe 26, Punkt 4), nicht beim ersten
/// fehlgeschlagenen Versuch und nicht im Async-Faden – sonst wäre der erste
/// Rateversuch schneller als alle folgenden und damit ein Orakel.
pub fn dummy_hash_erzeugen() -> anyhow::Result<String> {
    passwort_hashen(DUMMY_PASSWORT)
}

/// Das Passwort, das nie jemand benutzt; nur der Hash davon wird gebraucht.
pub const DUMMY_PASSWORT: &str = "kein-benutzer-hat-dieses-passwort-123";

/// `GET /anmeldearten`: welche Wege bietet dieser Server? (Für die Anmeldeseite.)
pub(crate) async fn anmeldearten(
    State(zustand): State<Arc<Zustand>>,
) -> Ergebnis<Json<Anmeldearten>> {
    let root = db(&zustand, |conn| speicher::root_ist_gesetzt(conn)).await?;
    Ok(Json(Anmeldearten {
        entra: zustand.oidc.is_some(),
        root,
    }))
}

// ------------------------------------------------------------------ Root-API

/// `POST /auth/root`: Anmeldung des Notzugangs mit E-Mail und Passwort.
pub(crate) async fn root_anmelden(
    State(zustand): State<Arc<Zustand>>,
    req: Request,
) -> Ergebnis<Response> {
    let ip = sitzung::aufrufer(&req, &zustand);
    let (teile, rumpf) = req.into_parts();
    sitzung::aenderung_pruefen(&teile.headers, &zustand.konfig.oeffentlich)?;
    let bytes = axum::body::to_bytes(rumpf, 64 * 1024)
        .await
        .map_err(|_| FehlerAntwort::ungueltig("Rumpf nicht lesbar"))?;
    let anfrage: RootAnmelden = serde_json::from_slice(&bytes)
        .map_err(|_| FehlerAntwort::ungueltig("Rumpf ist kein JSON mit email und passwort"))?;

    let email = email_normalisieren(&anfrage.email);
    if email.is_empty() || email.len() > 254 {
        return Err(FehlerAntwort::ungueltig("E-Mail fehlt oder ist zu lang"));
    }
    if !(PASSWORT_MIN..=PASSWORT_MAX).contains(&anfrage.passwort.chars().count()) {
        return Err(FehlerAntwort::ungueltig(format!(
            "Passwort muss {PASSWORT_MIN} bis {PASSWORT_MAX} Zeichen haben"
        )));
    }

    let jetzt = zustand.uhr.jetzt();
    let ip_schluessel = ip_schluessel(ip);
    let konto = konto_schluessel(&email);
    warteschlange(&zustand, &ip_schluessel, &konto, jetzt)?;

    let root = db(&zustand, {
        let email = email.clone();
        move |conn| {
            Ok((
                speicher::root_ist_gesetzt(conn)?,
                speicher::root_per_email(conn, &email)?,
                speicher::root_passwort(conn)?,
            ))
        }
    })
    .await?;
    let (gesetzt, gefunden, hash) = root;
    if !gesetzt {
        return Err(FehlerAntwort::nicht_gefunden(
            "kein Root gesetzt; bitte `quiver-server root-setzen` ausführen",
        ));
    }

    // Auch bei unbekannter E-Mail wird gerechnet, und zwar am Dummy-Hash –
    // sonst wäre die Antwortzeit ein Orakel für gültige Konten. Der Dummy-Hash
    // ist beim Start entstanden (`Zustand::dummy`), nicht jetzt.
    let vorhanden = gefunden.is_some();
    let permit = zustand.argon.try_acquire().map_err(|_| {
        FehlerAntwort::neu(
            StatusCode::TOO_MANY_REQUESTS,
            "zu viele Prüfungen; bitte kurz warten",
        )
    })?;
    let passwort = anfrage.passwort.clone();
    let hash = if vorhanden {
        hash.unwrap_or_else(|| zustand.dummy.clone())
    } else {
        zustand.dummy.clone()
    };
    let passt = tokio::task::spawn_blocking(move || passwort_pruefen(&passwort, &hash))
        .await
        .map_err(|fehler| {
            FehlerAntwort::intern(format!("Passwortprüfung gescheitert: {fehler}"))
        })?;
    drop(permit);

    // Unbekannte E-Mail und falsches Passwort: gleiche Antwort, gleiche Dauer.
    let passt = passt && vorhanden;
    if !passt {
        zustand.drossel.fehlschlag(&ip_schluessel, jetzt);
        zustand.drossel.fehlschlag(&konto, jetzt);
        let akteur = if vorhanden {
            email.clone()
        } else {
            "unbekannt".to_string()
        };
        db(&zustand, move |conn| {
            speicher::pruefspur_schreiben(conn, jetzt, "fehlschlag", &akteur, "Root-Anmeldung")?;
            Ok(())
        })
        .await?;
        return Err(FehlerAntwort::nicht_angemeldet(
            "E-Mail oder Passwort falsch",
        ));
    }

    zustand.drossel.erfolg(&ip_schluessel);
    zustand.drossel.erfolg(&konto);

    let kennung = speicher::ROOT_OID.to_string();
    let token = db(&zustand, move |conn| {
        let token = sitzung::sitzung_anlegen(conn, &kennung, jetzt)?;
        speicher::pruefspur_schreiben(conn, jetzt, "anmeldung", &email, "Root über Passwort")?;
        Ok(token)
    })
    .await?;

    let benutzer = db(&zustand, {
        let kennung = speicher::ROOT_OID.to_string();
        move |conn| speicher::benutzer_holen(conn, &kennung)
    })
    .await?
    .ok_or_else(|| FehlerAntwort::intern("Root ist nach der Anmeldung verschwunden"))?;

    antwort_mit_sitzung(&zustand, &token, &benutzer, Anmeldeart::Root)
}

/// Prüft beide Drosseln und antwortet mit 429, wenn eine noch sperrt.
fn warteschlange(zustand: &Zustand, ip: &str, konto: &str, jetzt: OffsetDateTime) -> Ergebnis<()> {
    let warten = [ip, konto]
        .iter()
        .filter_map(|schluessel| zustand.drossel.wartezeit(schluessel, jetzt))
        .max();
    match warten {
        Some(sekunden) => Err(FehlerAntwort::neu(
            StatusCode::TOO_MANY_REQUESTS,
            format!("zu viele Versuche; bitte {sekunden} Sekunden warten"),
        )),
        None => Ok(()),
    }
}

/// Antwort nach einer Anmeldung: Rumpf [`Ich`] und Sitzungscookie.
pub(crate) fn antwort_mit_sitzung(
    zustand: &Zustand,
    token: &str,
    benutzer: &Benutzer,
    art: Anmeldeart,
) -> Ergebnis<Response> {
    let koerper = Ich {
        benutzer: sicht(benutzer, 0),
        art,
    };
    let mut antwort = Json(koerper).into_response();
    let cookie = sitzung::sitzung_cookie(token, zustand.konfig.ist_https());
    antwort.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|_| FehlerAntwort::intern("Cookie-Kopf nicht darstellbar"))?,
    );
    Ok(antwort)
}

/// `GET /auth/ich`: wer ist angemeldet, mit frischer Rolle aus der DB.
pub(crate) async fn ich(
    State(zustand): State<Arc<Zustand>>,
    axum::Extension(kontext): axum::Extension<SitzungKontext>,
) -> Ergebnis<Json<Ich>> {
    let kennung = kontext.benutzer.oid.clone();
    let geraete = db(&zustand, move |conn| {
        Ok(speicher::geraete_von_user(conn, &kennung)?.len())
    })
    .await?;
    Ok(Json(Ich {
        benutzer: sicht(
            &kontext.benutzer,
            u32::try_from(geraete).unwrap_or(u32::MAX),
        ),
        art: kontext.art,
    }))
}

/// `POST /auth/abmelden`: Sitzung am Server löschen.
pub(crate) async fn abmelden(
    State(zustand): State<Arc<Zustand>>,
    req: Request,
) -> Ergebnis<Response> {
    let token = sitzung::sitzungstoken(&req, zustand.konfig.ist_https());
    sitzung::aenderung_pruefen(req.headers(), &zustand.konfig.oeffentlich)?;
    if let Some(token) = token {
        sitzung::sitzung_beenden(&zustand, &token).await?;
    }
    let mut antwort = StatusCode::NO_CONTENT.into_response();
    let cookie = sitzung::loesch_cookie(
        if zustand.konfig.ist_https() {
            sitzung::SITZUNG_SICHER
        } else {
            sitzung::SITZUNG
        },
        "/",
        zustand.konfig.ist_https(),
    );
    if let Ok(wert) = HeaderValue::from_str(&cookie) {
        antwort.headers_mut().insert(header::SET_COOKIE, wert);
    }
    Ok(antwort)
}

// -------------------------------------------------------------- Verwaltung

pub(crate) fn sicht(benutzer: &Benutzer, geraete: u32) -> BenutzerSicht {
    BenutzerSicht {
        oid: benutzer.oid.clone(),
        email: benutzer.email.clone(),
        name: benutzer.name.clone(),
        rolle: benutzer.rolle,
        gesperrt: benutzer.gesperrt,
        entra: benutzer.entra.clone(),
        zuletzt: benutzer.zuletzt.clone(),
        token_stand: benutzer.token_stand.clone(),
        geraete,
    }
}

pub(crate) async fn benutzer_liste(
    State(zustand): State<Arc<Zustand>>,
) -> Ergebnis<Json<Vec<BenutzerSicht>>> {
    let liste = db(&zustand, |conn| {
        let mut aus = Vec::new();
        for benutzer in speicher::benutzer_liste(conn)? {
            let geraete = speicher::geraete_von_user(conn, &benutzer.oid)?.len();
            aus.push(sicht(&benutzer, u32::try_from(geraete).unwrap_or(u32::MAX)));
        }
        Ok(aus)
    })
    .await?;
    Ok(Json(liste))
}

pub(crate) async fn benutzer_setzen(
    State(zustand): State<Arc<Zustand>>,
    axum::Extension(akteur): axum::Extension<SitzungKontext>,
    AxumPath(oid): AxumPath<String>,
    Json(aenderung): Json<BenutzerAendern>,
) -> Ergebnis<Json<BenutzerSicht>> {
    let jetzt = zustand.uhr.jetzt();
    let wer = akteur.benutzer.oid.clone();
    let rolle_wer = akteur.benutzer.rolle;
    let kennung = oid.clone();
    let stand = db(&zustand, move |conn| {
        let Some(mut benutzer) = speicher::benutzer_holen(conn, &kennung)? else {
            return Ok(Err(FehlerAntwort::nicht_gefunden(format!(
                "Benutzer {kennung} gibt es nicht"
            ))));
        };
        let ist_root = benutzer.rolle == Rolle::Root;
        let neuer_root = matches!(aenderung.rolle, Some(Rolle::Root));
        if ist_root && aenderung.rolle.is_some() && !neuer_root {
            return Ok(Err(FehlerAntwort::konflikt(
                "der Root-Benutzer lässt sich nicht herabstufen",
            )));
        }
        if neuer_root && !ist_root {
            return Ok(Err(FehlerAntwort::verboten(
                "Root wird nur über `quiver-server root-setzen` gesetzt",
            )));
        }
        if ist_root && aenderung.gesperrt == Some(true) {
            return Ok(Err(FehlerAntwort::konflikt(
                "der Root-Benutzer lässt sich nicht sperren",
            )));
        }
        if kennung == wer
            && (aenderung.gesperrt == Some(true)
                || matches!(aenderung.rolle, Some(Rolle::Benutzer)))
            && !aenderung.selbst_bestaetigt
        {
            return Ok(Err(FehlerAntwort::konflikt(
                "sich selbst entziehen nur mit selbst_bestaetigt",
            )));
        }
        if !rolle_wer.darf_verwalten() {
            return Ok(Err(FehlerAntwort::verboten(
                "diese Rolle darf die Verwaltung nicht benutzen",
            )));
        }
        if let Some(rolle) = aenderung.rolle {
            if benutzer.rolle != rolle {
                speicher::rolle_setzen(conn, &kennung, rolle)?;
                speicher::pruefspur_schreiben(
                    conn,
                    jetzt,
                    "rolle",
                    &wer,
                    &format!(
                        "{kennung}: Rolle {} → {}",
                        benutzer.rolle.als_str(),
                        rolle.als_str()
                    ),
                )?;
                benutzer.rolle = rolle;
            }
        }
        if let Some(gesperrt) = aenderung.gesperrt {
            if benutzer.gesperrt != gesperrt {
                speicher::sperre_setzen(conn, &kennung, gesperrt)?;
                if gesperrt {
                    // Sperren wirkt sofort: alle Sitzungen enden.
                    conn.execute(
                        "DELETE FROM sitzungen WHERE oid = ?1",
                        rusqlite::params![kennung],
                    )?;
                }
                speicher::pruefspur_schreiben(
                    conn,
                    jetzt,
                    "aenderung",
                    &wer,
                    &format!(
                        "{kennung}: {}",
                        if gesperrt { "gesperrt" } else { "entsperrt" }
                    ),
                )?;
                benutzer.gesperrt = gesperrt;
            }
        }
        let geraete = speicher::geraete_von_user(conn, &kennung)?.len();
        Ok(Ok(sicht(
            &benutzer,
            u32::try_from(geraete).unwrap_or(u32::MAX),
        )))
    })
    .await??;
    zustand.melde_aenderung();
    Ok(Json(stand))
}

pub(crate) async fn benutzer_loeschen(
    State(zustand): State<Arc<Zustand>>,
    axum::Extension(akteur): axum::Extension<SitzungKontext>,
    AxumPath(oid): AxumPath<String>,
) -> Ergebnis<StatusCode> {
    let jetzt = zustand.uhr.jetzt();
    let wer = akteur.benutzer.oid.clone();
    let kennung = oid.clone();
    let weg = db(&zustand, move |conn| {
        let Some(benutzer) = speicher::benutzer_holen(conn, &kennung)? else {
            return Ok(Err(FehlerAntwort::nicht_gefunden(format!(
                "Benutzer {kennung} gibt es nicht"
            ))));
        };
        if benutzer.rolle == Rolle::Root {
            return Ok(Err(FehlerAntwort::konflikt(
                "der Root-Benutzer lässt sich nicht löschen",
            )));
        }
        let geloescht = speicher::benutzer_loeschen(conn, &kennung)?;
        if geloescht {
            speicher::pruefspur_schreiben(
                conn,
                jetzt,
                "aenderung",
                &wer,
                &format!("{kennung}: gelöscht"),
            )?;
        }
        Ok(Ok(geloescht))
    })
    .await??;
    if !weg {
        return Err(FehlerAntwort::nicht_gefunden(format!(
            "Benutzer {oid} gibt es nicht"
        )));
    }
    zustand.melde_aenderung();
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /admin/benutzer/{oid}/pruefen`: Refresh-Token sofort einlösen.
pub(crate) async fn benutzer_pruefen(
    State(zustand): State<Arc<Zustand>>,
    AxumPath(oid): AxumPath<String>,
) -> Ergebnis<Json<BenutzerSicht>> {
    if zustand.oidc.is_none() {
        return Err(FehlerAntwort::nicht_gefunden(
            "dieser Server nutzt kein Entra; es gibt nichts nachzuprüfen",
        ));
    }
    match crate::refresh::jetzt_pruefen(&zustand, &oid).await {
        Ok(()) => {}
        Err(QuiverGrund::Gesperrt) => return Err(FehlerAntwort::verboten("Benutzer ist gesperrt")),
        Err(QuiverGrund::NeuAnmelden) => {
            return Err(FehlerAntwort::verboten(
                "Anmeldung gilt nicht mehr; neu anmelden nötig",
            ))
        }
        Err(QuiverGrund::KeinZugriff) => {
            return Err(FehlerAntwort::verboten(
                "dem Benutzer ist keine Gruppe zugeordnet",
            ))
        }
    }
    let kennung = oid.clone();
    let sicht = db(&zustand, move |conn| {
        let Some(benutzer) = speicher::benutzer_holen(conn, &kennung)? else {
            return Ok(None);
        };
        let geraete = speicher::geraete_von_user(conn, &kennung)?.len();
        Ok(Some(sicht(
            &benutzer,
            u32::try_from(geraete).unwrap_or(u32::MAX),
        )))
    })
    .await?
    .ok_or_else(|| FehlerAntwort::nicht_gefunden(format!("Benutzer {oid} gibt es nicht")))?;
    Ok(Json(sicht))
}

pub(crate) async fn entra_gruppen(
    State(zustand): State<Arc<Zustand>>,
) -> Ergebnis<Json<Vec<EntraGruppe>>> {
    let liste = db(&zustand, |conn| {
        Ok(speicher::entra_gruppen_liste(conn)?
            .into_iter()
            .map(|zeile| EntraGruppe {
                id: zeile.id,
                anzeigename: zeile.anzeigename,
                zuletzt: zeile.zuletzt,
            })
            .collect())
    })
    .await?;
    Ok(Json(liste))
}

#[derive(Debug, Deserialize)]
pub(crate) struct Anzeigename {
    #[serde(default)]
    pub anzeigename: Option<String>,
}

pub(crate) async fn entra_gruppe_setzen(
    State(zustand): State<Arc<Zustand>>,
    AxumPath(id): AxumPath<String>,
    Json(aenderung): Json<Anzeigename>,
) -> Ergebnis<Json<EntraGruppe>> {
    let kennung = id.clone();
    let name = aenderung.anzeigename.clone();
    let gesetzt = db(&zustand, move |conn| {
        speicher::entra_anzeigename_setzen(conn, &kennung, name.as_deref())
    })
    .await?;
    if !gesetzt {
        return Err(FehlerAntwort::nicht_gefunden(format!(
            "Entra-Gruppe {id} wurde noch nicht gesehen"
        )));
    }
    let zeile = db(&zustand, move |conn| {
        Ok(speicher::entra_gruppen_liste(conn)?
            .into_iter()
            .find(|zeile| zeile.id == id))
    })
    .await?
    .ok_or_else(|| FehlerAntwort::intern("Gruppe ist nach dem Speichern verschwunden"))?;
    Ok(Json(EntraGruppe {
        id: zeile.id,
        anzeigename: zeile.anzeigename,
        zuletzt: zeile.zuletzt,
    }))
}

pub(crate) async fn pruefspur(
    State(zustand): State<Arc<Zustand>>,
    axum::extract::Query(abfrage): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Ergebnis<Json<Vec<PruefspurEintrag>>> {
    let limit = abfrage
        .get("limit")
        .and_then(|wert| wert.parse::<u32>().ok())
        .unwrap_or(200);
    let liste = db(&zustand, move |conn| {
        Ok(speicher::pruefspur_liste(conn, limit)?
            .into_iter()
            .map(|zeile| PruefspurEintrag {
                zeit: zeile.zeit,
                art: zeile.art,
                akteur: zeile.akteur,
                text: zeile.text,
            })
            .collect())
    })
    .await?;
    Ok(Json(liste))
}

// ----------------------------------------------------------- Kommandozeile

/// Prüft eine E-Mail grob (eine `@`, kein Leerzeichen, höchstens 254 Zeichen).
pub fn email_gueltig(email: &str) -> bool {
    let email = email.trim();
    !email.is_empty()
        && email.len() <= 254
        && !email.chars().any(char::is_whitespace)
        && email.matches('@').count() == 1
}

/// Setzt den Root-Benutzer (Kommandozeile). Danach gibt es keinen
/// Admin-Schlüssel mehr; `<daten>/admin-token` wird entfernt.
pub fn root_setzen_auf_platte(daten: &Path, email: &str, passwort: &str) -> anyhow::Result<()> {
    let email = email_normalisieren(email);
    if !email_gueltig(&email) {
        anyhow::bail!("E-Mail ist nicht gültig");
    }
    if !(PASSWORT_MIN..=PASSWORT_MAX).contains(&passwort.chars().count()) {
        anyhow::bail!("Passwort muss {PASSWORT_MIN} bis {PASSWORT_MAX} Zeichen haben");
    }
    std::fs::create_dir_all(daten)?;
    let conn = speicher::oeffnen(&daten.join("quiver.db"))?;
    let hash = passwort_hashen(passwort)?;
    let jetzt = OffsetDateTime::now_utc();
    speicher::root_setzen(&conn, &email, &hash, jetzt)?;
    speicher::pruefspur_schreiben(
        &conn,
        jetzt,
        "kommandozeile",
        &email,
        "Root gesetzt; alle Root-Sitzungen beendet",
    )?;
    let token_datei = daten.join("admin-token");
    if token_datei.exists() {
        std::fs::remove_file(&token_datei)?;
        tracing::warn!(
            "{} entfernt: der Admin-Schlüssel gilt nicht mehr",
            token_datei.display()
        );
    }
    Ok(())
}

/// Macht einen Benutzer zum Admin (Kommandozeile).
pub fn admin_setzen_auf_platte(daten: &Path, oid: &str) -> anyhow::Result<()> {
    let conn = speicher::oeffnen(&daten.join("quiver.db"))?;
    let jetzt = OffsetDateTime::now_utc();
    let gesetzt = speicher::rolle_setzen(&conn, oid, Rolle::Admin)?;
    if !gesetzt {
        anyhow::bail!("Benutzer {oid} wurde noch nicht gesehen; erst anmelden lassen");
    }
    speicher::pruefspur_schreiben(
        &conn,
        jetzt,
        "kommandozeile",
        "root-setzen",
        &format!("{oid}: Rolle admin"),
    )?;
    Ok(())
}

/// Warnt einmal beim Start, wenn noch `QUIVER_ADMIN_TOKEN` gesetzt ist.
pub fn admin_token_ignorieren() {
    if std::env::var_os("QUIVER_ADMIN_TOKEN").is_some() {
        tracing::warn!(
            "QUIVER_ADMIN_TOKEN wird seit Aufgabe 20 ignoriert; die Verwaltung nutzt Sitzungen"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_hash_ist_ein_echter_argon2id_hash() {
        // Beim Start erzeugt (`Zustand::dummy`), damit der erste Rateversuch
        // nicht schneller ist als alle folgenden (Aufgabe 26, Punkt 4).
        let hash = dummy_hash_erzeugen().expect("Dummy-Hash");
        assert!(hash.starts_with("$argon2id$"), "kein Argon2id: {hash}");
        assert!(passwort_pruefen(DUMMY_PASSWORT, &hash));
        assert!(!passwort_pruefen("etwas-anderes", &hash));
        // Zwei Hashes desselben Passworts sind verschieden (eigener Salt).
        let zweiter = dummy_hash_erzeugen().expect("Dummy-Hash");
        assert_ne!(hash, zweiter);
    }

    #[test]
    fn passwortregeln_und_email() {
        assert!(!email_gueltig("keine-email"));
        assert!(!email_gueltig("zwei@punkte@example.test"));
        assert!(!email_gueltig("mit leer@example.test"));
        assert!(email_gueltig("root@example.test"));
        let hash = passwort_hashen("kurz").expect("Hash");
        assert!(passwort_pruefen("kurz", &hash));
        assert!(!passwort_pruefen("lang", &hash));
    }
}
