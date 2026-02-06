//! Configuration I/O - Loading and saving configuration
//!
//! Handles reading configuration from files and environment variables.

use std::path::Path;

use super::types::Config;
use crate::error::{Error, Result};

/// A snapshot of the configuration file
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    /// Path to the config file
    pub path: std::path::PathBuf,
    /// Whether the file exists
    pub exists: bool,
    /// Raw file content
    pub raw: Option<String>,
    /// Parsed configuration
    pub config: Option<Config>,
    /// Validation issues
    pub issues: Vec<String>,
}

/// Load configuration from the default path
pub fn load_config() -> Result<Config> {
    let config_path = super::paths::config_path();

    if config_path.exists() {
        load_config_from_path(&config_path)
    } else {
        // Try to load from environment variables
        load_config_from_env()
    }
}

/// Load configuration from a specific path
pub fn load_config_from_path(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!("Failed to read config file {}: {}", path.display(), e))
    })?;

    // Detect format by extension
    let config: Config = if path.extension().map_or(false, |ext| ext == "json") {
        // Parse as JSON5 (more lenient than strict JSON)
        json5::from_str(&content).map_err(|e| Error::Config(format!("Invalid JSON config: {}", e)))?
    } else if path.extension().map_or(false, |ext| ext == "toml") {
        toml::from_str(&content).map_err(|e| Error::Config(format!("Invalid TOML config: {}", e)))?
    } else {
        // Try JSON5 first, then TOML
        json5::from_str(&content)
            .or_else(|_| toml::from_str(&content).map_err(|e| Error::Config(e.to_string())))
            .map_err(|e| Error::Config(format!("Failed to parse config: {}", e)))?
    };

    Ok(config)
}

/// Load configuration from environment variables
pub fn load_config_from_env() -> Result<Config> {
    use secrecy::SecretString;

    // Load .env file if it exists
    dotenvy::dotenv().ok();

    let mut config = Config::default();

    // Load OpenRouter config
    if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
        config.provider.openrouter = Some(super::types::provider::OpenRouterConfig {
            api_key: SecretString::from(api_key),
            default_model: std::env::var("DEFAULT_MODEL")
                .or_else(|_| std::env::var("OPENROUTER_MODEL"))
                .unwrap_or_else(|_| "anthropic/claude-sonnet-4".to_string()),
            base_url: std::env::var("OPENROUTER_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string()),
            site_url: std::env::var("OPENROUTER_SITE_URL").ok(),
            site_name: std::env::var("OPENROUTER_SITE_NAME").ok(),
            timeout_secs: std::env::var("OPENROUTER_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            max_retries: std::env::var("OPENROUTER_MAX_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
        });
    }

    // Load Telegram config
    if let Ok(bot_token) = std::env::var("TELEGRAM_BOT_TOKEN") {
        config.channels.telegram = Some(super::types::channel::TelegramConfig {
            bot_token: SecretString::from(bot_token),
            allow_from: std::env::var("TELEGRAM_ALLOWED_USERS")
                .ok()
                .map(|s| {
                    s.split(',')
                        .filter_map(|id| id.trim().parse().ok())
                        .collect()
                })
                .unwrap_or_default(),
            dm_policy: super::types::channel::DmPolicy::Open,
            groups: std::collections::HashMap::new(),
            use_long_polling: std::env::var("TELEGRAM_USE_WEBHOOK")
                .map(|v| v != "true" && v != "1")
                .unwrap_or(true),
            webhook_url: std::env::var("TELEGRAM_WEBHOOK_URL").ok(),
            webhook_secret: std::env::var("TELEGRAM_WEBHOOK_SECRET").ok(),
        });
    }

    // Load database config
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        config.storage.postgres = Some(super::types::storage::PostgresConfig {
            url: SecretString::from(database_url),
            max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            connect_timeout_secs: std::env::var("DATABASE_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            enable_pgvector: true,
        });
        config.storage.backend = super::types::storage::StorageBackendType::Postgres;
    }

    // Load sandbox config
    if let Ok(env_str) = std::env::var("EXECUTION_ENV") {
        if let Ok(exec_env) = env_str.parse() {
            config.sandbox.execution_env = exec_env;
        }
    }
    if let Ok(allowed_dir) = std::env::var("ALLOWED_DIR") {
        config.sandbox.allowed_dir = std::path::PathBuf::from(allowed_dir);
    }

    // Load gateway config
    if let Ok(port) = std::env::var("GATEWAY_PORT") {
        if let Ok(port) = port.parse() {
            config.gateway.port = port;
        }
    }

    Ok(config)
}

/// Save configuration to a file
pub fn save_config(config: &Config, path: &Path) -> Result<()> {
    let content = if path.extension().map_or(false, |ext| ext == "toml") {
        toml::to_string_pretty(config).map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?
    } else {
        serde_json::to_string_pretty(config).map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, content)?;
    Ok(())
}

/// Read a configuration file into a snapshot
#[allow(dead_code)]
pub fn read_config_snapshot(path: &Path) -> ConfigSnapshot {
    if !path.exists() {
        return ConfigSnapshot {
            path: path.to_path_buf(),
            exists: false,
            raw: None,
            config: None,
            issues: vec!["Configuration file does not exist".to_string()],
        };
    }

    let raw = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            return ConfigSnapshot {
                path: path.to_path_buf(),
                exists: true,
                raw: None,
                config: None,
                issues: vec![format!("Failed to read file: {}", e)],
            };
        }
    };

    let config = match load_config_from_path(path) {
        Ok(config) => Some(config),
        Err(e) => {
            return ConfigSnapshot {
                path: path.to_path_buf(),
                exists: true,
                raw: Some(raw),
                config: None,
                issues: vec![format!("Failed to parse config: {}", e)],
            };
        }
    };

    ConfigSnapshot {
        path: path.to_path_buf(),
        exists: true,
        raw: Some(raw),
        config,
        issues: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_config() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_config.json");

        let config = Config::default();
        save_config(&config, &path).unwrap();

        let loaded = load_config_from_path(&path).unwrap();
        assert_eq!(loaded.agent.model, config.agent.model);
    }
}
