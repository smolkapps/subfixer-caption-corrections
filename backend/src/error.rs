//! Error type shared across the SubFixer library and surfaced as HTTP status
//! codes by the API layer.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum SubFixerError {
    /// Caller-supplied input was invalid (bad URL, bad timestamp, empty body).
    #[error("validation error: {0}")]
    Validation(String),

    /// The requested resource does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// A correction that does not actually change anything was submitted.
    #[error("no-op correction: corrected text is identical to the original")]
    NoOpCorrection,

    /// Underlying datastore failure.
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<rusqlite::Error> for SubFixerError {
    fn from(e: rusqlite::Error) -> Self {
        SubFixerError::Storage(e.to_string())
    }
}

impl IntoResponse for SubFixerError {
    fn into_response(self) -> Response {
        let status = match &self {
            SubFixerError::Validation(_) | SubFixerError::NoOpCorrection => StatusCode::BAD_REQUEST,
            SubFixerError::NotFound(_) => StatusCode::NOT_FOUND,
            SubFixerError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({ "error": self.to_string() }));
        (status, body).into_response()
    }
}
