// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Ersteinrichtung im Browser (Aufgabe 25): gibt es keinen Root, legt
//! genau ein Aufruf ihn an – innerhalb von 15 Minuten nach dem Start und,
//! außer auf Loopback, nur mit dem Einmal-Code aus dem Server-Log.
//!
//! Der Zustand liegt im Arbeitsspeicher: Code (gehasht), Fehlversuche und das
//! Zeitfenster. Das Fenster hängt an der **Laufzeit** des Prozesses, nicht an
//! der Wanduhr (`config::Laufzeit`); wer die Systemzeit verstellt, verlängert es
//! nicht.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;
use quiver_protocol::{AuthStand, Einrichten, Einrichtungslage};

use crate::benutzer::{self, PASSWORT_MAX, PASSWORT_MIN};
use crate::db;
use crate::drossel::{email_normalisieren, ip_schluessel};
use crate::fehler::{Ergebnis, FehlerAntwort};
use crate::hilfen;
use crate::sitzung;
use crate::speicher;
use crate::zustand::Zustand;

/// So lange nach dem Start ist die Einrichtung offen.
pub const FENSTER_MINUTEN: u64 = 15;
/// Nach so vielen falschen Codes ist die Einrichtung **für diese Adresse** zu.
pub const CODE_FEHLVERSUCHE: u32 = 10;
/// Erst nach so vielen falschen Codes ist die Einrichtung ganz zu – über alle
/// Adressen zusammen. So sperrt ein einzelner Aufrufer die Einrichtung nicht
/// für alle aus.
pub const GESAMT_FEHLVERSUCHE: u32 = 100;
/// Länge des Einmal-Codes: 128 Bit aus dem Zufall, base64url (22 Zeichen).
pub const CODE_BYTES: usize = 16;

/// Der Stand der Ersteinrichtung – einmal beim Start festgelegt.
#[derive(Debug)]
pub(crate) struct Einrichtung {
    /// Hash des Codes; `None` heißt: auf Loopback braucht es keinen.
    code_hash: Option<String>,
    /// Fehlversuche über alle Adressen (die harte Grenze).
    fehlversuche: AtomicU32,
    /// Fehlversuche je Adresse; jede sperrt nur sich selbst.
    je_ip: Mutex<HashMap<String, u32>>,
}

impl Einrichtung {
    /// Beim Start: Code erzeugen, wenn der Server ihn verlangt
    /// ([`crate::Konfig::einrichtung_mit_code`]), und ihn **nur** ins eigene Log
    /// schreiben.
    pub(crate) fn neu(oeffentlich: &str, mit_code: bool) -> Self {
        let leer = Self {
            code_hash: None,
            fehlversuche: AtomicU32::new(0),
            je_ip: Mutex::new(HashMap::new()),
        };
        if !mit_code {
            if !crate::config::ist_loopback_url(oeffentlich) {
                // Kommt hier an, hat die Konfiguration den Fall schon abgelehnt;
                // zur Sicherheit trotzdem ein Code.
                tracing::warn!(
                    "Ersteinrichtung ohne Code bei nicht-lokaler Basis-URL – es gilt trotzdem ein Code"
                );
            } else {
                tracing::info!(
                    "Ersteinrichtung ohne Einmal-Code (nur lokal erreichbar); \
                     die Einrichtung ist {FENSTER_MINUTEN} Minuten nach dem Start offen"
                );
                return leer;
            }
        }
        let code = krypto_code();
        // Die eine Zeile, in der der Code steht – sonst nirgends.
        tracing::info!(
            "Ersteinrichtung: Code für diesen Start: {code} \
             (steht nur hier im Log, gilt {FENSTER_MINUTEN} Minuten) – \
             im Container zeigt ihn `docker logs <container>`"
        );
        Self {
            code_hash: Some(hilfen::hash(&code)),
            fehlversuche: AtomicU32::new(0),
            je_ip: Mutex::new(HashMap::new()),
        }
    }

    /// Nur für Tests: eine Einrichtung mit bekanntem Code.
    #[cfg(test)]
    pub(crate) fn mit_code(code: &str) -> Self {
        Self {
            code_hash: Some(hilfen::hash(code)),
            fehlversuche: AtomicU32::new(0),
            je_ip: Mutex::new(HashMap::new()),
        }
    }

    /// Braucht die Einrichtung den Code aus dem Log?
    pub(crate) fn code_noetig(&self) -> bool {
        self.code_hash.is_some()
    }

    /// Wie viele Codes wurden schon falsch eingegeben (alle Adressen)?
    pub(crate) fn fehlversuche(&self) -> u32 {
        self.fehlversuche.load(Ordering::SeqCst)
    }

    /// Ist die Einrichtung **ganz** zu, weil zu viele Codes falsch waren?
    pub(crate) fn gesperrt(&self) -> bool {
        self.fehlversuche() >= GESAMT_FEHLVERSUCHE
    }

    /// Wie viele Fehlversuche hat diese Adresse schon?
    pub(crate) fn fehlversuche_von(&self, ip: Option<IpAddr>) -> u32 {
        let karte = ip_karte(&self.je_ip);
        *karte.get(&ip_schluessel(ip)).unwrap_or(&0)
    }

    /// Ist die Einrichtung für diese Adresse zu?
    pub(crate) fn gesperrt_fuer(&self, ip: Option<IpAddr>) -> bool {
        self.gesperrt() || self.fehlversuche_von(ip) >= CODE_FEHLVERSUCHE
    }

    /// Der Stand für `/auth/stand` (ohne Adresse: die Gesamtlage).
    pub(crate) fn lage(&self, seit_start: Duration) -> Einrichtungslage {
        if self.gesperrt() || seit_start >= Duration::from_secs(FENSTER_MINUTEN * 60) {
            Einrichtungslage::Geschlossen
        } else {
            Einrichtungslage::Offen
        }
    }

    /// Den Code prüfen: in konstanter Zeit, je Adresse gezählt, global nach
    /// [`GESAMT_FEHLVERSUCHE`] zu.
    pub(crate) fn code_pruefen(&self, code: Option<&str>, ip: Option<IpAddr>) -> Ergebnis<()> {
        let Some(erwartet) = self.code_hash.as_deref() else {
            return Ok(());
        };
        if self.gesperrt() {
            return Err(FehlerAntwort::verboten(
                "zu viele falsche Codes; die Einrichtung ist geschlossen – \
                 Server neu starten",
            ));
        }
        if self.gesperrt_fuer(ip) {
            return Err(FehlerAntwort::neu(
                StatusCode::TOO_MANY_REQUESTS,
                "zu viele falsche Codes von dieser Adresse; bitte später erneut versuchen",
            ));
        }
        let eingegeben = code.unwrap_or_default().trim();
        if eingegeben.is_empty() {
            return Err(FehlerAntwort::ungueltig(
                "der Einmal-Code fehlt; er steht in der Log-Ausgabe des Servers \
                 (im Container: `docker logs <container>`)",
            ));
        }
        if hilfen::gleich_konstant(erwartet, &hilfen::hash(eingegeben)) {
            return Ok(());
        }
        let versuche = self.fehlversuche.fetch_add(1, Ordering::SeqCst) + 1;
        let jetzt_ip = {
            let mut karte = ip_karte(&self.je_ip);
            let zaehler = karte.entry(ip_schluessel(ip)).or_insert(0);
            *zaehler = zaehler.saturating_add(1);
            *zaehler
        };
        if versuche >= GESAMT_FEHLVERSUCHE {
            return Err(FehlerAntwort::verboten(
                "zu viele falsche Codes; die Einrichtung ist geschlossen – \
                 Server neu starten",
            ));
        }
        if jetzt_ip >= CODE_FEHLVERSUCHE {
            return Err(FehlerAntwort::neu(
                StatusCode::TOO_MANY_REQUESTS,
                "zu viele falsche Codes von dieser Adresse; bitte später erneut versuchen",
            ));
        }
        Err(FehlerAntwort::verboten("der Einmal-Code stimmt nicht"))
    }
}

/// Sperre holen, auch wenn ein anderer Aufruf panisch war.
fn ip_karte(
    karte: &Mutex<HashMap<String, u32>>,
) -> std::sync::MutexGuard<'_, HashMap<String, u32>> {
    karte
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
}

/// 128 Bit Zufall als base64url (22 Zeichen).
fn krypto_code() -> String {
    crate::krypto::zufall_b64(CODE_BYTES)
}

// ------------------------------------------------------------------- Wege

/// `GET /auth/stand` (offen, Aufgabe 25): mehr sagt der Server vor der
/// Anmeldung nicht.
pub(crate) async fn stand(
    State(zustand): State<Arc<Zustand>>,
    headers: axum::http::HeaderMap,
) -> Ergebnis<Json<AuthStand>> {
    host_pruefen(&headers, &zustand.konfig.oeffentlich)?;
    let eingerichtet = db(&zustand, |conn| speicher::root_ist_gesetzt(conn)).await?;
    let einrichtung = if eingerichtet {
        None
    } else {
        Some(
            zustand
                .einrichtung
                .lage(zustand.konfig.laufzeit.seit_start()),
        )
    };
    Ok(Json(AuthStand {
        eingerichtet,
        einrichtung,
        code_noetig: zustand.einrichtung.code_noetig(),
        entra: zustand.oidc.is_some(),
    }))
}

/// `POST /auth/einrichten`: den einen Root anlegen und gleich anmelden.
pub(crate) async fn einrichten(
    State(zustand): State<Arc<Zustand>>,
    req: Request,
) -> Ergebnis<Response> {
    let ip = sitzung::aufrufer(&req, &zustand);
    let (teile, rumpf) = req.into_parts();
    host_pruefen(&teile.headers, &zustand.konfig.oeffentlich)?;
    // Ändernde Anfrage: Origin und `X-Quiver: 1` wie in der Verwaltung.
    sitzung::aenderung_pruefen(&teile.headers, &zustand.konfig.oeffentlich)?;
    let bytes = axum::body::to_bytes(rumpf, 64 * 1024)
        .await
        .map_err(|_| FehlerAntwort::ungueltig("Rumpf nicht lesbar"))?;
    let anfrage: Einrichten = serde_json::from_slice(&bytes)
        .map_err(|_| FehlerAntwort::ungueltig("Rumpf ist kein JSON mit email und passwort"))?;

    // Gibt es schon einen Root, ist dieser Weg zu Ende – auch die Seite.
    let gesetzt = db(&zustand, |conn| speicher::root_ist_gesetzt(conn)).await?;
    if gesetzt {
        return Err(FehlerAntwort::nicht_gefunden(
            "dieser Server ist schon eingerichtet; bitte anmelden",
        ));
    }

    let jetzt = zustand.uhr.jetzt();
    if zustand
        .einrichtung
        .lage(zustand.konfig.laufzeit.seit_start())
        == Einrichtungslage::Geschlossen
    {
        return Err(FehlerAntwort::verboten(
            "die Ersteinrichtung ist geschlossen (15 Minuten nach dem Start); \
             Server neu starten",
        ));
    }

    // Der Code wird in konstanter Zeit geprüft; Fehlversuche zählen je Adresse
    // (zehn, dann ist die Einrichtung für diese Adresse zu) und global erst nach
    // hundert. Zusätzlich bremst die Drossel je IP.
    let schluessel = format!("einrichtung:{}", ip_schluessel(ip));
    if let Err(fehler) = zustand
        .einrichtung
        .code_pruefen(anfrage.code.as_deref(), ip)
    {
        let wartezeit = zustand.drossel.fehlschlag(&schluessel, jetzt);
        let akteur = ip_schluessel(ip);
        db(&zustand, move |conn| {
            speicher::pruefspur_fehlschlag(conn, jetzt, &akteur, "Einrichtung: Code")?;
            Ok(())
        })
        .await?;
        if zustand.einrichtung.gesperrt() || zustand.einrichtung.gesperrt_fuer(ip) {
            // Ab jetzt hilft auch der richtige Code nicht mehr.
            return Err(fehler);
        }
        if wartezeit > 0 {
            return Err(FehlerAntwort::neu(
                StatusCode::TOO_MANY_REQUESTS,
                format!("zu viele Versuche; bitte {wartezeit} Sekunden warten"),
            ));
        }
        return Err(fehler);
    }
    zustand.drossel.erfolg(&schluessel);

    let email = email_normalisieren(&anfrage.email);
    if email.is_empty() || email.len() > 254 || !benutzer::email_gueltig(&email) {
        return Err(FehlerAntwort::ungueltig(
            "E-Mail fehlt oder ist nicht gültig",
        ));
    }
    if !(PASSWORT_MIN..=PASSWORT_MAX).contains(&anfrage.passwort.chars().count()) {
        return Err(FehlerAntwort::ungueltig(format!(
            "Passwort muss {PASSWORT_MIN} bis {PASSWORT_MAX} Zeichen haben"
        )));
    }

    // Argon2id wie beim Root. Höchstens zwei Prüfungen laufen gleichzeitig;
    // hier wird gewartet statt abgewiesen – bei gleichzeitigen Aufrufen soll
    // genau einer den Root anlegen und die anderen 409 bekommen, nicht 429.
    let permit = tokio::time::timeout(std::time::Duration::from_secs(20), zustand.argon.acquire())
        .await
        .map_err(|_| {
            FehlerAntwort::neu(
                StatusCode::TOO_MANY_REQUESTS,
                "zu viele Prüfungen; bitte kurz warten",
            )
        })?
        .map_err(|_| FehlerAntwort::intern("Argon2-Sperre ist geschlossen"))?;
    let passwort = anfrage.passwort.clone();
    let hash = tokio::task::spawn_blocking(move || benutzer::passwort_hashen(&passwort))
        .await
        .map_err(|fehler| FehlerAntwort::intern_mit_kennung(&fehler))?
        .map_err(|fehler| FehlerAntwort::intern_mit_kennung(&fehler))?;
    drop(permit);

    // Genau einer gewinnt: Prüfung „kein Root“ und Einfügen in einer Transaktion.
    let angelegt = db(&zustand, {
        let email = email.clone();
        move |conn| speicher::root_einrichten(conn, &email, &hash, jetzt)
    })
    .await?;
    if !angelegt {
        // Ein gleichzeitiger Aufruf war schneller.
        return Err(FehlerAntwort::konflikt(
            "dieser Server ist schon eingerichtet; bitte anmelden",
        ));
    }

    // Der Admin-Schlüssel aus alter Zeit gilt nicht mehr.
    admin_token_entfernen(&zustand.konfig.daten);

    let kennung = speicher::ROOT_OID.to_string();
    let token = db(&zustand, {
        let email = email.clone();
        move |conn| {
            let token = sitzung::sitzung_anlegen(conn, &kennung, jetzt)?;
            speicher::pruefspur_schreiben(
                conn,
                jetzt,
                "einrichtung",
                &email,
                "Root über die Ersteinrichtung angelegt",
            )?;
            Ok(token)
        }
    })
    .await?;

    let benutzer = db(&zustand, {
        let kennung = speicher::ROOT_OID.to_string();
        move |conn| speicher::benutzer_holen(conn, &kennung)
    })
    .await?
    .ok_or_else(|| FehlerAntwort::intern("Root ist nach dem Einrichten verschwunden"))?;

    benutzer::antwort_mit_sitzung(
        &zustand,
        &token,
        &benutzer,
        quiver_protocol::Anmeldeart::Root,
    )
}

/// `<daten>/admin-token` entfernen (Aufgabe 25: der Schlüssel gilt nicht mehr).
fn admin_token_entfernen(daten: &Path) {
    let datei = daten.join("admin-token");
    if datei.exists() {
        match std::fs::remove_file(&datei) {
            Ok(()) => tracing::warn!(
                "{} entfernt: der Admin-Schlüssel gilt seit der Einrichtung nicht mehr",
                datei.display()
            ),
            Err(fehler) => tracing::warn!("{} nicht entfernbar: {fehler}", datei.display()),
        }
    }
}

// --------------------------------------------------------------- Prüfungen

/// `Host` muss exakt die Basis-URL sein (gegen DNS-Rebinding). Der Kopf trägt
/// keinen Schemen: `127.0.0.1:8740` oder `quiver.example.test`.
pub(crate) fn host_pruefen(headers: &axum::http::HeaderMap, oeffentlich: &str) -> Ergebnis<()> {
    let host = headers
        .get(header::HOST)
        .and_then(|wert| wert.to_str().ok())
        .ok_or_else(|| FehlerAntwort::verboten("Host fehlt"))?;
    if !host_passt(host, oeffentlich) {
        return Err(FehlerAntwort::verboten(
            "Host passt nicht zur Basis-URL des Servers",
        ));
    }
    Ok(())
}

/// Passt `Host` (ohne Schemen) zur Basis-URL?
pub(crate) fn host_passt(host: &str, oeffentlich: &str) -> bool {
    let Ok(url) = url::Url::parse(oeffentlich) else {
        return false;
    };
    let Some(url_host) = url.host_str() else {
        return false;
    };
    let erwartet = match url.port() {
        Some(port) => format!("{url_host}:{port}"),
        None => url_host.to_string(),
    };
    host.trim().eq_ignore_ascii_case(&erwartet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_muss_exakt_passen() {
        assert!(host_passt("127.0.0.1:8740", "http://127.0.0.1:8740"));
        assert!(host_passt("127.0.0.1:8740", "http://127.0.0.1:8740/"));
        assert!(host_passt(
            "quiver.example.test",
            "https://quiver.example.test"
        ));
        assert!(host_passt(
            "QUIVER.example.test",
            "https://quiver.example.test"
        ));
        assert!(!host_passt("127.0.0.1:8741", "http://127.0.0.1:8740"));
        assert!(!host_passt(
            "evil.example.test",
            "https://quiver.example.test"
        ));
        assert!(!host_passt(
            "quiver.example.test:8443",
            "https://quiver.example.test"
        ));
        assert!(!host_passt("", "http://127.0.0.1:8740"));
    }

    #[test]
    fn fenster_haengt_an_der_laufzeit() {
        let einrichtung = Einrichtung::neu("http://127.0.0.1:8740", false);
        assert!(!einrichtung.code_noetig(), "Loopback braucht keinen Code");
        assert_eq!(
            einrichtung.lage(Duration::from_secs(FENSTER_MINUTEN * 60 - 1)),
            Einrichtungslage::Offen
        );
        assert_eq!(
            einrichtung.lage(Duration::from_secs(FENSTER_MINUTEN * 60)),
            Einrichtungslage::Geschlossen
        );
        assert!(einrichtung.code_pruefen(None, None).is_ok());
    }

    /// Fehlversuche zählen je Adresse; gesperrt wird zuerst nur diese.
    #[test]
    fn code_ist_pflicht_und_je_adresse_zu() {
        let einrichtung = Einrichtung::neu("https://quiver.example.test", true);
        assert!(einrichtung.code_noetig());
        let a: IpAddr = "203.0.113.9".parse().unwrap();
        let b: IpAddr = "203.0.113.10".parse().unwrap();
        assert!(einrichtung.code_pruefen(None, Some(a)).is_err());
        for _ in 0..CODE_FEHLVERSUCHE {
            let _ = einrichtung.code_pruefen(Some("falsch"), Some(a));
        }
        assert!(einrichtung.gesperrt_fuer(Some(a)));
        assert!(
            !einrichtung.gesperrt_fuer(Some(b)),
            "eine andere Adresse ist nicht mitgesperrt"
        );
        assert!(!einrichtung.gesperrt(), "global noch offen");
        // Für die gesperrte Adresse hilft auch der richtige Code nicht mehr.
        assert!(einrichtung.code_pruefen(None, Some(a)).is_err());
    }

    /// Global ist die Einrichtung erst nach hundert Fehlversuchen zu.
    #[test]
    fn global_erst_nach_hundert() {
        let einrichtung = Einrichtung::mit_code("richtiger-code");
        for i in 0..GESAMT_FEHLVERSUCHE {
            let ip: IpAddr = format!("198.51.100.{}", (i % 250) + 1).parse().unwrap();
            let _ = einrichtung.code_pruefen(Some("falsch"), Some(ip));
        }
        assert!(einrichtung.gesperrt());
        assert_eq!(
            einrichtung.lage(Duration::from_secs(0)),
            Einrichtungslage::Geschlossen
        );
        assert!(einrichtung.code_pruefen(None, None).is_err());
    }

    #[test]
    fn richtiger_code_oeffnet() {
        let einrichtung = Einrichtung::mit_code("richtiger-code");
        assert!(einrichtung.code_pruefen(Some("falsch"), None).is_err());
        assert!(einrichtung
            .code_pruefen(Some("richtiger-code"), None)
            .is_ok());
        assert_eq!(einrichtung.fehlversuche(), 1, "richtig zählt nicht weiter");
    }

    #[test]
    fn code_hat_128_bit() {
        let code = krypto_code();
        assert_eq!(code.len(), 22, "16 Byte base64url ohne Füllzeichen");
    }
}
