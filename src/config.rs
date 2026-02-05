//! Configuration management for OpenAgent
//!
//! Loads configuration from environment variables and config files.

use crate::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::path::PathBuf;

/// Execution environment for sandboxed code execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionEnv {
    /// Run in OS directory with restricted permissions
    Os,
    /// Run in WebAssembly sandbox (recommended)
    #[default]
    Sandbox,
    /// Run in ephemeral Docker container
    Container,
}

impl std::str::FromStr for ExecutionEnv {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "os" => Ok(ExecutionEnv::Os),
            "sandbox" | "wasm" => Ok(ExecutionEnv::Sandbox),
            "container" | "docker" => Ok(ExecutionEnv::Container),
            _ => Err(Error::Config(format!(
                "Invalid execution environment: {}. Valid options: os, sandbox, container",
                s
            ))),
        }
    }
}

impl std::fmt::Display for ExecutionEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionEnv::Os => write!(f, "os"),
            ExecutionEnv::Sandbox => write!(f, "sandbox"),
            ExecutionEnv::Container => write!(f, "container"),
        }
    }
}

/// OpenRouter configuration
#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    /// API key for OpenRouter
    pub api_key: SecretString,
    /// Default model to use
    pub default_model: String,
    /// Site URL for rankings
    pub site_url: Option<String>,
    /// Site name for rankings
    pub site_name: Option<String>,
    /// Base URL for OpenRouter API
    pub base_url: String,
}

/// Telegram bot configuration
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    /// Bot token from BotFather
    pub bot_token: SecretString,
    /// Allowed user IDs (empty = allow all)
    pub allowed_users: Vec<i64>,
    /// Use long polling instead of webhook
    pub use_long_polling: bool,
    /// Webhook URL (if not using long polling)
    pub webhook_url: Option<String>,
}

/// PostgreSQL database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url: SecretString,
    /// Maximum connections in pool
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connect_timeout_secs: u64,
}

/// OpenSearch configuration
#[derive(Debug, Clone)]
pub struct OpenSearchConfig {
    /// OpenSearch URL
    pub url: String,
    /// Username for authentication
    pub username: Option<String>,
    /// Password for authentication
    pub password: Option<SecretString>,
    /// Index prefix
    pub index_prefix: String,
}

/// Docker/container configuration
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    /// Docker image to use
    pub image: String,
    /// Network mode (none, bridge, host)
    pub network: String,
    /// Memory limit
    pub memory_limit: String,
    /// CPU limit
    pub cpu_limit: f64,
}

/// Sandbox/execution configuration
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Execution environment type
    pub execution_env: ExecutionEnv,
    /// Allowed directory for file operations
    pub allowed_dir: PathBuf,
    /// Container settings
    pub container: ContainerConfig,
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Log level filter
    pub level: String,
    /// Log format (pretty, json)
    pub format: String,
}

/// Main application configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// OpenRouter settings
    pub openrouter: OpenRouterConfig,
    /// Telegram bot settings
    pub telegram: TelegramConfig,
    /// PostgreSQL database settings
    pub database: DatabaseConfig,
    /// OpenSearch settings
    pub opensearch: OpenSearchConfig,
    /// Sandbox/execution settings
    pub sandbox: SandboxConfig,
    /// Logging settings
    pub log: LogConfig,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        // Load .env file if it exists
        dotenvy::dotenv().ok();

        Ok(Config {
            openrouter: OpenRouterConfig {
                api_key: SecretString::from(std::env::var("OPENROUTER_API_KEY")?),
                default_model: std::env::var("DEFAULT_MODEL")
                    .unwrap_or_else(|_| "anthropic/claude-3.5-sonnet".to_string()),
                site_url: std::env::var("OPENROUTER_SITE_URL").ok(),
                site_name: std::env::var("OPENROUTER_SITE_NAME").ok(),
                base_url: std::env::var("OPENROUTER_BASE_URL")
                    .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string()),
            },
            telegram: TelegramConfig {
                bot_token: SecretString::from(std::env::var("TELEGRAM_BOT_TOKEN")?),
                allowed_users: std::env::var("TELEGRAM_ALLOWED_USERS")
                    .unwrap_or_default()
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect(),
                use_long_polling: std::env::var("USE_LONG_POLLING")
                    .map(|v| v.to_lowercase() == "true")
                    .unwrap_or(true),
                webhook_url: std::env::var("WEBHOOK_URL").ok(),
            },
            database: DatabaseConfig {
                url: SecretString::from(std::env::var("DATABASE_URL")?),
                max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                    .unwrap_or_else(|_| "10".to_string())
                    .parse()
                    .unwrap_or(10),
                connect_timeout_secs: std::env::var("DATABASE_CONNECT_TIMEOUT")
                    .unwrap_or_else(|_| "30".to_string())
                    .parse()
                    .unwrap_or(30),
            },
            opensearch: OpenSearchConfig {
                url: std::env::var("OPENSEARCH_URL")
                    .unwrap_or_else(|_| "https://localhost:9200".to_string()),
                username: std::env::var("OPENSEARCH_USERNAME").ok(),
                password: std::env::var("OPENSEARCH_PASSWORD").ok().map(SecretString::from),
                index_prefix: std::env::var("OPENSEARCH_INDEX_PREFIX")
                    .unwrap_or_else(|_| "openagent".to_string()),
            },
            sandbox: SandboxConfig {
                execution_env: std::env::var("EXECUTION_ENV")
                    .unwrap_or_else(|_| "sandbox".to_string())
                    .parse()?,
                allowed_dir: PathBuf::from(
                    std::env::var("ALLOWED_DIR").unwrap_or_else(|_| "./workspace".to_string()),
                ),
                container: ContainerConfig {
                    image: std::env::var("DOCKER_IMAGE")
                        .unwrap_or_else(|_| "python:3.12-slim".to_string()),
                    network: std::env::var("DOCKER_NETWORK")
                        .unwrap_or_else(|_| "none".to_string()),
                    memory_limit: std::env::var("DOCKER_MEMORY_LIMIT")
                        .unwrap_or_else(|_| "512m".to_string()),
                    cpu_limit: std::env::var("DOCKER_CPU_LIMIT")
                        .unwrap_or_else(|_| "1.0".to_string())
                        .parse()
                        .unwrap_or(1.0),
                },
            },
            log: LogConfig {
                level: std::env::var("RUST_LOG")
                    .unwrap_or_else(|_| "info,openagent=debug".to_string()),
                format: std::env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string()),
            },
        })
    }

    /// Create a minimal config for testing or CLI commands that don't need full config
    pub fn minimal() -> Self {
        Config {
            openrouter: OpenRouterConfig {
                api_key: SecretString::from(""),
                default_model: "anthropic/claude-3.5-sonnet".to_string(),
                site_url: None,
                site_name: None,
                base_url: "https://openrouter.ai/api/v1".to_string(),
            },
            telegram: TelegramConfig {
                bot_token: SecretString::from(""),
                allowed_users: vec![],
                use_long_polling: true,
                webhook_url: None,
            },
            database: DatabaseConfig {
                url: SecretString::from(""),
                max_connections: 5,
                connect_timeout_secs: 30,
            },
            opensearch: OpenSearchConfig {
                url: "https://localhost:9200".to_string(),
                username: None,
                password: None,
                index_prefix: "openagent".to_string(),
            },
            sandbox: SandboxConfig {
                execution_env: ExecutionEnv::Sandbox,
                allowed_dir: PathBuf::from("./workspace"),
                container: ContainerConfig {
                    image: "python:3.12-slim".to_string(),
                    network: "none".to_string(),
                    memory_limit: "512m".to_string(),
                    cpu_limit: 1.0,
                },
            },
            log: LogConfig {
                level: "info".to_string(),
                format: "pretty".to_string(),
            },
        }
    }

    /// Validate that all required configuration is present
    pub fn validate(&self) -> Result<()> {
        if self.openrouter.api_key.expose_secret().is_empty() {
            return Err(Error::Config("OPENROUTER_API_KEY is required".to_string()));
        }
        if self.telegram.bot_token.expose_secret().is_empty() {
            return Err(Error::Config("TELEGRAM_BOT_TOKEN is required".to_string()));
        }
        if self.database.url.expose_secret().is_empty() {
            return Err(Error::Config("DATABASE_URL is required".to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_env_parsing() {
        assert_eq!("os".parse::<ExecutionEnv>().unwrap(), ExecutionEnv::Os);
        assert_eq!(
            "sandbox".parse::<ExecutionEnv>().unwrap(),
            ExecutionEnv::Sandbox
        );
        assert_eq!(
            "container".parse::<ExecutionEnv>().unwrap(),
            ExecutionEnv::Container
        );
        assert_eq!(
            "docker".parse::<ExecutionEnv>().unwrap(),
            ExecutionEnv::Container
        );
        assert!("invalid".parse::<ExecutionEnv>().is_err());
    }

    #[test]
    fn test_minimal_config() {
        let config = Config::minimal();
        assert!(config.validate().is_err()); // Should fail validation
    }
}
