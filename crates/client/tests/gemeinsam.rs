// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Testhilfe: Root anlegen und eine Verwaltungssitzung holen.
//!
//! Seit Aufgabe 20 gibt es keinen Admin-Schlüssel mehr. Die Verwaltung meldet
//! sich mit einem Root-Benutzer an; genau das macht diese Hilfe, damit die
//! Tests der Verwaltung und des Clients weiterlaufen.

use std::path::Path;

use serde_json::json;

pub const ROOT_EMAIL: &str = "root@example.test";
pub const ROOT_PASSWORT: &str = "ein-sehr-langes-testpasswort";

/// Argon2 läuft höchstens zweimal gleichzeitig; Tests melden sich nacheinander an.
static ROOT_SPERRE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Root setzen und anmelden; liefert den Cookie-Kopf (`Name=Wert`).
pub async fn sitzung(http: &reqwest::Client, basis: &str, daten: &Path) -> String {
    let _wache = ROOT_SPERRE.lock().await;
    quiver_server::benutzer::root_setzen_auf_platte(daten, ROOT_EMAIL, ROOT_PASSWORT)
        .expect("Root setzen");
    let antwort = http
        .post(format!("{basis}/api/v1/auth/root"))
        .header("content-type", "application/json")
        .header("origin", basis)
        .header("x-quiver", "1")
        .body(
            serde_json::to_vec(&json!({
                "email": ROOT_EMAIL,
                "passwort": ROOT_PASSWORT
            }))
            .expect("JSON"),
        )
        .send()
        .await
        .expect("Server erreichbar");
    let status = antwort.status();
    let set = antwort
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|wert| wert.to_str().ok())
        .map(str::to_string);
    assert_eq!(status, reqwest::StatusCode::OK, "Root-Anmeldung: {set:?}");
    set.expect("Sitzungscookie")
        .split(';')
        .next()
        .expect("Name=Wert")
        .to_string()
}
