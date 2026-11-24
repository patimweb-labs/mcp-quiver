// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Fehler als [`quiver_protocol::Fehler`] mit passendem Status, plus ein
//! Rumpf-Extraktor, der kaputtes JSON ebenfalls so beantwortet.

use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;

/// Ein Fehler, wie ihn der Vertrag vorsieht: Status plus [`quiver_protocol::Fehler`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FehlerAntwort {
    pub status: StatusCode,
    pub text: String,
}

impl FehlerAntwort {
    pub fn neu(status: StatusCode, text: impl Into<String>) -> Self {
        Self {
            status,
            text: text.into(),
        }
    }

    /// 400 – der Aufruf passt nicht zum Vertrag.
    pub fn ungueltig(text: impl Into<String>) -> Self {
        Self::neu(StatusCode::BAD_REQUEST, text)
    }

    /// 401 – kein oder falsches Token.
    pub fn nicht_angemeldet(text: impl Into<String>) -> Self {
        Self::neu(StatusCode::UNAUTHORIZED, text)
    }

    /// 403 – Token passt, aber das Gerät ist gesperrt.
    pub fn verboten(text: impl Into<String>) -> Self {
        Self::neu(StatusCode::FORBIDDEN, text)
    }

    /// 404 – gibt es nicht.
    pub fn nicht_gefunden(text: impl Into<String>) -> Self {
        Self::neu(StatusCode::NOT_FOUND, text)
    }

    /// 409 – der Löschversuch trifft etwas, das noch gebraucht wird.
    pub fn konflikt(text: impl Into<String>) -> Self {
        Self::neu(StatusCode::CONFLICT, text)
    }

    /// 410 – der Code ist abgelaufen.
    pub fn abgelaufen(text: impl Into<String>) -> Self {
        Self::neu(StatusCode::GONE, text)
    }

    /// 500 – der Server konnte nicht, was er sollte.
    pub fn intern(text: impl Into<String>) -> Self {
        Self::neu(StatusCode::INTERNAL_SERVER_ERROR, text)
    }

    /// 500 aus einem beliebigen Fehler; die Meldung geht an den Aufrufer.
    pub fn aus_fehler(fehler: &anyhow::Error) -> Self {
        Self::intern(format!("{fehler:#}"))
    }
}

impl IntoResponse for FehlerAntwort {
    fn into_response(self) -> Response {
        let rumpf = Json(quiver_protocol::Fehler { fehler: self.text });
        (self.status, rumpf).into_response()
    }
}

/// Ergebnis eines Aufrufs im Vertrag.
pub type Ergebnis<T> = Result<T, FehlerAntwort>;

/// JSON-Rumpf, dessen Ablehnung als [`FehlerAntwort`] herauskommt.
///
/// `axum::Json` würde bei kaputtem JSON 422 mit Text beantworten; der Vertrag
/// verlangt [`quiver_protocol::Fehler`].
pub struct Koerper<T>(pub T);

impl<T, S> FromRequest<S> for Koerper<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = FehlerAntwort;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(wert) = Json::<T>::from_request(req, state)
            .await
            .map_err(|fehler| FehlerAntwort::ungueltig(format!("Rumpf nicht lesbar: {fehler}")))?;
        Ok(Koerper(wert))
    }
}
