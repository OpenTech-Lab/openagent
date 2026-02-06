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

/// Load configuration with layered precedence:
/// 1. Config file (config.json) if it exists, otherwise defaults
/// 2. Environment variable overrides (includes .env for backward compat)
pub fn load_config() -> Result<Config> {
    let config_path = super::paths::config_path();

    let mut config = if config_path.exists() {
        load_config_from_path(&config_path)?
    } else {
        Config::default()
    };

    // Apply environment variable overrides (highest precedence)
    apply_env_overrides(&mut config);

    Ok(config)
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

/// Load configuration from environment variables (backward compat wrapper)
#[allow(dead_code)]
pub fn load_config_from_env() -> Result<Config> {
    let mut config = Config::default();
    apply_env_overrides(&mut config);
    Ok(config)
}

/// Apply environment variable overrides to an existing config.
///
/// This loads `.env` file (for backward compat) and overlays any set
/// environment variables onto the config. Env vars have the highest
/// precedence in the config layering: defaults < file < DB < env.
pub fn apply_env_overrides(config: &mut Config) {
    use secrecy::SecretString;

    // Load .env file if it exists (backward compat)
    dotenvy::dotenv().ok();

    // OpenRouter overrides
    if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
        let or = config.provider.openrouter.get_or_insert_with(|| {
            super::types::provider::OpenRouterConfig {
                api_key: SecretString::from(String::new()),
                default_model: "anthropic/claude-sonnet-4".to_string(),
                base_url: "https://openrouter.ai/api/v1".to_string(),
                site_url: None,
                site_name: None,
                timeout_secs: 120,
                max_retries: 3,
            }
        });
        or.api_key = SecretString::from(api_key);
    }
    if let Ok(model) = std::env::var("DEFAULT_MODEL").or_else(|_| std::env::var("OPENROUTER_MODEL")) {
        if let Some(ref mut or) = config.provider.openrouter {
            or.default_model = model;
        }
    }
    if let Ok(url) = std::env::var("OPENROUTER_BASE_URL") {
        if let Some(ref mut or) = config.provider.openrouter {
            or.base_url = url;
        }
    }
    if let Ok(url) = std::env::var("OPENROUTER_SITE_URL") {
        if let Some(ref mut or) = config.provider.openrouter {
            or.site_url = Some(url);
        }
    }
    if let Ok(name) = std::env::var("OPENROUTER_SITE_NAME") {
        if let Some(ref mut or) = config.provider.openrouter {
            or.site_name = Some(name);
        }
    }
    if let Ok(timeout) = std::env::var("OPENROUTER_TIMEOUT") {
        if let Some(ref mut or) = config.provider.openrouter {
            if let Ok(v) = timeout.parse() {
                or.timeout_secs = v;
            }
        }
    }
    if let Ok(retries) = std::env::var("OPENROUTER_MAX_RETRIES") {
        if let Some(ref mut or) = config.provider.openrouter {
            if let Ok(v) = retries.parse() {
                or.max_retries = v;
            }
        }
    }

    // Telegram overrides
    if let Ok(bot_token) = std::env::var("TELEGRAM_BOT_TOKEN") {
        let tg = config.channels.telegram.get_or_insert_with(|| {
            super::types::channel::TelegramConfig {
                bot_token: SecretString::from(String::new()),
                allow_from: Vec::new(),
                dm_policy: super::types::channel::DmPolicy::Open,
                groups: std::collections::HashMap::new(),
                use_long_polling: true,
                webhook_url: None,
                webhook_secret: None,
            }
        });
        tg.bot_token = SecretString::from(bot_token);
    }
    if let Ok(users) = std::env::var("TELEGRAM_ALLOWED_USERS") {
        if let Some(ref mut tg) = config.channels.telegram {
            tg.allow_from = users
                .split(',')
                .filter_map(|id| id.trim().parse().ok())
                .collect();
        }
    }
    if let Ok(v) = std::env::var("TELEGRAM_USE_WEBHOOK") {
        if let Some(ref mut tg) = config.channels.telegram {
            tg.use_long_polling = v != "true" && v != "1";
        }
    }
    if let Ok(url) = std::env::var("TELEGRAM_WEBHOOK_URL") {
        if let Some(ref mut tg) = config.channels.telegram {
            tg.webhook_url = Some(url);
        }
    }
    if let Ok(secret) = std::env::var("TELEGRAM_WEBHOOK_SECRET") {
        if let Some(ref mut tg) = config.channels.telegram {
            tg.webhook_secret = Some(secret);
        }
    }

    // Database overrides
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        let pg = config.storage.postgres.get_or_insert_with(|| {
            super::types::storage::PostgresConfig {
                url: SecretString::from(String::new()),
                max_connections: 5,
                connect_timeout_secs: 30,
                enable_pgvector: true,
            }
        });
        pg.url = SecretString::from(database_url);
        config.storage.backend = super::types::storage::StorageBackendType::Postgres;
    }
    if let Ok(max_conn) = std::env::var("DATABASE_MAX_CONNECTIONS") {
        if let Some(ref mut pg) = config.storage.postgres {
            if let Ok(v) = max_conn.parse() {
                pg.max_connections = v;
            }
        }
    }
    if let Ok(timeout) = std::env::var("DATABASE_TIMEOUT") {
        if let Some(ref mut pg) = config.storage.postgres {
            if let Ok(v) = timeout.parse() {
                pg.connect_timeout_secs = v;
            }
        }
    }

    // Sandbox overrides
    if let Ok(env_str) = std::env::var("EXECUTION_ENV") {
        if let Ok(exec_env) = env_str.parse() {
            config.sandbox.execution_env = exec_env;
        }
    }
    if let Ok(allowed_dir) = std::env::var("ALLOWED_DIR") {
        config.sandbox.allowed_dir = std::path::PathBuf::from(allowed_dir);
    }

    // Gateway overrides
    if let Ok(port) = std::env::var("GATEWAY_PORT") {
        if let Ok(port) = port.parse() {
            config.gateway.port = port;
        }
    }
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
