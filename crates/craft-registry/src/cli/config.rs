//! Configuration management for the CRAFT Registry CLI

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{RegistryError, RegistryResult};

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliConfig {
    /// Registry URL
    pub registry_url: Option<String>,
    /// Authentication token
    pub auth_token: Option<String>,
    /// Default organization
    pub default_org: Option<String>,
    /// Default output format
    pub default_format: Option<String>,
}

/// Get the default config file path
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("craft")
        .join("registry.toml")
}

/// Load configuration from file
pub fn load_config(path: &PathBuf) -> RegistryResult<CliConfig> {
    if !path.exists() {
        return Ok(CliConfig::default());
    }

    let content = std::fs::read_to_string(path)?;
    let config = toml::from_str(&content)
        .map_err(|e| RegistryError::Config(format!("Failed to parse config: {}", e)))?;

    Ok(config)
}

/// Save configuration to file
pub fn save_config(path: &PathBuf, config: &CliConfig) -> RegistryResult<()> {
    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(config)
        .map_err(|e| RegistryError::Config(format!("Failed to serialize config: {}", e)))?;

    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        let config = CliConfig {
            registry_url: Some("https://registry.example.com".to_string()),
            auth_token: Some("test_token".to_string()),
            default_org: Some("myorg".to_string()),
            default_format: Some("json".to_string()),
        };

        save_config(&config_path, &config).unwrap();
        let loaded = load_config(&config_path).unwrap();

        assert_eq!(loaded.registry_url, config.registry_url);
        assert_eq!(loaded.auth_token, config.auth_token);
        assert_eq!(loaded.default_org, config.default_org);
        assert_eq!(loaded.default_format, config.default_format);
    }

    #[test]
    fn test_load_nonexistent_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.toml");

        let config = load_config(&config_path).unwrap();
        assert!(config.registry_url.is_none());
        assert!(config.auth_token.is_none());
    }
}
