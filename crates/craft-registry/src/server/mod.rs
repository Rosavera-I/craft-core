//! Axum server for the CRAFT Cloud Harness Registry
//!
//! Provides RESTful API endpoints for org/team management, harness publishing,
//! version control, and access token management.

use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    middleware::from_fn_with_state,
    response::Json,
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};

use crate::{
    RegistryConfig,
    auth::{AuthState, auth_middleware},
    db::Database,
    error::RegistryResult,
    server::handlers::*,
};

mod handlers;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: RegistryConfig,
    pub db: Database,
}

/// Server instance
pub struct Server {
    config: RegistryConfig,
    app: Router,
}

impl Server {
    /// Create a new server instance
    pub async fn new(config: RegistryConfig) -> RegistryResult<Self> {
        let db = Database::new(&config.database_url).await?;

        // Run migrations
        db.migrate().await?;

        let state = Arc::new(AppState {
            config: config.clone(),
            db,
        });

        if config.simple_auth_enabled {
            if let Some(bootstrap) = crate::auth::ApiKeyAuthenticator::new(state.db.clone())
                .bootstrap_admin(&config)
                .await?
            {
                tracing::warn!(
                    username = %bootstrap.username,
                    api_key = %bootstrap.api_key,
                    "Created first-run CRAFT registry admin API key; store it now because it will not be shown again"
                );
            }
        }

        // Create auth state
        let auth_state = Arc::new(AuthState {
            auth_service: crate::auth::AuthService::new(&config, state.db.clone())?,
            db: state.db.clone(),
        });

        // Build routes
        let app = Self::build_routes(state, auth_state, &config);

        Ok(Self { config, app })
    }

    /// Build the router with all routes
    fn build_routes(
        state: Arc<AppState>,
        auth_state: Arc<AuthState>,
        config: &RegistryConfig,
    ) -> Router {
        // CORS configuration
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // Public routes (no auth required)
        let public_routes = Router::new()
            .route("/health", get(health_check))
            .route("/api/v1/status", get(status))
            .route("/api/v1/auth/login", post(login))
            .route("/api/v1/auth/register", post(register))
            .route("/api/v1/public/orgs", get(list_public_orgs))
            .route("/api/v1/public/orgs/:name", get(get_org_public))
            .route("/api/v1/harnesses/search", get(search_harnesses));

        let public_routes = if config.enable_github_oauth {
            public_routes
                .route("/auth/device", post(initiate_device_auth))
                .route("/auth/device/poll", post(poll_device_auth))
                .route("/auth/github/callback", get(github_callback))
                .route("/api/v1/auth/device", post(initiate_device_auth))
                .route("/api/v1/auth/device/poll", post(poll_device_auth))
                .route("/api/v1/auth/github/callback", get(github_callback))
        } else {
            public_routes
        };

        // Routes requiring authentication
        let protected = Router::new()
            // User routes
            .route("/api/v1/user/me", get(get_current_user))
            .route("/api/v1/user/orgs", get(list_user_orgs_handler))
            .route("/api/v1/user/orgs/:name", get(get_org_handler))
            .route("/api/v1/user/tokens", post(create_token))
            .route("/api/v1/user/tokens", get(list_tokens))
            .route("/api/v1/user/tokens/:id", delete(revoke_token))
            // Organization routes
            .route("/api/v1/orgs", get(list_user_orgs_handler))
            .route("/api/v1/orgs", post(create_org_handler))
            .route("/api/v1/orgs/:name", get(get_org_handler))
            .route("/api/v1/orgs/:name", put(update_org_handler))
            .route("/api/v1/orgs/:name", delete(delete_org_handler))
            .route("/api/v1/orgs/:name/members", get(list_org_members_handler))
            .route("/api/v1/orgs/:name/invites", post(invite_org_member))
            .route(
                "/api/v1/orgs/:name/members/:user_id",
                put(update_org_member_role),
            )
            .route(
                "/api/v1/orgs/:name/members/:user_id",
                delete(remove_org_member),
            )
            .route("/api/v1/orgs/:name/teams", get(list_teams_handler))
            .route("/api/v1/orgs/:name/teams", post(create_team_handler))
            // Team routes
            .route("/api/v1/orgs/:name/teams/:team_name", get(get_team_handler))
            .route(
                "/api/v1/orgs/:name/teams/:team_name",
                put(update_team_handler),
            )
            .route(
                "/api/v1/orgs/:name/teams/:team_name",
                delete(delete_team_handler),
            )
            .route(
                "/api/v1/orgs/:name/teams/:team_name/members",
                get(list_team_members_handler),
            )
            .route(
                "/api/v1/orgs/:name/teams/:team_name/members",
                post(invite_team_member),
            )
            .route(
                "/api/v1/orgs/:name/teams/:team_name/members/:user_id",
                delete(remove_team_member),
            )
            // Harness routes
            .route("/api/v1/packages", post(publish_package_handler))
            .route("/api/v1/harnesses/:org", post(create_harness_handler))
            .route("/api/v1/harnesses/:org/:name", get(get_harness_handler))
            .route("/api/v1/harnesses/:org/:name", put(update_harness_handler))
            .route(
                "/api/v1/harnesses/:org/:name",
                delete(delete_harness_handler),
            )
            .route(
                "/api/v1/harnesses/:org/:name/versions",
                get(list_harness_versions_handler),
            )
            .route(
                "/api/v1/harnesses/:org/:name/versions",
                post(publish_version_handler),
            )
            .route(
                "/api/v1/harnesses/:org/:name/versions/:version",
                get(get_version_handler),
            )
            .route(
                "/api/v1/harnesses/:org/:name/versions/:version/yank",
                post(yank_version_handler),
            )
            .route(
                "/api/v1/harnesses/:org/:name/versions/:version/unyank",
                post(unyank_version_handler),
            )
            .route(
                "/api/v1/harnesses/:org/:name/download/:version",
                get(download_version_handler),
            );

        // Combine routes
        Router::new()
            .merge(public_routes)
            .merge(protected.layer(from_fn_with_state(auth_state, auth_middleware)))
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(CompressionLayer::new())
                    .layer(RequestBodyLimitLayer::new(config.max_package_size))
                    .layer(cors)
                    .layer(DefaultBodyLimit::max(config.max_package_size)),
            )
            .with_state(state)
    }

    /// Run the server
    pub async fn run(self) -> RegistryResult<()> {
        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!("Starting server on {}", addr);
        axum::serve(listener, self.app).await?;

        Ok(())
    }
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: crate::VERSION.to_string(),
    })
}

/// Status endpoint
async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    Json(StatusResponse {
        version: crate::VERSION.to_string(),
        database_connected: !state.db.pool().is_closed(),
    })
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Status response
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub database_connected: bool,
}

/// Pagination parameters
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}
