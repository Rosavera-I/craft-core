//! Access token request handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::{AuthService, AuthUser, TokenScope},
    db::*,
    error::RegistryResult,
    server::AppState,
};

/// Create token request
#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub scopes: Vec<String>,
    pub org_id: Option<String>,
}

/// Create token response (includes the plain token)
#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub id: String,
    pub name: String,
    pub token: String,
    pub scopes: Vec<String>,
    pub prefix: String,
    pub created_at: String,
}

/// Create token handler
pub async fn create_token(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<CreateTokenRequest>,
) -> RegistryResult<Json<CreateTokenResponse>> {
    let auth_service = AuthService::new(&state.config, state.db.clone())?;

    // Parse org_id if provided
    let org_id = if let Some(org_id_str) = &req.org_id {
        Some(org_id_str.parse().map_err(|_| {
            crate::error::RegistryError::Validation("Invalid org_id format".to_string())
        })?)
    } else {
        None
    };

    // Parse scopes
    let scopes: Vec<TokenScope> = req
        .scopes
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    let (plain_token, db_token) = auth_service
        .generate_access_token(auth_user.user_id, org_id, &req.name, scopes)
        .await?;

    Ok(Json(CreateTokenResponse {
        id: db_token.id.to_string(),
        name: db_token.name,
        token: plain_token,
        scopes: db_token.scopes,
        prefix: db_token.token_prefix,
        created_at: db_token.created_at.to_rfc3339(),
    }))
}

/// Token list response item
#[derive(Debug, Serialize)]
pub struct TokenListItem {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub prefix: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

/// List tokens handler
pub async fn list_tokens(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> RegistryResult<Json<Vec<TokenListItem>>> {
    let tokens = list_user_access_tokens(state.db.pool(), auth_user.user_id).await?;

    Ok(Json(
        tokens
            .into_iter()
            .map(|t| TokenListItem {
                id: t.id.to_string(),
                name: t.name,
                scopes: t.scopes,
                prefix: t.token_prefix,
                last_used_at: t.last_used_at.map(|dt| dt.to_rfc3339()),
                created_at: t.created_at.to_rfc3339(),
                expires_at: t.expires_at.map(|dt| dt.to_rfc3339()),
            })
            .collect(),
    ))
}

/// Revoke token handler
pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(token_id): Path<String>,
) -> RegistryResult<StatusCode> {
    let id = token_id.parse().map_err(|_| {
        crate::error::RegistryError::Validation("Invalid token ID format".to_string())
    })?;

    // Get token to verify ownership
    let tokens = list_user_access_tokens(state.db.pool(), auth_user.user_id).await?;
    let token = tokens
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| crate::error::RegistryError::NotFound("Token not found".to_string()))?;

    revoke_access_token(state.db.pool(), token.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
