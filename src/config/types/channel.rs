//! Channel configuration types
//!
//! Configuration for messaging channels (Telegram, Discord, Slack, etc.)

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// All channel configurations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    /// Telegram configuration
    pub telegram: Option<TelegramConfig>,
    /// Discord configuration
    pub discord: Option<DiscordConfig>,
    /// Slack configuration
    pub slack: Option<SlackConfig>,
    /// WhatsApp configuration
    pub whatsapp: Option<WhatsAppConfig>,
    /// WebChat configuration
    #[serde(default)]
    pub webchat: WebChatConfig,
    /// Custom channel configurations
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Telegram bot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token
    #[serde(skip_serializing)]
    pub bot_token: SecretString,
    /// Allowed user IDs (empty = allow all with pairing)
    #[serde(default)]
    pub allow_from: Vec<i64>,
    /// DM policy
    #[serde(default)]
    pub dm_policy: DmPolicy,
    /// Group configurations
    #[serde(default)]
    pub groups: HashMap<String, GroupConfig>,
    /// Use long polling instead of webhook
    #[serde(default = "default_true")]
    pub use_long_polling: bool,
    /// Webhook URL (if not using long polling)
    pub webhook_url: Option<String>,
    /// Webhook secret
    pub webhook_secret: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Discord bot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Bot token
    #[serde(skip_serializing)]
    pub token: SecretString,
    /// Application ID
    pub application_id: Option<String>,
    /// DM policy
    #[serde(default)]
    pub dm: DmConfig,
    /// Guild configurations
    #[serde(default)]
    pub guilds: HashMap<String, GuildConfig>,
    /// Maximum media size in MB
    #[serde(default = "default_media_size")]
    pub media_max_mb: u32,
}

fn default_media_size() -> u32 {
    8
}

/// Slack configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Bot token
    #[serde(skip_serializing)]
    pub bot_token: SecretString,
    /// App token
    #[serde(skip_serializing)]
    pub app_token: SecretString,
    /// Signing secret
    pub signing_secret: Option<String>,
    /// DM policy
    #[serde(default)]
    pub dm: DmConfig,
    /// Channel configurations
    #[serde(default)]
    pub channels: HashMap<String, ChannelSpecificConfig>,
}

/// WhatsApp configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    /// Allowed phone numbers
    #[serde(default)]
    pub allow_from: Vec<String>,
    /// Group configurations
    #[serde(default)]
    pub groups: HashMap<String, GroupConfig>,
    /// Credentials directory
    pub credentials_dir: Option<String>,
}

/// WebChat configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebChatConfig {
    /// Enable WebChat
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Path for WebChat UI
    #[serde(default = "default_webchat_path")]
    pub path: String,
}

fn default_webchat_path() -> String {
    "/chat".to_string()
}

/// DM (Direct Message) policy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DmPolicy {
    /// Require pairing code
    #[default]
    Pairing,
    /// Use allowlist
    Allowlist,
    /// Allow all
    Open,
    /// Disable DMs
    Disabled,
}

/// DM configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DmConfig {
    /// DM policy
    #[serde(default)]
    pub policy: DmPolicy,
    /// Allowed senders
    #[serde(default)]
    pub allow_from: Vec<String>,
}

/// Group configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConfig {
    /// Whether the group is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Require mention to activate
    #[serde(default = "default_true")]
    pub require_mention: bool,
    /// Tool access configuration
    #[serde(default)]
    pub tools: ToolAccessConfig,
}

impl Default for GroupConfig {
    fn default() -> Self {
        GroupConfig {
            enabled: true,
            require_mention: true,
            tools: ToolAccessConfig::default(),
        }
    }
}

/// Guild (server) configuration for Discord
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuildConfig {
    /// Whether the guild is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Allowed channels
    #[serde(default)]
    pub channels: HashMap<String, ChannelSpecificConfig>,
}

/// Channel-specific configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelSpecificConfig {
    /// Whether the channel is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Require mention
    #[serde(default)]
    pub require_mention: bool,
    /// Tool access
    #[serde(default)]
    pub tools: ToolAccessConfig,
}

/// Tool access configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolAccessConfig {
    /// Allowed tools (whitelist)
    #[serde(default)]
    pub allow: Vec<String>,
    /// Denied tools (blacklist)
    #[serde(default)]
    pub deny: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dm_policy_default() {
        let policy = DmPolicy::default();
        assert_eq!(policy, DmPolicy::Pairing);
    }

    #[test]
    fn test_webchat_config_default() {
        let config = WebChatConfig::default();
        assert!(config.enabled);
        assert_eq!(config.path, "/chat");
    }
}
