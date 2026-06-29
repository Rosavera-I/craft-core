//! Server setup and configuration

use crate::api;
use crate::error::WebResult;
use crate::websocket;
use axum::{
    Router,
    extract::Extension,
    routing::{get, post},
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

use craft_core::CraftHome;

/// Shared application state
pub struct AppState {
    pub home: CraftHome,
    pub runtime_stats: RwLock<RuntimeStats>,
}

#[derive(Debug, Default, Clone)]
pub struct RuntimeStats {
    pub compositions_created: usize,
    pub validations_run: usize,
}

impl AppState {
    pub fn new(home: CraftHome) -> Self {
        Self {
            home,
            runtime_stats: RwLock::new(RuntimeStats::default()),
        }
    }

    pub async fn increment_compositions(&self) {
        let mut stats = self.runtime_stats.write().await;
        stats.compositions_created += 1;
    }

    pub async fn increment_validations(&self) {
        let mut stats = self.runtime_stats.write().await;
        stats.validations_run += 1;
    }
}

/// Create the Axum router with all routes
pub fn create_app(home: CraftHome, static_dir: Option<&str>) -> Router {
    let state = Arc::new(AppState::new(home));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_routes = Router::new()
        // Harness endpoints
        .route("/api/v1/harnesses", get(api::list_harnesses))
        .route("/api/v1/harnesses/:name", get(api::get_harness))
        .route(
            "/api/v1/harnesses/:name/versions",
            get(api::list_harness_versions),
        )
        // Composition endpoints
        .route("/api/v1/compose/plan", post(api::compose_plan))
        .route("/api/v1/compose", post(api::compose_harnesses_endpoint))
        // Memory endpoints
        .route("/api/v1/memory/search", get(api::search_memory))
        .route("/api/v1/memory/facts", get(api::list_memory_facts))
        .route("/api/v1/memory/facts", post(api::create_memory_fact))
        .route("/api/v1/memory/scope/:scope", get(api::get_memory_scope))
        // Runtime status
        .route("/api/v1/status", get(api::runtime_status))
        // WebSocket validation
        .route("/ws/validate", get(websocket::validation_handler))
        .layer(Extension(state));

    // Add static file serving if directory is provided
    if let Some(dir) = static_dir {
        info!("Serving static files from: {}", dir);
        Router::new()
            .nest("/", api_routes)
            .fallback_service(ServeDir::new(dir))
            .layer(cors)
    } else {
        api_routes.layer(cors)
    }
}

/// Run the server
pub async fn run_server(
    addr: SocketAddr,
    home: CraftHome,
    static_dir: Option<&str>,
) -> WebResult<()> {
    info!("Starting CRAFT Web Dashboard on {}", addr);
    info!("CRAFT_HOME: {:?}", home.root());

    ensure_environment(&home)?;

    let app = create_app(home, static_dir);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| crate::error::WebError::Internal(format!("Failed to bind: {e}")))?;

    info!("Server ready at http://{}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| crate::error::WebError::Internal(format!("Server error: {e}")))?;

    Ok(())
}

/// Ensure the CRAFT environment is set up
fn ensure_environment(home: &CraftHome) -> WebResult<()> {
    home.ensure().map_err(|e| {
        crate::error::WebError::Config(format!("Failed to initialize CRAFT_HOME: {e}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn app_state_tracks_compositions() {
        let home = CraftHome::new(std::env::temp_dir().join("craft-test-state"));
        let state = AppState::new(home);

        state.increment_compositions().await;
        state.increment_compositions().await;
        state.increment_validations().await;

        let stats = state.runtime_stats.read().await;
        assert_eq!(stats.compositions_created, 2);
        assert_eq!(stats.validations_run, 1);
    }
}
