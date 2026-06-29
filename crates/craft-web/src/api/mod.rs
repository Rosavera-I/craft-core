//! HTTP API handlers

mod composition;
mod harness;
mod memory;
mod status;

pub use composition::*;
pub use harness::*;
pub use memory::*;
pub use status::*;

use axum::response::Json;

/// Common response wrapper for consistent JSON structure
pub fn json_response<T: serde::Serialize>(data: T) -> Json<crate::ApiResponse<T>> {
    Json(crate::ApiResponse::success(data))
}
