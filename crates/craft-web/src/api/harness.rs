//! Harness registry API handlers

use axum::{
    extract::{Extension, Path, Query},
    response::Json,
};
use craft_core::{HarnessRegistry, InstalledHarness};
use craft_manifest::load_manifest;
use serde::Deserialize;
use std::sync::Arc;

use crate::HarnessInfo;
use crate::api::json_response;
use crate::error::{WebError, WebResult};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

/// GET /api/v1/harnesses - List all installed harnesses
pub async fn list_harnesses(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> WebResult<Json<crate::ApiResponse<Vec<HarnessInfo>>>> {
    let registry = HarnessRegistry::open(state.home.registry_path())?;
    let harnesses = registry.list()?;

    let infos: Vec<HarnessInfo> = harnesses
        .into_iter()
        .map(installed_to_info)
        .skip(query.offset.unwrap_or(0))
        .take(query.limit.unwrap_or(100))
        .collect();

    Ok(json_response(infos))
}

/// GET /api/v1/harnesses/{name} - Get a specific harness
pub async fn get_harness(
    Extension(state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> WebResult<Json<crate::ApiResponse<HarnessInfo>>> {
    let registry = HarnessRegistry::open(state.home.registry_path())?;
    let harness = registry
        .info(&name)
        .map_err(|_| WebError::NotFound(format!("harness `{name}` not found")))?;

    Ok(json_response(installed_to_info(harness)))
}

/// GET /api/v1/harnesses/{name}/versions - List all versions of a harness
pub async fn list_harness_versions(
    Extension(state): Extension<Arc<AppState>>,
    Path(name): Path<String>,
) -> WebResult<Json<crate::ApiResponse<Vec<HarnessInfo>>>> {
    let registry = HarnessRegistry::open(state.home.registry_path())?;
    let harnesses = registry.list_versions(&name)?;

    let infos: Vec<HarnessInfo> = harnesses.into_iter().map(installed_to_info).collect();

    Ok(json_response(infos))
}

fn installed_to_info(installed: InstalledHarness) -> HarnessInfo {
    // Try to load the manifest to get description and authors
    let manifest = load_manifest(installed.path.join("craft.toml"))
        .ok()
        .map(|m| m.harness);

    HarnessInfo {
        name: installed.name,
        version: installed.version,
        description: manifest
            .as_ref()
            .map(|h| h.description.clone())
            .unwrap_or_default(),
        source: installed.source,
        authors: manifest
            .as_ref()
            .map(|h| h.authors.clone())
            .unwrap_or_default(),
        installed_at: "unknown".to_string(), // Could be read from database
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_info_serialization() {
        let info = HarnessInfo {
            name: "test-harness".to_string(),
            version: "1.0.0".to_string(),
            description: "Test harness".to_string(),
            source: "github:test/repo".to_string(),
            authors: vec!["Test Author".to_string()],
            installed_at: "2024-01-01".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-harness"));
        assert!(json.contains("1.0.0"));
    }
}
