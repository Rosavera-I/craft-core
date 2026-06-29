//! Database layer for the CRAFT Registry
//!
//! Provides PostgreSQL database access using sqlx with full type safety.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Row};
use uuid::Uuid;

use crate::{
    Role, Visibility,
    error::{RegistryError, RegistryResult},
};

mod queries;

pub use queries::*;

/// Database connection pool wrapper
#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Create a new database connection pool
    pub async fn new(database_url: &str) -> RegistryResult<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(20)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    /// Get a reference to the underlying pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Run database migrations
    pub async fn migrate(&self) -> RegistryResult<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| RegistryError::Database(sqlx::Error::Migrate(Box::new(e))))?;
        Ok(())
    }
}

/// Organization entity
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub website_url: Option<String>,
    pub visibility: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Organization {
    /// Get visibility as enum
    pub fn visibility(&self) -> Visibility {
        match self.visibility.as_str() {
            "public" => Visibility::Public,
            "internal" => Visibility::Internal,
            _ => Visibility::Private,
        }
    }
}

/// Team entity
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Team {
    /// Get visibility as enum
    pub fn visibility(&self) -> Visibility {
        match self.visibility.as_str() {
            "public" => Visibility::Public,
            "internal" => Visibility::Internal,
            _ => Visibility::Private,
        }
    }
}

/// User entity
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub password_hash: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// Org membership with role
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct OrgMember {
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OrgMember {
    /// Get role as enum
    pub fn role(&self) -> Role {
        match self.role.as_str() {
            "admin" => Role::Admin,
            "maintainer" => Role::Maintainer,
            _ => Role::Member,
        }
    }
}

/// Team membership with role
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TeamMember {
    pub id: Uuid,
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TeamMember {
    /// Get role as enum
    pub fn role(&self) -> Role {
        match self.role.as_str() {
            "admin" => Role::Admin,
            "maintainer" => Role::Maintainer,
            _ => Role::Member,
        }
    }
}

/// Harness (package) entity
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Harness {
    pub id: Uuid,
    pub org_id: Uuid,
    pub team_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub keywords: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    pub git_repository_url: Option<String>,
    pub git_default_branch: Option<String>,
    pub total_downloads: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Harness {
    /// Get visibility as enum
    pub fn visibility(&self) -> Visibility {
        match self.visibility.as_str() {
            "public" => Visibility::Public,
            "internal" => Visibility::Internal,
            _ => Visibility::Private,
        }
    }
}

/// Harness version entity
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct HarnessVersion {
    pub id: Uuid,
    pub harness_id: Uuid,
    pub version: String,
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    pub prerelease: Option<String>,
    pub build_metadata: Option<String>,
    pub git_ref: Option<String>,
    pub git_commit_sha: Option<String>,
    pub description: Option<String>,
    pub readme_content: Option<String>,
    pub package_size_bytes: Option<i64>,
    pub content_sha256: String,
    pub storage_path: String,
    pub download_count: i64,
    pub is_yanked: bool,
    pub yanked_reason: Option<String>,
    pub published_by: Option<Uuid>,
    pub published_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Access token entity
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AccessToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Audit log entry
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<sqlx::types::ipnetwork::IpNetwork>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Webhook entity
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Webhook {
    pub id: Uuid,
    pub org_id: Uuid,
    pub harness_id: Option<Uuid>,
    pub name: String,
    pub url: String,
    pub secret: Option<String>,
    pub events: Vec<String>,
    pub is_active: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Rate limit entry
#[derive(Debug, Clone, FromRow)]
pub struct RateLimitEntry {
    pub id: Uuid,
    pub key: String,
    pub window_start: DateTime<Utc>,
    pub request_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Helper to convert SQL rows to semver::Version
pub fn parse_version_from_row(row: &sqlx::postgres::PgRow) -> RegistryResult<semver::Version> {
    let major: i32 = row.try_get("major")?;
    let minor: i32 = row.try_get("minor")?;
    let patch: i32 = row.try_get("patch")?;
    let prerelease: Option<String> = row.try_get("prerelease")?;

    let version_str = if let Some(pre) = prerelease {
        format!("{}.{}.{}-{}", major, minor, patch, pre)
    } else {
        format!("{}.{}.{}", major, minor, patch)
    };

    Ok(version_str.parse()?)
}
