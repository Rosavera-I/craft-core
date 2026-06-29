//! Memory service API handlers

use axum::{
    Json as AxumJson,
    extract::{Extension, Path, Query},
    response::Json,
};
use craft_memory::{Memory, MemoryScope, MemoryStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::json_response;
use crate::error::{WebError, WebResult};
use crate::server::AppState;
use crate::{MemoryFactDto, MemorySearchResult};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryListQuery {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFactRequest {
    pub scope: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct CreateFactResponse {
    pub scope: String,
    pub key: String,
    pub value: String,
    pub created_at: i64,
}

/// GET /api/v1/memory/search - Search memory facts using FTS
pub async fn search_memory(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> WebResult<Json<crate::ApiResponse<MemorySearchResult>>> {
    if query.q.trim().is_empty() {
        return Err(WebError::BadRequest(
            "search query 'q' is required".to_string(),
        ));
    }

    let memory = Memory::open(state.home.root())?;

    let scopes: Vec<MemoryScope> = if let Some(s) = &query.scope {
        vec![MemoryScope::parse(s).map_err(|_| WebError::BadRequest("invalid scope".to_string()))?]
    } else {
        vec![
            MemoryScope::Global,
            MemoryScope::User,
            MemoryScope::Project,
            MemoryScope::Session,
        ]
    };

    let facts = memory.search(&query.q, &scopes)?;

    let limit = query.limit.unwrap_or(50);
    let total = facts.len();

    let dto: Vec<MemoryFactDto> = facts
        .into_iter()
        .take(limit)
        .map(|f| MemoryFactDto {
            scope: f.scope.storage_key(),
            key: f.key,
            value: f.value,
            created_at: f.created_at,
        })
        .collect();

    Ok(json_response(MemorySearchResult { facts: dto, total }))
}

/// GET /api/v1/memory/facts - List all memory facts (paginated)
pub async fn list_memory_facts(
    Extension(state): Extension<Arc<AppState>>,
    Query(query): Query<MemoryListQuery>,
) -> WebResult<Json<crate::ApiResponse<Vec<MemoryFactDto>>>> {
    let memory = Memory::open(state.home.root())?;

    // If no query, list facts from a specific scope or all scopes
    let facts = if let Some(scope_str) = &query.scope {
        let scope = MemoryScope::parse(scope_str)
            .map_err(|_| WebError::BadRequest("invalid scope".to_string()))?;
        memory.inspect(&scope)?
    } else {
        // Search with empty query returns first 50 across all scopes
        memory.search(
            "",
            &[
                MemoryScope::Global,
                MemoryScope::User,
                MemoryScope::Project,
                MemoryScope::Session,
            ],
        )?
    };

    let limit = query.limit.unwrap_or(50);

    let dto: Vec<MemoryFactDto> = facts
        .into_iter()
        .take(limit)
        .map(|f| MemoryFactDto {
            scope: f.scope.storage_key(),
            key: f.key,
            value: f.value,
            created_at: f.created_at,
        })
        .collect();

    Ok(json_response(dto))
}

/// POST /api/v1/memory/facts - Create a new memory fact
pub async fn create_memory_fact(
    Extension(state): Extension<Arc<AppState>>,
    AxumJson(req): AxumJson<CreateFactRequest>,
) -> WebResult<Json<crate::ApiResponse<CreateFactResponse>>> {
    let scope = MemoryScope::parse(&req.scope)
        .map_err(|_| WebError::BadRequest("invalid scope".to_string()))?;

    if req.key.trim().is_empty() {
        return Err(WebError::BadRequest("key is required".to_string()));
    }

    let memory = Memory::open(state.home.root())?;
    let fact = memory.save_fact(scope, &req.key, &req.value)?;

    let response = CreateFactResponse {
        scope: fact.scope.storage_key(),
        key: fact.key,
        value: fact.value,
        created_at: fact.created_at,
    };

    Ok(json_response(response))
}

/// GET /api/v1/memory/scope/{scope} - Get all facts for a specific scope
pub async fn get_memory_scope(
    Extension(state): Extension<Arc<AppState>>,
    Path(scope): Path<String>,
    Query(query): Query<MemoryListQuery>,
) -> WebResult<Json<crate::ApiResponse<Vec<MemoryFactDto>>>> {
    let memory_scope = MemoryScope::parse(&scope)
        .map_err(|_| WebError::BadRequest("invalid scope".to_string()))?;

    let memory = Memory::open(state.home.root())?;
    let facts = memory.inspect(&memory_scope)?;

    let limit = query.limit.unwrap_or(100);

    let dto: Vec<MemoryFactDto> = facts
        .into_iter()
        .take(limit)
        .map(|f| MemoryFactDto {
            scope: f.scope.storage_key(),
            key: f.key,
            value: f.value,
            created_at: f.created_at,
        })
        .collect();

    Ok(json_response(dto))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_parsing() {
        // Verify that the SearchQuery struct can be deserialized from URL params
        let query = "q=test&scope=project&limit=10";
        let parsed: SearchQuery = serde_urlencoded::from_str(query)
            .map_err(|e| {
                eprintln!("Parse error: {:?}", e);
                e
            })
            .unwrap();
        assert_eq!(parsed.q, "test");
        assert_eq!(parsed.scope, Some("project".to_string()));
        assert_eq!(parsed.limit, Some(10));
    }

    #[test]
    fn create_fact_request_deserialization() {
        let json = r#"{
            "scope": "project",
            "key": "language",
            "value": "rust"
        }"#;

        let req: CreateFactRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.scope, "project");
        assert_eq!(req.key, "language");
        assert_eq!(req.value, "rust");
    }
}
