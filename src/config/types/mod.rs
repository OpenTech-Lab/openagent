//! Configuration types module
//!
//! Re-exports all configuration types following openclaw's pattern.

pub mod channel;
pub mod provider;
pub mod sandbox;
pub mod storage;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Agent configuration
    #[serde(default)]
    pub agent: AgentConfig,

    /// Provider configuration (OpenRouter, etc.)
    #[serde(default)]
    pub provider: provider::ProviderConfig,

    /// Channel configurations
    #[serde(default)]
    pub channels: channel::ChannelsConfig,

    /// Storage configuration
    #[serde(default)]
    pub storage: storage::StorageConfig,

    /// Sandbox configuration
    #[serde(default)]
    pub sandbox: sandbox::SandboxConfig,

    /// Gateway configuration
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Plugin configurations
    #[serde(default)]
    pub plugins: HashMap<String, serde_json::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            agent: AgentConfig::default(),
            provider: provider::ProviderConfig::default(),
            channels: channel::ChannelsConfig::default(),
            storage: storage::StorageConfig::default(),
            sandbox: sandbox::SandboxConfig::default(),
            gateway: GatewayConfig::default(),
            plugins: HashMap::new(),
        }
    }
}

impl Config {
    /// Load configuration from environment variables and files
    ///
    /// This provides backward compatibility with the old `from_env()` pattern.
    /// It loads configuration from:
    /// 1. Default values
    /// 2. Config file (if present)
    /// 3. Environment variable overrides
    pub fn from_env() -> crate::error::Result<Self> {
        crate::config::load_config()
    }
}

/// Agent-level configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Default model to use
    #[serde(default = "default_model")]
    pub model: String,
    /// Agent workspace directory
    #[serde(default = "default_workspace")]
    pub workspace: PathBuf,
    /// System prompt file (SOUL.md, AGENTS.md, etc.)
    pub system_prompt_file: Option<PathBuf>,
    /// Maximum context tokens
    #[serde(default = "default_max_context")]
    pub max_context_tokens: u32,
    /// Default thinking level
    #[serde(default)]
    pub thinking_level: ThinkingLevel,
    /// Enable verbose output
    #[serde(default)]
    pub verbose: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            model: default_model(),
            workspace: default_workspace(),
            system_prompt_file: None,
            max_context_tokens: default_max_context(),
            thinking_level: ThinkingLevel::default(),
            verbose: false,
        }
    }
}

fn default_model() -> String {
    "anthropic/claude-sonnet-4".to_string()
}

fn default_workspace() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".openagent").join("workspace"))
        .unwrap_or_else(|| PathBuf::from("./workspace"))
}

fn default_max_context() -> u32 {
    200_000
}

/// Thinking/reasoning level
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    /// No extended thinking
    Off,
    /// Minimal thinking
    Minimal,
    /// Low thinking
    Low,
    /// Medium thinking (default)
    #[default]
    Medium,
    /// High thinking
    High,
    /// Extra high thinking
    XHigh,
}

/// Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Port to bind to
    #[serde(default = "default_port")]
    pub port: u16,
    /// Bind address
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Enable WebSocket
    #[serde(default = "default_true")]
    pub websocket: bool,
    /// Authentication configuration
    #[serde(default)]
    pub auth: AuthConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        GatewayConfig {
            port: default_port(),
            bind: default_bind(),
            websocket: true,
            auth: AuthConfig::default(),
        }
    }
}

fn default_port() -> u16 {
    18789
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_true() -> bool {
    true
}

/// Authentication configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication mode
    #[serde(default)]
    pub mode: AuthMode,
    /// Shared password (for password mode)
    pub password: Option<String>,
    /// Allowed tokens
    #[serde(default)]
    pub tokens: Vec<String>,
}

/// Authentication mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    /// No authentication (local only)
    #[default]
    None,
    /// Password authentication
    Password,
    /// Token-based authentication
    Token,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.agent.model, "anthropic/claude-sonnet-4");
        assert_eq!(config.gateway.port, 18789);
    }
}
