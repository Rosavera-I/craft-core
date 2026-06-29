//! Simple API-key authentication for single-tenant and self-hosted registries.

use crate::{
    RegistryConfig,
    auth::{AccessToken, TokenScope},
    db::{self, Database},
    error::RegistryResult,
};

/// One-time first-run bootstrap credential.
#[derive(Debug, Clone)]
pub struct BootstrapApiKey {
    pub username: String,
    pub api_key: String,
}

/// Minimal API-key authenticator backed by the existing access token table.
#[derive(Debug, Clone)]
pub struct ApiKeyAuthenticator {
    db: Database,
}

impl ApiKeyAuthenticator {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Create an admin user and API key when the registry has no users yet.
    pub async fn bootstrap_admin(
        &self,
        config: &RegistryConfig,
    ) -> RegistryResult<Option<BootstrapApiKey>> {
        if !config.simple_auth_enabled || db::count_users(self.db.pool()).await? > 0 {
            return Ok(None);
        }

        let user = db::create_bootstrap_admin_user(self.db.pool()).await?;
        let api_key = config
            .bootstrap_admin_api_key
            .clone()
            .or_else(|| std::env::var("CRAFT_REGISTRY_BOOTSTRAP_API_KEY").ok())
            .unwrap_or_else(|| format!("crp_{}", super::generate_secure_token(32)));

        let scopes = vec![
            TokenScope::Read.to_string(),
            TokenScope::Write.to_string(),
            TokenScope::Admin.to_string(),
        ];
        let token_prefix = token_prefix(&api_key);
        db::create_access_token(
            self.db.pool(),
            user.id,
            None,
            "bootstrap-admin",
            &super::hash_token(&api_key),
            &token_prefix,
            &scopes,
            None,
        )
        .await?;

        Ok(Some(BootstrapApiKey {
            username: user.username,
            api_key,
        }))
    }

    pub async fn verify_api_key(&self, api_key: &str) -> RegistryResult<AccessToken> {
        let db_token =
            db::get_access_token_by_hash(self.db.pool(), &super::hash_token(api_key)).await?;
        db::update_token_last_used(self.db.pool(), db_token.id).await?;

        let scopes = db_token
            .scopes
            .iter()
            .filter_map(|scope| scope.parse().ok())
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

fn token_prefix(api_key: &str) -> String {
    api_key.chars().take(12).collect()
}

impl std::fmt::Display for TokenScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenScope::Read => write!(f, "read"),
            TokenScope::Write => write!(f, "write"),
            TokenScope::Admin => write!(f, "admin"),
        }
    }
}
