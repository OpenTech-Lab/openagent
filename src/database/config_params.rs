//! Config parameter storage and retrieval from PostgreSQL
//!
//! Provides a key-value store for runtime-editable configuration parameters.
//! Parameters are organized by category (e.g., "provider.openrouter", "sandbox").

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use sqlx::FromRow;
use tracing::trace;
use uuid::Uuid;

/// A stored configuration parameter
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConfigParam {
    pub id: Uuid,
    pub category: String,
    pub key: String,
    pub value: String,
    pub value_type: String,
    pub is_secret: bool,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Value type hint for config params
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigValueType {
    String,
    Number,
    Boolean,
    Json,
}

impl ConfigValueType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Json => "json",
        }
    }
}

impl std::str::FromStr for ConfigValueType {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "string" | "str" => Ok(Self::String),
            "number" | "num" | "int" | "float" => Ok(Self::Number),
            "boolean" | "bool" => Ok(Self::Boolean),
            "json" | "object" | "array" => Ok(Self::Json),
            _ => Err(crate::error::Error::Config(format!(
                "Invalid value type: {}. Valid: string, number, boolean, json",
                s
            ))),
        }
    }
}

/// Config parameter store backed by PostgreSQL
#[derive(Clone)]
pub struct ConfigParamStore {
    pool: PgPool,
}

impl ConfigParamStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a single parameter by category and key
    pub async fn get(&self, category: &str, key: &str) -> Result<Option<ConfigParam>> {
        let param: Option<ConfigParam> = sqlx::query_as(
            "SELECT * FROM config_params WHERE category = $1 AND key = $2",
        )
        .bind(category)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(param)
    }

    /// Get all parameters, optionally filtered by category
    pub async fn get_all(&self, category: Option<&str>) -> Result<Vec<ConfigParam>> {
        if let Some(cat) = category {
            let params: Vec<ConfigParam> = sqlx::query_as(
                "SELECT * FROM config_params WHERE category = $1 ORDER BY category, key",
            )
            .bind(cat)
            .fetch_all(&self.pool)
            .await?;
            Ok(params)
        } else {
            let params: Vec<ConfigParam> = sqlx::query_as(
                "SELECT * FROM config_params ORDER BY category, key",
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(params)
        }
    }

    /// Upsert (insert or update) a parameter
    pub async fn upsert(
        &self,
        category: &str,
        key: &str,
        value: &str,
        value_type: ConfigValueType,
        is_secret: bool,
        description: Option<&str>,
    ) -> Result<ConfigParam> {
        let param: ConfigParam = sqlx::query_as(
            r#"
            INSERT INTO config_params (category, key, value, value_type, is_secret, description)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (category, key) DO UPDATE SET
                value = EXCLUDED.value,
                value_type = EXCLUDED.value_type,
                is_secret = EXCLUDED.is_secret,
                description = COALESCE(EXCLUDED.description, config_params.description),
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(category)
        .bind(key)
        .bind(value)
        .bind(value_type.as_str())
        .bind(is_secret)
        .bind(description)
        .fetch_one(&self.pool)
        .await?;
        Ok(param)
    }

    /// Delete a parameter by category and key
    pub async fn delete(&self, category: &str, key: &str) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM config_params WHERE category = $1 AND key = $2")
                .bind(category)
                .bind(key)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Seed the config_params table from a Config struct.
    /// Only inserts params that don't already exist (does not overwrite).
    /// Returns the number of newly inserted parameters.
    pub async fn init_from_config(&self, config: &crate::config::Config) -> Result<usize> {
        let mut count = 0usize;

        // Agent settings
        count += self
            .seed_if_absent(
                "agent",
                "model",
                &config.agent.model,
                ConfigValueType::String,
                false,
                Some("Default agent model"),
            )
            .await?;
        count += self
            .seed_if_absent(
                "agent",
                "max_context_tokens",
                &config.agent.max_context_tokens.to_string(),
                ConfigValueType::Number,
                false,
                Some("Maximum context window tokens"),
            )
            .await?;

        // Provider: OpenRouter
        if let Some(ref or_config) = config.provider.openrouter {
            count += self
                .seed_if_absent(
                    "provider.openrouter",
                    "default_model",
                    &or_config.default_model,
                    ConfigValueType::String,
                    false,
                    Some("Default LLM model for OpenRouter"),
                )
                .await?;
            count += self
                .seed_if_absent(
                    "provider.openrouter",
                    "base_url",
                    &or_config.base_url,
                    ConfigValueType::String,
                    false,
                    Some("OpenRouter API base URL"),
                )
                .await?;
            count += self
                .seed_if_absent(
                    "provider.openrouter",
                    "timeout_secs",
                    &or_config.timeout_secs.to_string(),
                    ConfigValueType::Number,
                    false,
                    Some("Request timeout in seconds"),
                )
                .await?;
            count += self
                .seed_if_absent(
                    "provider.openrouter",
                    "max_retries",
                    &or_config.max_retries.to_string(),
                    ConfigValueType::Number,
                    false,
                    Some("Maximum retries on failure"),
                )
                .await?;
            if let Some(ref site_url) = or_config.site_url {
                count += self
                    .seed_if_absent(
                        "provider.openrouter",
                        "site_url",
                        site_url,
                        ConfigValueType::String,
                        false,
                        Some("Site URL for OpenRouter rankings"),
                    )
                    .await?;
            }
            if let Some(ref site_name) = or_config.site_name {
                count += self
                    .seed_if_absent(
                        "provider.openrouter",
                        "site_name",
                        site_name,
                        ConfigValueType::String,
                        false,
                        Some("Site name for OpenRouter rankings"),
                    )
                    .await?;
            }
        }

        // Sandbox settings
        count += self
            .seed_if_absent(
                "sandbox",
                "execution_env",
                &config.sandbox.execution_env.to_string(),
                ConfigValueType::String,
                false,
                Some("Execution environment: os, sandbox, container"),
            )
            .await?;
        count += self
            .seed_if_absent(
                "sandbox",
                "allowed_dir",
                &config.sandbox.allowed_dir.to_string_lossy(),
                ConfigValueType::String,
                false,
                Some("Allowed directory for file operations"),
            )
            .await?;
        count += self
            .seed_if_absent(
                "sandbox",
                "default_timeout_secs",
                &config.sandbox.default_timeout_secs.to_string(),
                ConfigValueType::Number,
                false,
                Some("Default execution timeout in seconds"),
            )
            .await?;

        // Container settings
        count += self
            .seed_if_absent(
                "sandbox.container",
                "image",
                &config.sandbox.container.image,
                ConfigValueType::String,
                false,
                Some("Docker image for container sandbox"),
            )
            .await?;
        count += self
            .seed_if_absent(
                "sandbox.container",
                "network",
                &config.sandbox.container.network,
                ConfigValueType::String,
                false,
                Some("Docker network mode"),
            )
            .await?;
        count += self
            .seed_if_absent(
                "sandbox.container",
                "memory_limit",
                &config.sandbox.container.memory_limit,
                ConfigValueType::String,
                false,
                Some("Docker memory limit"),
            )
            .await?;
        count += self
            .seed_if_absent(
                "sandbox.container",
                "cpu_limit",
                &config.sandbox.container.cpu_limit.to_string(),
                ConfigValueType::Number,
                false,
                Some("Docker CPU limit"),
            )
            .await?;

        // Gateway settings
        count += self
            .seed_if_absent(
                "gateway",
                "port",
                &config.gateway.port.to_string(),
                ConfigValueType::Number,
                false,
                Some("Gateway bind port"),
            )
            .await?;
        count += self
            .seed_if_absent(
                "gateway",
                "bind",
                &config.gateway.bind,
                ConfigValueType::String,
                false,
                Some("Gateway bind address"),
            )
            .await?;

        Ok(count)
    }

    /// Insert a param only if it doesn't already exist.
    /// Returns 1 if inserted, 0 if skipped.
    pub async fn seed_if_absent(
        &self,
        category: &str,
        key: &str,
        value: &str,
        value_type: ConfigValueType,
        is_secret: bool,
        description: Option<&str>,
    ) -> Result<usize> {
        let result = sqlx::query(
            r#"
            INSERT INTO config_params (category, key, value, value_type, is_secret, description)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (category, key) DO NOTHING
            "#,
        )
        .bind(category)
        .bind(key)
        .bind(value)
        .bind(value_type.as_str())
        .bind(is_secret)
        .bind(description)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }

    /// Apply DB config params as overrides to a Config struct.
    /// Only overrides non-secret, non-bootstrap fields.
    pub async fn apply_to_config(&self, config: &mut crate::config::Config) -> Result<()> {
        let params = self.get_all(None).await?;

        for param in &params {
            apply_param(config, param);
        }

        Ok(())
    }
}

/// Apply a single config param to the Config struct
fn apply_param(config: &mut crate::config::Config, param: &ConfigParam) {
    match (param.category.as_str(), param.key.as_str()) {
        // Agent
        ("agent", "model") => {
            config.agent.model = param.value.clone();
        }
        ("agent", "max_context_tokens") => {
            if let Ok(v) = param.value.parse::<u32>() {
                config.agent.max_context_tokens = v;
            }
        }
        // Provider: OpenRouter
        ("provider.openrouter", "default_model") => {
            if let Some(ref mut or) = config.provider.openrouter {
                or.default_model = param.value.clone();
            }
        }
        ("provider.openrouter", "base_url") => {
            if let Some(ref mut or) = config.provider.openrouter {
                or.base_url = param.value.clone();
            }
        }
        ("provider.openrouter", "timeout_secs") => {
            if let Some(ref mut or) = config.provider.openrouter {
                if let Ok(v) = param.value.parse::<u64>() {
                    or.timeout_secs = v;
                }
            }
        }
        ("provider.openrouter", "max_retries") => {
            if let Some(ref mut or) = config.provider.openrouter {
                if let Ok(v) = param.value.parse::<u32>() {
                    or.max_retries = v;
                }
            }
        }
        ("provider.openrouter", "site_url") => {
            if let Some(ref mut or) = config.provider.openrouter {
                or.site_url = Some(param.value.clone());
            }
        }
        ("provider.openrouter", "site_name") => {
            if let Some(ref mut or) = config.provider.openrouter {
                or.site_name = Some(param.value.clone());
            }
        }
        // Sandbox
        ("sandbox", "execution_env") => {
            if let Ok(env) = param.value.parse() {
                config.sandbox.execution_env = env;
            }
        }
        ("sandbox", "allowed_dir") => {
            config.sandbox.allowed_dir = std::path::PathBuf::from(&param.value);
        }
        ("sandbox", "default_timeout_secs") => {
            if let Ok(v) = param.value.parse::<u64>() {
                config.sandbox.default_timeout_secs = v;
            }
        }
        // Container
        ("sandbox.container", "image") => {
            config.sandbox.container.image = param.value.clone();
        }
        ("sandbox.container", "network") => {
            config.sandbox.container.network = param.value.clone();
        }
        ("sandbox.container", "memory_limit") => {
            config.sandbox.container.memory_limit = param.value.clone();
        }
        ("sandbox.container", "cpu_limit") => {
            if let Ok(v) = param.value.parse::<f64>() {
                config.sandbox.container.cpu_limit = v;
            }
        }
        // Gateway
        ("gateway", "port") => {
            if let Ok(v) = param.value.parse::<u16>() {
                config.gateway.port = v;
            }
        }
        ("gateway", "bind") => {
            config.gateway.bind = param.value.clone();
        }
        _ => {
            trace!("Unknown config param: {}.{}", param.category, param.key);
        }
    }
}
