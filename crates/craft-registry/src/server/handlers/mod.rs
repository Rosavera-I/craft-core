//! Request handlers for the CRAFT Registry API

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::{hash_password, verify_password, AuthUser, Claims},
    db::*,
    error::{RegistryError, RegistryResult},
    server::{AppState, PaginationParams},
    Role, Visibility,
};

mod harness;
mod device;
mod org;
mod team;
mod token;

pub use harness::*;
pub use device::*;
pub use org::*;
pub use team::*;
pub use token::*;

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserResponse,
}

/// User response
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
}

/// Registration request
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

/// Login handler
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> RegistryResult<Json<LoginResponse>> {
    // Find user by username
    let user = match get_user_by_username(state.db.pool(), &req.username).await {
        Ok(u) => u,
        Err(_) => {
            return Err(RegistryError::Auth(
                "Invalid username or password".to_string(),
            ));
        }
    };

    // Verify password
    let password_hash = user.password_hash.as_ref().ok_or_else(|| {
        RegistryError::Auth("User has no password set".to_string())
    })?;

    if !verify_password(&req.password, password_hash)? {
        return Err(RegistryError::Auth(
            "Invalid username or password".to_string(),
        ));
    }

    // Update last login
    update_user_last_login(state.db.pool(), user.id).await.ok();

    // Generate JWT
    let claims = Claims::new(
        &user.id.to_string(),
        &user.username,
        &user.email,
        user.is_admin,
    );

    let auth_service = crate::auth::AuthService::new(&state.config, state.db.clone())?;
    let token = auth_service.generate_jwt(&claims)?;

    Ok(Json(LoginResponse {
        token,
        user: user.into(),
    }))
}

/// Registration handler
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> RegistryResult<Json<UserResponse>> {
    // Hash password
    let password_hash = hash_password(&req.password)?;

    // Create user
    let user = create_user(
        state.db.pool(),
        &req.username,
        &req.email,
        Some(&password_hash),
        req.display_name.as_deref(),
    )
    .await?;

    Ok(Json(user.into()))
}

/// Get current user handler
pub async fn get_current_user(auth_user: AuthUser) -> RegistryResult<Json<UserResponse>> {
    Ok(Json(UserResponse {
        id: auth_user.user_id.to_string(),
        username: auth_user.name.clone(),
        email: auth_user.email.clone(),
        display_name: Some(auth_user.name),
        is_admin: auth_user.is_admin,
    }))
}

// Re-export handler functions
pub use org::{
    create_org_handler, delete_org_handler, get_org_handler, get_org_public, invite_org_member,
    list_org_members_handler, list_public_orgs, list_user_orgs_handler, remove_org_member,
    update_org_handler, update_org_member_role,
};

pub use team::{
    create_team_handler, delete_team_handler, get_team_handler, invite_team_member,
    list_team_members_handler, list_teams_handler, remove_team_member, update_team_handler,
};

pub use harness::{
    create_harness_handler, delete_harness_handler, download_version_handler, get_harness_handler,
    get_version_handler, list_harness_versions_handler, publish_package_handler, publish_version_handler,
    search_harnesses, update_harness_handler, yank_version_handler, unyank_version_handler,
};

pub use token::{create_token, list_tokens, revoke_token};
