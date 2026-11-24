// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Fehler als [`quiver_protocol::Fehler`] mit passendem Status, plus ein
//! Rumpf-Extraktor, der kaputtes JSON ebenfalls so beantwortet.

use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use quiver_protocol::AnmeldeGrund;
use serde::de::DeserializeOwned;

/// Ein Fehler, wie ihn der Vertrag vorsieht: Status plus [`quiver_protocol::Fehler`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FehlerAntwort {
    pub status: StatusCode,
    pub text: String,
    /// Maschinenlesbarer Grund bei Geräte-Antworten (Aufgabe 20).
    pub grund: Option<AnmeldeGrund>,
}

impl FehlerAntwort {
    pub fn neu(status: StatusCode, text: impl Into<String>) -> Self {
        Self {
            status,
            text: text.into(),
            grund: None,
        }
    }

    /// Ein Fehler mit maschinenlesbarem Grund für das Gerät.
    pub fn mit_grund(status: StatusCode, text: impl Into<String>, grund: AnmeldeGrund) -> Self {
        Self {
            status,
            text: text.into(),
            grund: Some(grund),
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

    /// 408 – der Client sendet den Rumpf nicht zu Ende (Zeitlimit).
    pub fn zeitlimit() -> Self {
        Self::neu(
            StatusCode::REQUEST_TIMEOUT,
            "der Rumpf kam nicht rechtzeitig; bitte erneut senden",
        )
    }

    /// 413 – der Rumpf ist größer als die Grenze dieser Route.
    ///
    /// Die Grenze selbst steht in der Route (64 KB, für Einträge 256 KB) und
    /// wird hier nicht wiederholt: die Antwort soll knapp bleiben.
    pub fn zu_gross() -> Self {
        Self::neu(StatusCode::PAYLOAD_TOO_LARGE, "der Rumpf ist zu groß")
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
        let rumpf = Json(quiver_protocol::Fehler {
            fehler: self.text,
            grund: self.grund,
        });
        (self.status, rumpf).into_response()
    }
}

/// Ergebnis eines Aufrufs im Vertrag.
pub type Ergebnis<T> = Result<T, FehlerAntwort>;

/// Steckt in der Ursachenkette ein Fehler vom Typ `T`?
///
/// Gebraucht für die beiden Fälle, die ein Extraktor nicht selbst
/// unterscheiden kann: Rumpf zu groß (413) und Rumpf zu langsam (408).
pub(crate) fn ursache_ist<T: std::error::Error + 'static>(
    fehler: &(dyn std::error::Error + 'static),
) -> bool {
    let mut quelle = Some(fehler);
    while let Some(jetzt) = quelle {
        if jetzt.is::<T>() {
            return true;
        }
        quelle = jetzt.source();
    }
    false
}

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
            .map_err(|fehler| {
                // Zu groß ist kein Formfehler, sondern 413 (die Grenze steht in
                // `RUMPF_STANDARD` bzw. `RUMPF_EINTRAEGE`).
                if fehler.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    return FehlerAntwort::zu_gross();
                }
                // Ein Client, der den Rumpf nicht zu Ende sendet, bekommt 408 und
                // nicht 400: das Zeitlimit hat gegriffen, der Aufruf ist nicht
                // falsch (er ist nur nicht fertig geworden).
                if ursache_ist::<tower_http::timeout::TimeoutError>(&fehler) {
                    return FehlerAntwort::zeitlimit();
                }
                FehlerAntwort::ungueltig(format!("Rumpf nicht lesbar: {fehler}"))
            })?;
        Ok(Koerper(wert))
    }
}
