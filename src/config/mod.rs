//! Configuration module - Modular configuration management
//!
//! Following openclaw's pattern of splitting configuration into focused modules:
//! - types/mod.rs: Core configuration types (Config, AgentConfig, etc.)
//! - types/provider.rs: LLM provider configuration
//! - types/channel.rs: Channel-specific configuration
//! - types/storage.rs: Storage backend configuration
//! - types/sandbox.rs: Sandbox/execution configuration
//! - io.rs: Configuration loading and saving
//! - validation.rs: Configuration validation
//! - paths.rs: Configuration file paths

mod io;
mod paths;
mod types;
mod validation;

// Re-export core config types
pub use types::{Config, AgentConfig, GatewayConfig, ThinkingLevel};

// Re-export channel types
pub use types::channel::{
    ChannelsConfig, TelegramConfig, DiscordConfig, SlackConfig, WhatsAppConfig, DmPolicy,
};

// Re-export provider types
pub use types::provider::{
    ProviderConfig, OpenRouterConfig, AnthropicConfig, OpenAIConfig, FailoverConfig,
};

// Re-export storage types
pub use types::storage::{
    StorageConfig, PostgresConfig, SqliteConfig, EmbeddingConfig,
};

// Backward compatibility aliases
pub type DatabaseConfig = PostgresConfig;

// Re-export sandbox types
pub use types::sandbox::{
    SandboxConfig, ExecutionEnv, ContainerConfig, WasmConfig,
};

// Re-export IO and utilities
pub use io::{load_config, save_config, apply_env_overrides, ConfigSnapshot};
pub use paths::{config_dir, config_path, state_dir, workspace_dir};
pub use validation::{validate_config, ConfigValidationResult};
