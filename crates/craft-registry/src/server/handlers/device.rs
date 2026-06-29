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
    /// OAuth client identifier for the requesting CLI or app.
    pub client_id: String,
}

/// Device authorization response for CLI clients.
#[derive(Debug, Serialize)]
pub struct DeviceAuthorizationResponse {
    /// Opaque code used by the CLI to poll for completion.
    pub device_code: String,
    /// Short code shown to the user during browser verification.
    pub user_code: String,
    /// Browser URL where the user completes authorization.
    pub verification_uri: String,
    /// Lifetime of the device authorization in seconds.
    pub expires_in: i64,
    /// Minimum polling interval in seconds.
    pub interval: i64,
}

/// Device authorization polling request.
#[derive(Debug, Deserialize)]
pub struct PollDeviceRequest {
    /// Opaque device code returned by device authorization initiation.
    pub device_code: String,
}

/// Device authorization polling response.
#[derive(Debug, Serialize)]
pub struct DevicePollResponse {
    /// Short-lived access JWT when authorization has completed.
    pub access_token: Option<String>,
    /// Long-lived refresh JWT when authorization has completed.
    pub refresh_token: Option<String>,
    /// Token type for successful responses.
    pub token_type: Option<String>,
    /// Access token lifetime in seconds for successful responses.
    pub expires_in: Option<i64>,
    /// OAuth device-flow error code for pending or failed grants.
    pub error: Option<String>,
}

/// GitHub OAuth callback query parameters.
#[derive(Debug, Deserialize)]
pub struct GithubCallbackQuery {
    /// GitHub authorization code.
    pub code: Option<String>,
    /// Device user code, carried through the OAuth state parameter.
    pub state: Option<String>,
    /// GitHub OAuth error code.
    pub error: Option<String>,
    /// Optional GitHub OAuth error description.
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
    let client_id = state
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::Database, RegistryConfig, StorageConfig};
    use sqlx::postgres::PgPoolOptions;

    fn test_state(config: RegistryConfig) -> RegistryResult<AppState> {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://localhost/craft_test")
            .map_err(|err| RegistryError::Database(err))?;
        Ok(AppState {
            config,
            db: Database::from_pool(pool),
        })
    }

    #[test]
    fn error_response_only_sets_error_code() {
        let response = error_response("authorization_pending");

        assert_eq!(response.error.as_deref(), Some("authorization_pending"));
        assert!(response.access_token.is_none());
        assert!(response.refresh_token.is_none());
        assert!(response.token_type.is_none());
        assert!(response.expires_in.is_none());
    }

    #[test]
    fn random_alphanumeric_has_expected_length() {
        let token = random_alphanumeric(DEVICE_CODE_LEN);

        assert_eq!(token.len(), DEVICE_CODE_LEN);
        assert!(token.chars().all(|ch| ch.is_ascii_alphanumeric()));
    }

    #[test]
    fn github_authorization_uri_uses_configured_client_and_state() -> RegistryResult<()> {
        let state = test_state(RegistryConfig {
            public_base_url: "https://registry.example.com".to_string(),
            github_oauth_client_id: Some("client-123".to_string()),
            github_oauth_redirect_uri: Some(
                "https://registry.example.com/oauth/callback".to_string(),
            ),
            storage: StorageConfig::default(),
            ..RegistryConfig::default()
        })?;

        let uri = github_authorization_uri(&state, "ABCD1234")?;
        let parsed =
            Url::parse(&uri).map_err(|err| RegistryError::Validation(err.to_string()))?;
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        assert_eq!(
            parsed.as_str().split('?').next(),
            Some("https://github.com/login/oauth/authorize")
        );
        assert_eq!(params.get("client_id").map(String::as_str), Some("client-123"));
        assert_eq!(params.get("state").map(String::as_str), Some("ABCD1234"));
        assert_eq!(
            params.get("redirect_uri").map(String::as_str),
            Some("https://registry.example.com/oauth/callback")
        );
        assert_eq!(params.get("scope").map(String::as_str), Some("read:user user:email"));
        Ok(())
    }

    #[test]
    fn github_authorization_uri_requires_client_id() -> RegistryResult<()> {
        let state = test_state(RegistryConfig::default())?;

        let error = github_authorization_uri(&state, "ABCD1234")
            .err()
            .ok_or_else(|| RegistryError::Internal("expected missing client id error".to_string()))?;

        assert!(matches!(error, RegistryError::Config(_)));
        Ok(())
    }
}
