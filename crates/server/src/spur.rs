// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Prüfspur der Verwaltung: **jede** ändernde Route schreibt, wer was getan hat.
//!
//! Der Eintrag entsteht in der Wache [`spur_wache`] und nicht in den Ansichten:
//! so kann keine Route ihn vergessen, auch keine, die später dazukommt. In der
//! Spur stehen Methode, Pfadmuster und Aktion – **keine Werte**, keine Namen
//! von Einträgen, keine Geheimnisse.
//!
//! Eine Route, die nichts geändert hat (abgelehnt, nicht gefunden, Konflikt),
//! schreibt nichts: die Spur ist ein Verzeichnis der Änderungen, kein Zugriffslog.

use std::sync::Arc;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::sitzung::{ist_aendernd, SitzungKontext};
use crate::speicher;
use crate::zustand::Zustand;

/// So heißt die Spur in der Datenbank.
pub const ART: &str = "verwaltung";

/// Die ändernden Routen der Verwaltung mit ihrer Aktion.
///
/// Die Reihenfolge ist die der Routen in [`crate::router`]; der Test
/// `jede_aendernde_verwaltungsroute_schreibt` geht die Tabelle durch.
pub const AENDERNDE_ROUTEN: &[(&str, &str, &str)] = &[
    (
        "POST",
        "/api/v1/admin/eintraege/import",
        "Eintrag importiert",
    ),
    ("PUT", "/api/v1/admin/eintraege/{id}", "Eintrag gespeichert"),
    ("DELETE", "/api/v1/admin/eintraege/{id}", "Eintrag gelöscht"),
    ("PUT", "/api/v1/admin/saetze/{id}", "Satz gespeichert"),
    ("DELETE", "/api/v1/admin/saetze/{id}", "Satz gelöscht"),
    ("PUT", "/api/v1/admin/gruppen/{id}", "Gruppe gespeichert"),
    ("DELETE", "/api/v1/admin/gruppen/{id}", "Gruppe gelöscht"),
    ("PUT", "/api/v1/admin/geraete/{id}", "Gerät geändert"),
    ("DELETE", "/api/v1/admin/geraete/{id}", "Gerät gelöscht"),
    ("POST", "/api/v1/admin/codes", "Einmal-Code angelegt"),
    ("PUT", "/api/v1/admin/benutzer/{oid}", "Benutzer geändert"),
    (
        "DELETE",
        "/api/v1/admin/benutzer/{oid}",
        "Benutzer gelöscht",
    ),
    (
        "POST",
        "/api/v1/admin/benutzer/{oid}/pruefen",
        "Benutzer nachgeprüft",
    ),
    (
        "PUT",
        "/api/v1/admin/entra-gruppen/{id}",
        "Entra-Gruppe aufgenommen",
    ),
];

/// Die Aktion zu Methode und Pfadmuster, falls die Tabelle sie kennt.
pub fn aktion(methode: &str, pfad: &str) -> Option<&'static str> {
    AENDERNDE_ROUTEN
        .iter()
        .find(|(m, p, _)| *m == methode && *p == pfad)
        .map(|(_, _, aktion)| *aktion)
}

/// Was in der Spur steht: Methode, Pfadmuster und – wenn bekannt – die Aktion.
pub fn eintrag(methode: &str, pfad: &str) -> String {
    match aktion(methode, pfad) {
        Some(aktion) => format!("{methode} {pfad} – {aktion}"),
        // Auch eine Route ohne Tabelleneintrag wird vermerkt; sie soll nur
        // nicht lange fehlen.
        None => format!("{methode} {pfad}"),
    }
}

/// Wessen Name in der Spur steht: E-Mail, sonst Name, sonst Objekt-Id.
fn akteur(kontext: &SitzungKontext) -> String {
    kontext
        .benutzer
        .email
        .clone()
        .or_else(|| kontext.benutzer.name.clone())
        .unwrap_or_else(|| kontext.benutzer.oid.clone())
}

/// Nach einer erfolgreichen Änderung in die Prüfspur schreiben.
pub(crate) async fn spur_wache(
    State(zustand): State<Arc<Zustand>>,
    req: Request,
    next: Next,
) -> Response {
    let aendernd = ist_aendernd(req.method());
    let methode = req.method().as_str().to_string();
    let pfad = req
        .extensions()
        .get::<MatchedPath>()
        .map(|pfad| pfad.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let kontext = req.extensions().get::<SitzungKontext>().cloned();
    let antwort = next.run(req).await;

    let (true, Some(kontext)) = (aendernd, kontext) else {
        return antwort;
    };
    if !antwort.status().is_success() {
        return antwort;
    }
    let texte = eintrag(&methode, &pfad);
    let akteur = akteur(&kontext);
    let jetzt = zustand.uhr.jetzt();
    let spur = crate::db(&zustand, move |conn| {
        speicher::pruefspur_schreiben(conn, jetzt, ART, &akteur, &texte)
    })
    .await;
    if let Err(fehler) = spur {
        // Die Änderung ist passiert; ein Fehler in der Spur darf sie nicht
        // ungeschehen machen – er gehört nur ins Log.
        tracing::warn!("Prüfspur nicht geschrieben: {}", fehler.text);
    }
    antwort
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jede_aendernde_route_hat_eine_aktion() {
        for (methode, pfad, text) in AENDERNDE_ROUTEN {
            assert!(!text.is_empty(), "{methode} {pfad}");
            assert_eq!(aktion(methode, pfad), Some(*text));
            let spur = eintrag(methode, pfad);
            assert!(spur.contains(methode) && spur.contains(pfad) && spur.contains(text));
        }
        // Keine Doppelten: eine Route, zwei Einträge wären zwei Wahrheiten.
        let mut gesehen = std::collections::HashSet::new();
        for (methode, pfad, _) in AENDERNDE_ROUTEN {
            assert!(gesehen.insert((methode, pfad)), "{methode} {pfad} doppelt");
        }
    }

    #[test]
    fn unbekannte_route_wird_trotzdem_vermerkt() {
        assert_eq!(aktion("PUT", "/api/v1/admin/neu"), None);
        assert_eq!(eintrag("PUT", "/api/v1/admin/neu"), "PUT /api/v1/admin/neu");
    }
}
