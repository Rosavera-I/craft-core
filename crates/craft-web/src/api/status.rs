//! Runtime status API handlers

use axum::{extract::Extension, response::Json};
use craft_core::HarnessRegistry;
use craft_memory::Memory;
use std::sync::Arc;

use crate::api::json_response;
use crate::error::WebResult;
use crate::server::AppState;
use crate::{RuntimeStats, RuntimeStatus};

/// GET /api/v1/status - Get runtime status
pub async fn runtime_status(
    Extension(state): Extension<Arc<AppState>>,
) -> WebResult<Json<crate::ApiResponse<RuntimeStatus>>> {
    // Try to get harness count
    let installed_harnesses =
        if let Ok(registry) = HarnessRegistry::open(state.home.registry_path()) {
            registry.list().map(|l| l.len()).unwrap_or(0)
        } else {
            0
        };

    // Try to get memory facts count
    let memory_facts_count = if let Ok(memory) = Memory::open(state.home.root()) {
        // Inspect global scope to get an approximate count
        memory
            .inspect(&craft_memory::MemoryScope::Global)
            .map(|f| f.len())
            .unwrap_or(0)
    } else {
        0
    };

    let stats = state.runtime_stats.read().await;

    let status = RuntimeStatus {
        active: true,
        current_harness: None, // Could be updated during composition runs
        last_activity: None,   // Could track last request
        stats: RuntimeStats {
            memory_facts_count: memory_facts_count
                + state.runtime_stats.read().await.validations_run, // Approximate
            installed_harnesses,
            compositions_created: stats.compositions_created,
        },
    };

    Ok(json_response(status))
}
