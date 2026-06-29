//! Authentication and authorization for the CRAFT Registry
//!
//! Provides JWT-based authentication with RS256 signing, access token management,
//! and authorization middleware for Axum routes.

use async_trait::async_trait;
use axum::{
    Json,
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::{
    RegistryConfig,
    db::{
        Database, User, get_access_token_by_hash, list_user_org_slugs, list_user_team_slugs,
        update_token_last_used,
    },
    error::{RegistryError, RegistryResult},
};

pub mod client;
mod password;

pub use client::{LoginRequest, LoginResponse, RegistryClient};
pub use password::{hash_password, verify_password};

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Issued at
    pub iat: i64,
    /// Expiration
    pub exp: i64,
    /// Issuer
    pub iss: String,
    /// User name
    pub name: String,
    /// User email
    pub email: String,
    /// Is admin
    pub is_admin: bool,
    /// Organization memberships
    #[serde(default)]
    pub orgs: Vec<String>,
    /// Team memberships as org/team
    #[serde(default)]
    pub teams: Vec<String>,
    /// Token class
    #[serde(default = "default_token_type")]
    pub token_type: String,
}

impl Claims {
    /// Create new claims for a user
    pub fn new(user_id: &str, name: &str, email: &str, is_admin: bool) -> Self {
        Self::with_ttl(
            user_id,
            name,
            email,
            is_admin,
            Vec::new(),
            Vec::new(),
            Duration::hours(24),
            "access",
        )
    }

    /// Create short-lived access token claims.
    pub fn access_token(
        user_id: &str,
        name: &str,
        email: &str,
        is_admin: bool,
        orgs: Vec<String>,
        teams: Vec<String>,
    ) -> Self {
        Self::with_ttl(
            user_id,
            name,
            email,
            is_admin,
            orgs,
            teams,
            Duration::hours(1),
            "access",
        )
    }

    /// Create long-lived refresh token claims.
    pub fn refresh_token(
        user_id: &str,
        name: &str,
        email: &str,
        is_admin: bool,
        orgs: Vec<String>,
        teams: Vec<String>,
    ) -> Self {
        Self::with_ttl(
            user_id,
            name,
            email,
            is_admin,
            orgs,
            teams,
            Duration::days(30),
            "refresh",
        )
    }

    fn with_ttl(
        user_id: &str,
        name: &str,
        email: &str,
        is_admin: bool,
        orgs: Vec<String>,
        teams: Vec<String>,
        ttl: Duration,
        token_type: &str,
    ) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: (now + ttl).timestamp(),
            iss: "craft-registry".to_string(),
            name: name.to_string(),
            email: email.to_string(),
            is_admin,
            orgs,
            teams,
            token_type: token_type.to_string(),
        }
    }
}

fn default_token_type() -> String {
    "access".to_string()
}

/// Login token pair returned by OAuth device flow.
#[derive(Debug, Clone, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Token scope for access tokens
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    /// Read-only access
    Read,
    /// Read and write access
    Write,
    /// Full admin access
    Admin,
}

impl TokenScope {
    /// Check if this scope satisfies a required scope
    pub fn satisfies(&self, required: TokenScope) -> bool {
        matches!(
            (self, required),
            (TokenScope::Admin, _)
                | (TokenScope::Write, TokenScope::Read)
                | (TokenScope::Write, TokenScope::Write)
                | (TokenScope::Read, TokenScope::Read)
        )
    }
}

impl std::str::FromStr for TokenScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(TokenScope::Read),
            "write" => Ok(TokenScope::Write),
            "admin" => Ok(TokenScope::Admin),
            _ => Err(format!("Unknown scope: {}", s)),
        }
    }
}

/// Access token (API token for CI/CD)
#[derive(Debug, Clone)]
pub struct AccessToken {
    /// Token ID (database ID)
    pub id: uuid::Uuid,
    /// User ID
    pub user_id: uuid::Uuid,
    /// Optional org ID (for org-scoped tokens)
    pub org_id: Option<uuid::Uuid>,
    /// Token name
    pub name: String,
    /// Token scopes
    pub scopes: Vec<TokenScope>,
}

/// Authenticated user extractor
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// User ID
    pub user_id: uuid::Uuid,
    /// User name
    pub name: String,
    /// User email
    pub email: String,
    /// Is admin
    pub is_admin: bool,
    /// Auth method used
    pub method: AuthMethod,
}

/// Authentication method
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// JWT token
    Jwt,
    /// Access token
    AccessToken { token_id: uuid::Uuid },
}

/// Auth service for managing keys and tokens
pub struct AuthService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    db: Database,
}

impl AuthService {
    /// Create a new auth service from config
    pub fn new(config: &RegistryConfig, db: Database) -> RegistryResult<Self> {
        let encoding_key = EncodingKey::from_rsa_pem(config.jwt_private_key.as_bytes())
            .map_err(|e| RegistryError::Auth(format!("Invalid JWT private key: {}", e)))?;

        let decoding_key = DecodingKey::from_rsa_pem(config.jwt_public_key.as_bytes())
            .map_err(|e| RegistryError::Auth(format!("Invalid JWT public key: {}", e)))?;

        Ok(Self {
            encoding_key,
            decoding_key,
            db,
        })
    }

    /// Generate a JWT token for a user
    pub fn generate_jwt(&self, claims: &Claims) -> RegistryResult<String> {
        let header = Header::new(Algorithm::RS256);
        encode(&header, claims, &self.encoding_key).map_err(RegistryError::Jwt)
    }

    /// Generate access and refresh JWTs for an authenticated user.
    pub async fn generate_device_token_pair(&self, user: &User) -> RegistryResult<TokenPair> {
        let orgs = list_user_org_slugs(self.db.pool(), user.id).await?;
        let teams = list_user_team_slugs(self.db.pool(), user.id).await?;
        let display_name = user.display_name.as_deref().unwrap_or(&user.username);

        let access_claims = Claims::access_token(
            &user.id.to_string(),
            display_name,
            &user.email,
            user.is_admin,
            orgs.clone(),
            teams.clone(),
        );
        let refresh_claims = Claims::refresh_token(
            &user.id.to_string(),
            display_name,
            &user.email,
            user.is_admin,
            orgs,
            teams,
        );

        Ok(TokenPair {
            access_token: self.generate_jwt(&access_claims)?,
            refresh_token: self.generate_jwt(&refresh_claims)?,
            token_type: "Bearer".to_string(),
            expires_in: 3600,
        })
    }

    /// Verify a JWT token
    pub fn verify_jwt(&self, token: &str) -> RegistryResult<Claims> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["craft-registry"]);

        decode::<Claims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(RegistryError::Jwt)
    }

    /// Generate a new access token (for CI/CD)
    /// Returns the plain token (to show once) and stores the hash
    pub async fn generate_access_token(
        &self,
        user_id: uuid::Uuid,
        org_id: Option<uuid::Uuid>,
        name: &str,
        scopes: Vec<TokenScope>,
    ) -> RegistryResult<(String, crate::db::AccessToken)> {
        // Generate random token
        let token = format!("crp_{}", generate_secure_token(32));
        let token_hash = hash_token(&token);
        let token_prefix = token[..12].to_string();

        let scope_strings: Vec<String> = scopes
            .iter()
            .map(|s| match s {
                TokenScope::Read => "read".to_string(),
                TokenScope::Write => "write".to_string(),
                TokenScope::Admin => "admin".to_string(),
            })
            .collect();

        let db_token = crate::db::create_access_token(
            self.db.pool(),
            user_id,
            org_id,
            name,
            &token_hash,
            &token_prefix,
            &scope_strings,
            None, // No expiration by default
        )
        .await?;

        Ok((token, db_token))
    }

    /// Verify an access token
    pub async fn verify_access_token(&self, token: &str) -> RegistryResult<AccessToken> {
        // Hash the token
        let token_hash = hash_token(token);

        // Look up in database
        let db_token = get_access_token_by_hash(self.db.pool(), &token_hash).await?;

        // Update last used
        update_token_last_used(self.db.pool(), db_token.id).await?;

        // Parse scopes
        let scopes: Vec<TokenScope> = db_token
            .scopes
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();

        Ok(AccessToken {
            id: db_token.id,
            user_id: db_token.user_id,
            org_id: db_token.org_id,
            name: db_token.name,
            scopes,
        })
    }
}

/// Hash a token using SHA-256
fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a secure random token
fn generate_secure_token(length: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// Extract bearer token from Authorization header
fn extract_bearer_token(parts: &Parts) -> Option<String> {
    extract_bearer_token_from_headers(&parts.headers)
}

fn extract_bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Extractor for authenticated users
#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = extract_bearer_token(parts).ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, "Missing authorization token").into_response()
        })?;

        if token.starts_with("crp_") {
            return Err((
                StatusCode::NOT_IMPLEMENTED,
                "Access token auth is handled by registry auth middleware",
            )
                .into_response());
        }

        Err((
            StatusCode::NOT_IMPLEMENTED,
            "JWT auth is handled by registry auth middleware",
        )
            .into_response())
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    // Extract token from header
    let token = extract_bearer_token_from_headers(request.headers()).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "code": "missing_token",
                    "message": "Authorization header with Bearer token required"
                }
            })),
        )
            .into_response()
    })?;

    // Authenticate
    let auth_user = if token.starts_with("crp_") {
        // Access token
        let access_token = state
            .auth_service
            .verify_access_token(&token)
            .await
            .map_err(|e| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": {
                            "code": "invalid_token",
                            "message": e.to_string()
                        }
                    })),
                )
                    .into_response()
            })?;

        // Get user details
        let user = crate::db::get_user_by_id(state.db.pool(), access_token.user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "code": "internal_error",
                            "message": e.to_string()
                        }
                    })),
                )
                    .into_response()
            })?;

        AuthUser {
            user_id: user.id,
            name: user.display_name.unwrap_or_else(|| user.username.clone()),
            email: user.email,
            is_admin: user.is_admin,
            method: AuthMethod::AccessToken {
                token_id: access_token.id,
            },
        }
    } else {
        // JWT token
        let claims = state.auth_service.verify_jwt(&token).map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {
                        "code": "invalid_token",
                        "message": e.to_string()
                    }
                })),
            )
                .into_response()
        })?;

        AuthUser {
            user_id: claims.sub.parse().map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {
                            "code": "internal_error",
                            "message": "Invalid user ID in token"
                        }
                    })),
                )
                    .into_response()
            })?,
            name: claims.name,
            email: claims.email,
            is_admin: claims.is_admin,
            method: AuthMethod::Jwt,
        }
    };

    // Add user to request extensions
    request.extensions_mut().insert(auth_user);

    Ok(next.run(request).await)
}

/// Authentication state for middleware
pub struct AuthState {
    pub auth_service: AuthService,
    pub db: Database,
}

/// Optional authentication - doesn't fail if no token provided
pub async fn optional_auth_middleware(
    State(state): State<Arc<AuthState>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(token) = extract_bearer_token_from_headers(request.headers())
        && let Ok(claims) = state.auth_service.verify_jwt(&token)
        && let Ok(user_id) = claims.sub.parse()
    {
        let auth_user = AuthUser {
            user_id,
            name: claims.name,
            email: claims.email,
            is_admin: claims.is_admin,
            method: AuthMethod::Jwt,
        };
        request.extensions_mut().insert(auth_user);
    }

    next.run(request).await
}

/// Require specific scope middleware factory
pub fn require_scope(scope: TokenScope) -> impl Fn(AuthUser) -> Result<AuthUser, Response> {
    move |user: AuthUser| match (scope, user.is_admin) {
        (TokenScope::Admin, false) => Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": {
                    "code": "insufficient_scope",
                    "message": "Admin scope required"
                }
            })),
        )
            .into_response()),
        _ => Ok(user),
    }
}

/// Admin-only middleware
pub async fn require_admin(request: Request, next: Next) -> Result<Response, Response> {
    let auth_user = request.extensions().get::<AuthUser>().ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "code": "unauthorized",
                    "message": "Authentication required"
                }
            })),
        )
            .into_response()
    })?;

    if !auth_user.is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": {
                    "code": "forbidden",
                    "message": "Admin access required"
                }
            })),
        )
            .into_response());
    }

    Ok(next.run(request).await)
}
