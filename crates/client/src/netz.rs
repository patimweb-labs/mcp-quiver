// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Grenzen für alles, was über das Netz kommt (Paket 3).
//!
//! Der Client spricht mit **einem** Server, den der User eingebunden hat, und
//! mit entfernten MCP-Einträgen. Beides kann hängen, tröpfeln oder riesig
//! antworten. Deshalb steht hier an einer Stelle:
//!
//! - **Fristen**: Verbinden 10 s, eine Antwort 30 s, ein Strom 60 s ohne ein
//!   einziges Byte.
//! - **Größen**: eine Antwort höchstens 4 MB, der Soll-Zustand höchstens 8 MB,
//!   eine SSE-Zeile höchstens 64 KB.
//! - **Keine Weiterleitungen**: ein Geräte-Token gehört zu genau einer Adresse.
//!   Wohin eine 30x-Antwort führt, weiß der Client nicht – also folgt er ihr
//!   nicht.
//!
//! Der Rumpf wird **beim Lesen** begrenzt: sobald mehr als die erlaubten Bytes
//! ankommen, bricht das Lesen ab, statt erst am Ende zu zählen. Sonst wäre die
//! Grenze nur eine Behauptung.

use std::time::Duration;

use futures_util::StreamExt;

/// So lange darf der Verbindungsaufbau höchstens dauern.
pub const VERBINDEN_FRIST: Duration = Duration::from_secs(10);
/// So lange darf eine Antwort höchstens dauern.
pub const ANTWORT_FRIST: Duration = Duration::from_secs(30);
/// So lange darf ein Strom (SSE) ohne ein Byte schweigen.
pub const STROM_FRIST: Duration = Duration::from_secs(60);
/// So groß darf eine Antwort höchstens sein.
pub const ANTWORT_BYTES: usize = 4 * 1024 * 1024;
/// So groß darf der Soll-Zustand höchstens sein (viele Einträge, viele Werte).
pub const SOLL_BYTES: usize = 8 * 1024 * 1024;

/// Der HTTP-Client des Clients: Fristen gesetzt, keine Weiterleitungen.
///
/// Kein Gesamt-Timeout am Client: der Ereignis-Strom (`text/event-stream`) ist
/// Absicht offen und lebt von seiner Stillstands-Frist. Jede **einzelne**
/// Anfrage bekommt ihre Frist über [`frist`]; der Strom liest mit
/// [`STROM_FRIST`] ohne Byte.
///
/// `redirect::Policy::none()` ist Absicht: eine Weiterleitung könnte ein
/// Geräte-Token an einen fremden Host tragen. Der Server hat keine.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(VERBINDEN_FRIST)
        .read_timeout(STROM_FRIST)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Eine Anfrage mit der Frist einer Antwort versehen (30 s).
pub fn frist(anfrage: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    anfrage.timeout(ANTWORT_FRIST)
}

/// Ein Rumpf, der höchstens `grenze` Bytes groß sein darf.
///
/// Gelesen wird in Stücken; sobald die Summe die Grenze überschreitet, endet
/// der Aufruf mit einem verständlichen Fehler. Es wird nie mehr Speicher
/// belegt als die Grenze.
pub async fn rumpf_begrenzt(antwort: reqwest::Response, grenze: usize) -> anyhow::Result<Vec<u8>> {
    if let Some(laenge) = antwort.content_length() {
        if laenge > grenze as u64 {
            anyhow::bail!(
                "Die Antwort ist zu groß ({} Bytes, erlaubt sind {grenze})",
                laenge
            );
        }
    }
    let mut rumpf = Vec::new();
    let mut strom = antwort.bytes_stream();
    while let Some(stueck) = strom.next().await {
        let stueck = stueck?;
        if rumpf.len().saturating_add(stueck.len()) > grenze {
            anyhow::bail!("Die Antwort ist zu groß (mehr als {grenze} Bytes)");
        }
        rumpf.extend_from_slice(&stueck);
    }
    Ok(rumpf)
}

/// Ein Rumpf als JSON, höchstens `grenze` Bytes.
pub async fn json_begrenzt<T: serde::de::DeserializeOwned>(
    antwort: reqwest::Response,
    grenze: usize,
) -> anyhow::Result<T> {
    let rumpf = rumpf_begrenzt(antwort, grenze).await?;
    serde_json::from_slice(&rumpf)
        .map_err(|fehler| anyhow::anyhow!("Antwort nicht lesbar: {fehler}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn die_grenzen_stehen_fest() {
        assert_eq!(VERBINDEN_FRIST, Duration::from_secs(10));
        assert_eq!(ANTWORT_FRIST, Duration::from_secs(30));
        assert_eq!(STROM_FRIST, Duration::from_secs(60));
        assert_eq!(ANTWORT_BYTES, 4 * 1024 * 1024);
        assert_eq!(SOLL_BYTES, 8 * 1024 * 1024);
        assert_eq!(crate::sse::ZEILE_MAX, 64 * 1024);
    }

    /// Eine Antwort wird beim Lesen begrenzt: mehr als erlaubt endet mit einem
    /// Fehler, statt erst am Ende zu zählen.
    #[tokio::test]
    async fn eine_zu_grosse_antwort_wird_beim_lesen_abgebrochen() {
        use axum::routing::get;
        let router = axum::Router::new()
            .route("/gross", get(|| async { "x".repeat(5 * 1024 * 1024) }))
            .route(
                "/weiter",
                get(|| async {
                    (
                        axum::http::StatusCode::FOUND,
                        [(axum::http::header::LOCATION, "http://127.0.0.1:1/ziel")],
                        "",
                    )
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Port");
        let adresse = listener.local_addr().expect("Adresse");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let client = client();
        let antwort = client
            .get(format!("http://{adresse}/gross"))
            .send()
            .await
            .expect("Antwort");
        let fehler = rumpf_begrenzt(antwort, ANTWORT_BYTES)
            .await
            .expect_err("zu groß");
        assert!(fehler.to_string().contains("zu groß"), "{fehler}");

        // Eine Weiterleitung wird nicht verfolgt: das Geräte-Token bleibt bei
        // seinem Server.
        let antwort = client
            .get(format!("http://{adresse}/weiter"))
            .send()
            .await
            .expect("Antwort");
        assert_eq!(antwort.status(), reqwest::StatusCode::FOUND);
    }

    /// Ein Soll-Zustand darf größer sein als eine normale Antwort (8 statt
    /// 4 MB) – aber nicht unbegrenzt.
    #[test]
    fn der_soll_zustand_hat_eine_groessere_grenze() {
        assert_eq!(SOLL_BYTES, 2 * ANTWORT_BYTES);
    }
}
