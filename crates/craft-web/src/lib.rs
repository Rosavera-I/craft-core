//! # CRAFT Web Dashboard
//!
//! Visual harness composition and memory inspection interface for CRAFT.
//!
//! ## Architecture
//! - Axum-based REST API server
//! - WebSocket for real-time composition validation
//! - Leptos WASM frontend (optional, in craft-web-ui crate)

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod api;
pub mod error;
pub mod memory;
pub mod server;
pub mod websocket;

pub use error::{WebError, WebResult};
pub use server::{create_app, run_server};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// API response wrapper for consistent JSON responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(ApiError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

/// Harness information for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub authors: Vec<String>,
    pub installed_at: String,
}

/// Composition plan for the visual canvas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionPlanDto {
    pub strategy: String,
    pub harnesses: Vec<CompositionHarnessDto>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionHarnessDto {
    pub name: String,
    pub version: String,
    pub source: String,
    pub path: String,
    pub prompt_path: String,
    pub memory_schema_path: String,
    pub mcp_tools_path: String,
    pub tdd_validators_path: String,
}

/// Memory fact for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFactDto {
    pub scope: String,
    pub key: String,
    pub value: String,
    pub created_at: i64,
}

/// Search result for memory queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub facts: Vec<MemoryFactDto>,
    pub total: usize,
}

/// Validation request/response for WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRequest {
    pub harness_names: Vec<String>,
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub status: ValidationStatus,
    pub message: String,
    pub details: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidationStatus {
    Valid,
    Warning,
    Error,
}

/// Runtime status for the dashboard monitor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub active: bool,
    pub current_harness: Option<String>,
    pub last_activity: Option<String>,
    pub stats: RuntimeStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeStats {
    pub memory_facts_count: usize,
    pub installed_harnesses: usize,
    pub compositions_created: usize,
}
