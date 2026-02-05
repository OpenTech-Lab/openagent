//! Plugin SDK - External integration framework
//!
//! This module provides the SDK for building OpenAgent plugins following
//! openclaw's plugin architecture. Plugins can:
//!
//! - Add new LLM providers
//! - Add new messaging channels
//! - Add new storage backends
//! - Add new tools/skills
//!
//! ## Creating a Plugin
//!
//! ```rust,no_run
//! use openagent::plugin_sdk::{Plugin, PluginManifest, PluginApi};
//!
//! pub struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn manifest(&self) -> PluginManifest {
//!         PluginManifest {
//!             id: "my-plugin".to_string(),
//!             name: "My Plugin".to_string(),
//!             version: "1.0.0".to_string(),
//!             ..Default::default()
//!         }
//!     }
//!
//!     fn register(&self, api: &mut dyn PluginApi) {
//!         // Register your channels, providers, tools, etc.
//!     }
//! }
//! ```

mod manifest;
mod registry;
mod traits;

pub use manifest::{PluginManifest, PluginKind};
pub use registry::{PluginRegistry, PluginLoadResult};
pub use traits::{Plugin, PluginApi};

// Re-export core traits that plugins will need
pub use crate::core::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelPlugin, ChannelReply,
    CodeExecutor, ExecutionRequest, ExecutionResult, Language,
    LlmProvider, LlmResponse, GenerationOptions,
    MemoryBackend, SearchBackend, StorageBackend,
    Message, Role,
};

// Re-export config types that plugins might need
pub use crate::config::{
    Config, ChannelsConfig, ProviderConfig, StorageConfig, SandboxConfig,
    TelegramConfig, DiscordConfig, OpenRouterConfig, PostgresConfig, ExecutionEnv,
};

// Re-export error types
pub use crate::error::{Error, Result};
