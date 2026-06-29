//! CRAFT Cloud Harness Registry
//!
//! Private harness registries for teams with Git-backed package management.
//!
//! This crate provides:
//! - Server (Axum): REST API for org/team management, harness publishing, version control
//! - Database (PostgreSQL): Full org/team/harness/version schema with audit logging
//! - CLI: Client commands for login, publish, install, and team management
//! - Git integration: Harness packages stored with Git refs
//! - Auth: JWT with RS256 signing, access tokens for CI/CD

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::fmt;

pub mod auth;
pub mod cli;
pub mod db;
pub mod error;
pub mod git;
pub mod storage;
pub mod version;

pub use auth::{AccessToken, Claims, RegistryClient, TokenScope};
pub use error::{RegistryError, RegistryResult};

/// Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Database URL
    pub database_url: String,
    /// JWT signing key (RS256 private key PEM)
    pub jwt_private_key: String,
    /// JWT public key (RS256 public key PEM)
    pub jwt_public_key: String,
    /// Storage backend configuration
    pub storage: StorageConfig,
    /// Server bind address
    pub bind_address: String,
    /// Server port
    pub port: u16,
    /// Git repository base path
    pub git_base_path: String,
    /// Maximum package size in bytes (default: 100MB)
    pub max_package_size: usize,
    /// Rate limiting requests per window (default: 100/min)
    pub rate_limit_requests: u32,
    /// Rate limiting window in seconds (default: 60)
    pub rate_limit_window_secs: u64,
    /// Public registry URL used to build OAuth callback and verification URLs.
    pub public_base_url: String,
    /// GitHub OAuth app client ID.
    pub github_oauth_client_id: Option<String>,
    /// GitHub OAuth app client secret.
    pub github_oauth_client_secret: Option<String>,
    /// Optional explicit GitHub OAuth redirect URI.
    #[serde(default)]
    pub github_oauth_redirect_uri: Option<String>,
    /// Enable GitHub OAuth device login routes.
    #[serde(default)]
    pub enable_github_oauth: bool,
    /// Enable simple API-key authentication.
    #[serde(default = "default_simple_auth_enabled")]
    pub simple_auth_enabled: bool,
    /// Optional first-run admin API key. Generated and logged once when omitted.
    #[serde(default)]
    pub bootstrap_admin_api_key: Option<String>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            database_url: String::new(),
            jwt_private_key: String::new(),
            jwt_public_key: String::new(),
            storage: StorageConfig::default(),
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            git_base_path: "/var/lib/craft-registry/git".to_string(),
            max_package_size: 100 * 1024 * 1024, // 100MB
            rate_limit_requests: 100,
            rate_limit_window_secs: 60,
            public_base_url: "http://localhost:8080".to_string(),
            github_oauth_client_id: None,
            github_oauth_client_secret: None,
            github_oauth_redirect_uri: None,
            enable_github_oauth: false,
            simple_auth_enabled: true,
            bootstrap_admin_api_key: None,
        }
    }
}

fn default_simple_auth_enabled() -> bool {
    true
}

/// Storage backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StorageConfig {
    /// Local filesystem storage
    Local {
        /// Base path for stored packages
        base_path: String,
    },
    /// S3-compatible storage
    S3 {
        /// Bucket name
        bucket: String,
        /// Region
        region: String,
        /// Endpoint (for MinIO, etc.)
        endpoint: Option<String>,
        /// Access key ID
        access_key_id: String,
        /// Secret access key
        secret_access_key: String,
    },
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self::Local {
            base_path: "/var/lib/craft-registry/packages".to_string(),
        }
    }
}

/// Published harness metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedHarness {
    /// Organization name
    pub org: String,
    /// Harness name
    pub name: String,
    /// Semantic version
    pub version: semver::Version,
    /// Git reference (tag or commit)
    pub git_ref: String,
    /// URL to download the artifact
    pub artifact_url: String,
    /// Content SHA-256 hash
    pub content_sha256: String,
    /// Package size in bytes
    pub package_size: u64,
    /// Published timestamp
    pub published_at: chrono::DateTime<chrono::Utc>,
    /// Publisher user ID
    pub published_by: String,
}

/// Organization visibility levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Visible to everyone
    Public,
    /// Visible to authenticated users
    Internal,
    /// Visible only to org members
    Private,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Internal => write!(f, "internal"),
            Visibility::Private => write!(f, "private"),
        }
    }
}

/// Team/Org member roles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Can view and use harnesses
    Member,
    /// Can publish and manage harnesses
    Maintainer,
    /// Can manage team/org settings and members
    Admin,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Member => write!(f, "member"),
            Role::Maintainer => write!(f, "maintainer"),
            Role::Admin => write!(f, "admin"),
        }
    }
}

/// Webhook event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
    /// Harness published
    HarnessPublished,
    /// Harness version yanked
    HarnessYanked,
    /// Harness version unyanked
    HarnessUnyanked,
    /// Team member added
    TeamMemberAdded,
    /// Team member removed
    TeamMemberRemoved,
}

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
