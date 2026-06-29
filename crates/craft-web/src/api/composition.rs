//! Composition API handlers

use axum::{Json as AxumJson, extract::Extension, response::Json};
use craft_core::{ConflictStrategy, HarnessRegistry, plan_composition};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use crate::api::json_response;
use crate::error::{WebError, WebResult};
use crate::server::AppState;
use crate::{CompositionHarnessDto, CompositionPlanDto};

#[derive(Debug, Deserialize)]
pub struct ComposeRequest {
    pub harness_names: Vec<String>,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub output_path: Option<PathBuf>,
}

fn default_strategy() -> String {
    "ordered-merge".to_string()
}

#[derive(Debug, Serialize)]
pub struct ComposeResponse {
    pub output_path: String,
    pub warnings: Vec<String>,
}

/// POST /api/v1/compose/plan - Preview composition without writing
pub async fn compose_plan(
    Extension(state): Extension<Arc<AppState>>,
    AxumJson(req): AxumJson<ComposeRequest>,
) -> WebResult<Json<crate::ApiResponse<CompositionPlanDto>>> {
    if req.harness_names.is_empty() {
        return Err(WebError::BadRequest(
            "at least one harness name required".to_string(),
        ));
    }

    let registry = HarnessRegistry::open(state.home.registry_path())?;

    let strategy =
        ConflictStrategy::from_string(&req.strategy).unwrap_or(ConflictStrategy::OrderedMerge);

    let plan = plan_composition(&registry, &req.harness_names, strategy)?;

    let dto = CompositionPlanDto {
        strategy: strategy.as_str().to_string(),
        harnesses: plan
            .harnesses
            .into_iter()
            .map(|h| CompositionHarnessDto {
                name: h.name,
                version: h.version,
                source: h.source,
                path: h.path.to_string_lossy().to_string(),
                prompt_path: h.prompt_path.to_string_lossy().to_string(),
                memory_schema_path: h.memory_schema_path.to_string_lossy().to_string(),
                mcp_tools_path: h.mcp_tools_path.to_string_lossy().to_string(),
                tdd_validators_path: h.tdd_validators_path.to_string_lossy().to_string(),
            })
            .collect(),
        warnings: plan.warnings,
    };

    Ok(json_response(dto))
}

/// POST /api/v1/compose - Compose harnesses and write output
pub async fn compose_harnesses_endpoint(
    Extension(state): Extension<Arc<AppState>>,
    AxumJson(req): AxumJson<ComposeRequest>,
) -> WebResult<Json<crate::ApiResponse<ComposeResponse>>> {
    if req.harness_names.is_empty() {
        return Err(WebError::BadRequest(
            "at least one harness name required".to_string(),
        ));
    }

    let registry = HarnessRegistry::open(state.home.registry_path())?;

    let strategy =
        ConflictStrategy::from_string(&req.strategy).unwrap_or(ConflictStrategy::OrderedMerge);

    let output_path = req
        .output_path
        .unwrap_or_else(|| std::env::temp_dir().join("craft.compose.toml"));

    let result =
        craft_core::compose_harnesses(&registry, &req.harness_names, &output_path, strategy)?;

    state.increment_compositions().await;

    let response = ComposeResponse {
        output_path: result.output_path.to_string_lossy().to_string(),
        warnings: result.warnings,
    };

    Ok(json_response(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_request_deserialization() {
        let json = r#"{
            "harness_names": ["harness1", "harness2"],
            "strategy": "merge",
            "output_path": "/tmp/output.toml"
        }"#;

        let req: ComposeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.harness_names, vec!["harness1", "harness2"]);
        assert_eq!(req.strategy, "merge");
        assert_eq!(req.output_path, Some(PathBuf::from("/tmp/output.toml")));
    }

    #[test]
    fn compose_request_defaults() {
        let json = r#"{"harness_names": ["harness1"]}"#;

        let req: ComposeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.harness_names, vec!["harness1"]);
        assert_eq!(req.strategy, "ordered-merge");
        assert_eq!(req.output_path, None);
    }
}
