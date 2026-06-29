//! Error types for craft-web

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use craft_core::CraftError;
use craft_memory::MemoryError;
use serde_json::json;
use std::fmt;
use tracing::error;

pub type WebResult<T> = std::result::Result<T, WebError>;

#[derive(Debug)]
pub enum WebError {
    Config(String),
    Craft(CraftError),
    Memory(MemoryError),
    Registry(String),
    Validation(String),
    WebSocket(String),
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl fmt::Display for WebError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(msg)
            | Self::Registry(msg)
            | Self::Validation(msg)
            | Self::WebSocket(msg)
            | Self::NotFound(msg)
            | Self::BadRequest(msg)
            | Self::Internal(msg) => write!(f, "{msg}"),
            Self::Craft(err) => write!(f, "{err}"),
            Self::Memory(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for WebError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Craft(err) => Some(err),
            Self::Memory(err) => Some(err),
            _ => None,
        }
    }
}

impl WebError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Config(_) => "config",
            Self::Craft(_) => "craft",
            Self::Memory(_) => "memory",
            Self::Registry(_) => "registry",
            Self::Validation(_) => "validation",
            Self::WebSocket(_) => "websocket",
            Self::NotFound(_) => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Internal(_) => "internal",
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) | Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Config(_) | Self::Registry(_) => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        self.to_string()
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        error!("Request failed: {}", self);

        let status = self.status_code();
        let body = Json(json!({
            "success": false,
            "error": {
                "code": self.code(),
                "message": self.message(),
            },
            "data": null,
        }));

        (status, body).into_response()
    }
}

impl From<CraftError> for WebError {
    fn from(err: CraftError) -> Self {
        Self::Craft(err)
    }
}

impl From<MemoryError> for WebError {
    fn from(err: MemoryError) -> Self {
        Self::Memory(err)
    }
}

impl From<std::io::Error> for WebError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(format!("IO error: {err}"))
    }
}

impl From<serde_json::Error> for WebError {
    fn from(err: serde_json::Error) -> Self {
        Self::BadRequest(format!("JSON error: {err}"))
    }
}

impl From<axum::extract::rejection::JsonRejection> for WebError {
    fn from(rejection: axum::extract::rejection::JsonRejection) -> Self {
        Self::BadRequest(format!("Invalid JSON: {rejection}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(WebError::NotFound("test".to_string()).code(), "not_found");
        assert_eq!(
            WebError::BadRequest("test".to_string()).code(),
            "bad_request"
        );
        assert_eq!(WebError::Internal("test".to_string()).code(), "internal");
    }

    #[test]
    fn error_display_includes_message() {
        let err = WebError::NotFound("harness missing".to_string());
        assert!(err.to_string().contains("harness missing"));
    }
}
