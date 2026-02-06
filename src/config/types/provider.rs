//! Provider configuration types
//!
//! Configuration for LLM providers (OpenRouter, Anthropic, OpenAI, etc.)

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Default provider
    #[serde(default = "default_provider")]
    pub default: String,
    /// OpenRouter configuration
    pub openrouter: Option<OpenRouterConfig>,
    /// Anthropic configuration
    pub anthropic: Option<AnthropicConfig>,
    /// OpenAI configuration
    pub openai: Option<OpenAIConfig>,
    /// Custom providers
    #[serde(default)]
    pub custom: HashMap<String, CustomProviderConfig>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        ProviderConfig {
            default: default_provider(),
            openrouter: None,
            anthropic: None,
            openai: None,
            custom: HashMap::new(),
        }
    }
}

fn default_provider() -> String {
    "openrouter".to_string()
}

fn default_secret() -> SecretString {
    SecretString::from(String::new())
}

/// OpenRouter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    /// API key
    #[serde(skip_serializing, default = "default_secret")]
    pub api_key: SecretString,
    /// Default model
    #[serde(default = "default_openrouter_model")]
    pub default_model: String,
    /// Base URL
    #[serde(default = "default_openrouter_url")]
    pub base_url: String,
    /// Site URL for rankings
    pub site_url: Option<String>,
    /// Site name for rankings
    pub site_name: Option<String>,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Maximum retries
    #[serde(default = "default_retries")]
    pub max_retries: u32,
}

fn default_openrouter_model() -> String {
    "anthropic/claude-sonnet-4".to_string()
}

fn default_openrouter_url() -> String {
    "https://openrouter.ai/api/v1".to_string()
}

fn default_timeout() -> u64 {
    300
}

fn default_retries() -> u32 {
    3
}

/// Anthropic configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// API key
    #[serde(skip_serializing, default = "default_secret")]
    pub api_key: SecretString,
    /// Default model
    #[serde(default = "default_anthropic_model")]
    pub default_model: String,
    /// Base URL
    #[serde(default = "default_anthropic_url")]
    pub base_url: String,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_anthropic_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_anthropic_url() -> String {
    "https://api.anthropic.com".to_string()
}

/// OpenAI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key
    #[serde(skip_serializing, default = "default_secret")]
    pub api_key: SecretString,
    /// Default model
    #[serde(default = "default_openai_model")]
    pub default_model: String,
    /// Base URL
    #[serde(default = "default_openai_url")]
    pub base_url: String,
    /// Organization ID
    pub organization: Option<String>,
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_openai_model() -> String {
    "gpt-4o".to_string()
}

fn default_openai_url() -> String {
    "https://api.openai.com/v1".to_string()
}

/// Custom provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProviderConfig {
    /// Provider ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Base URL
    pub base_url: String,
    /// API key
    #[serde(skip_serializing, default)]
    pub api_key: Option<SecretString>,
    /// Default model
    pub default_model: String,
    /// Whether the provider is OpenAI-compatible
    #[serde(default)]
    pub openai_compatible: bool,
    /// Custom headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Model failover configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// Enable automatic failover
    #[serde(default)]
    pub enabled: bool,
    /// Fallback models in order of preference
    #[serde(default)]
    pub fallback_models: Vec<String>,
    /// Cooldown period in seconds after a failure
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
    /// Maximum failures before cooldown
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,
}

fn default_cooldown() -> u64 {
    300
}

fn default_max_failures() -> u32 {
    3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_default() {
        let config = ProviderConfig::default();
        assert_eq!(config.default, "openrouter");
    }
}
