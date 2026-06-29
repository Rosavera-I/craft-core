//! Error types for the CRAFT Registry

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

/// Result type alias for registry operations
pub type RegistryResult<T> = Result<T, RegistryError>;

/// Main error type for the CRAFT Registry
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Git operation error
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    /// Authentication/authorization error
    #[error("Authentication error: {0}")]
    Auth(String),

    /// JWT error
    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Not found error
    #[error("Not found: {0}")]
    NotFound(String),

    /// Conflict error (duplicate, already exists)
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded. Try again in {0} seconds")]
    RateLimited(u64),

    /// Package error (invalid format, too large, etc.)
    #[error("Package error: {0}")]
    Package(String),

    /// Semver parsing error
    #[error("Version error: {0}")]
    Version(#[from] semver::Error),

    /// HTTP client error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Internal server error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl RegistryError {
    /// Get the HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            RegistryError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RegistryError::Git(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RegistryError::Auth(_) => StatusCode::UNAUTHORIZED,
            RegistryError::Jwt(_) => StatusCode::UNAUTHORIZED,
            RegistryError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RegistryError::Validation(_) => StatusCode::BAD_REQUEST,
            RegistryError::NotFound(_) => StatusCode::NOT_FOUND,
            RegistryError::Conflict(_) => StatusCode::CONFLICT,
            RegistryError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            RegistryError::Package(_) => StatusCode::BAD_REQUEST,
            RegistryError::Version(_) => StatusCode::BAD_REQUEST,
            RegistryError::Http(_) => StatusCode::BAD_GATEWAY,
            RegistryError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RegistryError::Serialization(_) => StatusCode::BAD_REQUEST,
            RegistryError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RegistryError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RegistryError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
        }
    }

    /// Get the error code string for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            RegistryError::Database(_) => "database_error",
            RegistryError::Git(_) => "git_error",
            RegistryError::Auth(_) => "auth_error",
            RegistryError::Jwt(_) => "jwt_error",
            RegistryError::Storage(_) => "storage_error",
            RegistryError::Validation(_) => "validation_error",
            RegistryError::NotFound(_) => "not_found",
            RegistryError::Conflict(_) => "conflict",
            RegistryError::RateLimited(_) => "rate_limited",
            RegistryError::Package(_) => "package_error",
            RegistryError::Version(_) => "version_error",
            RegistryError::Http(_) => "http_error",
            RegistryError::Io(_) => "io_error",
            RegistryError::Serialization(_) => "serialization_error",
            RegistryError::Config(_) => "config_error",
            RegistryError::Internal(_) => "internal_error",
            RegistryError::NotImplemented(_) => "not_implemented",
        }
    }
}

impl IntoResponse for RegistryError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_code = self.error_code();
        let message = self.to_string();

        let body = Json(json!({
            "error": {
                "code": error_code,
                "message": message,
                "status": status.as_u16(),
            }
        }));

        (status, body).into_response()
    }
}

/// API error response structure
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub status: u16,
}

impl From<&RegistryError> for ApiError {
    fn from(err: &RegistryError) -> Self {
        Self {
            code: err.error_code().to_string(),
            message: err.to_string(),
            status: err.status_code().as_u16(),
        }
    }
}

/// Helper trait for adding context to results
pub trait Context<T> {
    fn context(self, msg: impl Into<String>) -> RegistryResult<T>;
}

impl<T> Context<T> for Option<T> {
    fn context(self, msg: impl Into<String>) -> RegistryResult<T> {
        self.ok_or_else(|| RegistryError::NotFound(msg.into()))
    }
}
