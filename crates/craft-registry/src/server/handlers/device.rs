//! OAuth device authorization request handlers

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use chrono::{Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use url::Url;

use crate::{
    auth::AuthService,
    db::*,
    error::{RegistryError, RegistryResult},
    server::AppState,
};

const DEVICE_CODE_LEN: usize = 40;
const USER_CODE_LEN: usize = 8;
const DEVICE_AUTH_EXPIRY_SECS: i64 = 600;
const DEVICE_AUTH_INTERVAL_SECS: i64 = 5;

/// Device authorization initiation request.
#[derive(Debug, Deserialize)]
pub struct InitiateDeviceRequest {
    pub client_id: String,
}

/// Device authorization response for CLI clients.
#[derive(Debug, Serialize)]
pub struct DeviceAuthorizationResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

/// Device authorization polling request.
#[derive(Debug, Deserialize)]
pub struct PollDeviceRequest {
    pub device_code: String,
}

/// Device authorization polling response.
#[derive(Debug, Serialize)]
pub struct DevicePollResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub token_type: Option<String>,
    pub expires_in: Option<i64>,
    pub error: Option<String>,
}

/// GitHub OAuth callback query parameters.
#[derive(Debug, Deserialize)]
pub struct GithubCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct GithubUserResponse {
    login: String,
    email: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubEmailResponse {
    email: String,
    primary: bool,
    verified: bool,
}

/// Start an OAuth device authorization flow.
pub async fn initiate_device_auth(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InitiateDeviceRequest>,
) -> RegistryResult<Json<DeviceAuthorizationResponse>> {
    if req.client_id.trim().is_empty() {
        return Err(RegistryError::Validation(
            "client_id is required".to_string(),
        ));
    }

    let device_code = random_alphanumeric(DEVICE_CODE_LEN);
    let user_code = random_alphanumeric(USER_CODE_LEN).to_ascii_uppercase();
    let expires_at = Utc::now() + Duration::seconds(DEVICE_AUTH_EXPIRY_SECS);

    create_device_authorization(
        state.db.pool(),
        &device_code,
        &user_code,
        &req.client_id,
        expires_at,
        DEVICE_AUTH_INTERVAL_SECS as i32,
    )
    .await?;

    Ok(Json(DeviceAuthorizationResponse {
        device_code,
        user_code: user_code.clone(),
        verification_uri: github_authorization_uri(&state.config, &user_code)?,
        expires_in: DEVICE_AUTH_EXPIRY_SECS,
        interval: DEVICE_AUTH_INTERVAL_SECS,
    }))
}

/// Poll for completion of an OAuth device authorization flow.
pub async fn poll_device_auth(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PollDeviceRequest>,
) -> RegistryResult<Json<DevicePollResponse>> {
    if req.device_code.trim().is_empty() {
        return Ok(Json(error_response("invalid_request")));
    }

    let grant = get_device_authorization_by_device_code(state.db.pool(), &req.device_code).await?;

    if grant.expires_at <= Utc::now() {
        expire_device_authorization(state.db.pool(), grant.id).await?;
        return Ok(Json(error_response("expired_token")));
    }

    let grant = record_device_poll(state.db.pool(), grant.id, grant.interval_secs).await?;

    match grant.status.as_str() {
        "pending" => Ok(Json(error_response("authorization_pending"))),
        "denied" => Ok(Json(error_response("access_denied"))),
        "expired" => Ok(Json(error_response("expired_token"))),
        "approved" => {
            let user_id = grant.user_id.ok_or_else(|| {
                RegistryError::Internal("approved device authorization has no user".to_string())
            })?;
            let user = get_user_by_id(state.db.pool(), user_id).await?;
            let auth_service = AuthService::new(&state.config, state.db.clone())?;
            let token_pair = auth_service.generate_device_token_pair(&user).await?;

            Ok(Json(DevicePollResponse {
                access_token: Some(token_pair.access_token),
                refresh_token: Some(token_pair.refresh_token),
                token_type: Some(token_pair.token_type),
                expires_in: Some(token_pair.expires_in),
                error: None,
            }))
        }
        other => Err(RegistryError::Internal(format!(
            "unknown device authorization status: {}",
            other
        ))),
    }
}

/// Complete GitHub OAuth and approve the pending device authorization.
pub async fn github_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GithubCallbackQuery>,
) -> RegistryResult<Response> {
    if let Some(error) = query.error {
        let description = query.error_description.unwrap_or(error);
        return Ok((StatusCode::BAD_REQUEST, description).into_response());
    }

    let code = query
        .code
        .ok_or_else(|| RegistryError::Validation("missing code".to_string()))?;
    let user_code = query
        .state
        .ok_or_else(|| RegistryError::Validation("missing state".to_string()))?;

    let grant = get_device_authorization_by_user_code(state.db.pool(), &user_code).await?;
    if grant.expires_at <= Utc::now() {
        expire_device_authorization(state.db.pool(), grant.id).await?;
        return Ok((StatusCode::BAD_REQUEST, "Device code expired").into_response());
    }
    if grant.status != "pending" {
        return Ok((
            StatusCode::BAD_REQUEST,
            "Device authorization is no longer pending",
        )
            .into_response());
    }

    let github_token = exchange_github_code(&state, &code).await?;
    let github_user = fetch_github_user(&github_token).await?;
    let email = match github_user.email {
        Some(email) => email,
        None => fetch_primary_github_email(&github_token).await?,
    };

    let user = upsert_github_user(
        state.db.pool(),
        &github_user.login,
        &email,
        github_user.name.as_deref(),
        github_user.avatar_url.as_deref(),
    )
    .await?;
    update_user_last_login(state.db.pool(), user.id).await?;
    approve_device_authorization(state.db.pool(), grant.id, user.id).await?;

    Ok(Redirect::to("/auth/device/success").into_response())
}

fn error_response(error: &str) -> DevicePollResponse {
    DevicePollResponse {
        access_token: None,
        refresh_token: None,
        token_type: None,
        expires_in: None,
        error: Some(error.to_string()),
    }
}

fn random_alphanumeric(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

fn github_authorization_uri(state: &AppState, user_code: &str) -> RegistryResult<String> {
    let client_id = config
        .config
        .github_oauth_client_id
        .as_deref()
        .ok_or_else(|| RegistryError::Config("GitHub OAuth client ID is not configured".to_string()))?;

    let mut url = Url::parse("https://github.com/login/oauth/authorize")
        .map_err(|e| RegistryError::Config(format!("invalid GitHub authorize URL: {}", e)))?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", &github_redirect_uri(state))
        .append_pair("scope", "read:user user:email")
        .append_pair("state", user_code);

    Ok(url.to_string())
}

fn github_redirect_uri(state: &AppState) -> String {
    state
        .config
        .github_oauth_redirect_uri
        .clone()
        .unwrap_or_else(|| format!("{}/auth/github/callback", state.config.public_base_url))
}

async fn exchange_github_code(state: &AppState, code: &str) -> RegistryResult<String> {
    let client_id = state
        .config
        .github_oauth_client_id
        .as_deref()
        .ok_or_else(|| RegistryError::Config("GitHub OAuth client ID is not configured".to_string()))?;
    let client_secret = state
        .config
        .github_oauth_client_secret
        .as_deref()
        .ok_or_else(|| RegistryError::Config("GitHub OAuth client secret is not configured".to_string()))?;

    let client = reqwest::Client::new();
    let response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", &github_redirect_uri(state)),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<GithubTokenResponse>()
        .await?;

    Ok(response.access_token)
}

async fn fetch_github_user(access_token: &str) -> RegistryResult<GithubUserResponse> {
    reqwest::Client::new()
        .get("https://api.github.com/user")
        .bearer_auth(access_token)
        .header("User-Agent", "craft-registry")
        .send()
        .await?
        .error_for_status()?
        .json::<GithubUserResponse>()
        .await
        .map_err(RegistryError::Http)
}

async fn fetch_primary_github_email(access_token: &str) -> RegistryResult<String> {
    let emails = reqwest::Client::new()
        .get("https://api.github.com/user/emails")
        .bearer_auth(access_token)
        .header("User-Agent", "craft-registry")
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<GithubEmailResponse>>()
        .await?;

    emails
        .into_iter()
        .find(|email| email.primary && email.verified)
        .map(|email| email.email)
        .ok_or_else(|| RegistryError::Auth("GitHub account has no verified primary email".to_string()))
}
