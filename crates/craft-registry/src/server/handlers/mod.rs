//! Request handlers for the CRAFT Registry API

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    auth::{AuthUser, Claims, hash_password, verify_password},
    db::{User, create_user, get_user_by_username, update_user_last_login},
    error::{RegistryError, RegistryResult},
    server::AppState,
};

mod device;
mod harness;
mod org;
mod team;
mod token;

pub use device::*;
pub use harness::*;
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

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            is_admin: user.is_admin,
        }
    }
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
    let password_hash = user
        .password_hash
        .as_ref()
        .ok_or_else(|| RegistryError::Auth("User has no password set".to_string()))?;

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
