//! Plugin traits - Core interfaces for plugins
//!
//! Following openclaw's pattern for plugin interfaces.

use std::collections::HashMap;
use std::sync::Arc;

use super::manifest::PluginManifest;
use crate::core::{Channel, CodeExecutor, LlmProvider, MemoryBackend};
use crate::error::Result;

/// Plugin trait - Main interface for plugins
pub trait Plugin: Send + Sync {
    /// Get the plugin manifest
    fn manifest(&self) -> PluginManifest;

    /// Get the plugin ID
    fn id(&self) -> &str {
        // Note: This returns a temporary, so plugins should implement this themselves
        // or cache the manifest
        "unknown"
    }

    /// Register the plugin with the API
    fn register(&self, api: &mut dyn PluginApi) -> Result<()>;

    /// Unregister/cleanup when the plugin is unloaded
    fn unregister(&self) -> Result<()> {
        Ok(())
    }
}

/// Plugin API - Interface provided to plugins for registration
pub trait PluginApi: Send + Sync {
    /// Register a channel
    fn register_channel(&mut self, id: &str, channel: Arc<dyn Channel>) -> Result<()>;

    /// Unregister a channel
    fn unregister_channel(&mut self, id: &str) -> Result<()>;

    /// Register an LLM provider
    fn register_provider(&mut self, id: &str, provider: Arc<dyn LlmProvider>) -> Result<()>;

    /// Unregister a provider
    fn unregister_provider(&mut self, id: &str) -> Result<()>;

    /// Register a storage backend
    fn register_storage(&mut self, id: &str, storage: Arc<dyn MemoryBackend>) -> Result<()>;

    /// Unregister a storage backend
    fn unregister_storage(&mut self, id: &str) -> Result<()>;

    /// Register a code executor
    fn register_executor(&mut self, id: &str, executor: Arc<dyn CodeExecutor>) -> Result<()>;

    /// Unregister an executor
    fn unregister_executor(&mut self, id: &str) -> Result<()>;

    /// Get plugin configuration
    fn get_config(&self, plugin_id: &str) -> Option<&serde_json::Value>;

    /// Log a message
    fn log(&self, level: LogLevel, message: &str);
}

/// Log level for plugin logging
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Trace level
    Trace,
    /// Debug level
    Debug,
    /// Info level
    Info,
    /// Warning level
    Warn,
    /// Error level
    Error,
}

/// Default plugin API implementation
pub struct DefaultPluginApi {
    channels: HashMap<String, Arc<dyn Channel>>,
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    storages: HashMap<String, Arc<dyn MemoryBackend>>,
    executors: HashMap<String, Arc<dyn CodeExecutor>>,
    configs: HashMap<String, serde_json::Value>,
}

impl DefaultPluginApi {
    /// Create a new plugin API
    pub fn new() -> Self {
        DefaultPluginApi {
            channels: HashMap::new(),
            providers: HashMap::new(),
            storages: HashMap::new(),
            executors: HashMap::new(),
            configs: HashMap::new(),
        }
    }

    /// Set plugin configuration
    pub fn set_config(&mut self, plugin_id: &str, config: serde_json::Value) {
        self.configs.insert(plugin_id.to_string(), config);
    }

    /// Get all registered channels
    pub fn channels(&self) -> &HashMap<String, Arc<dyn Channel>> {
        &self.channels
    }

    /// Get all registered providers
    pub fn providers(&self) -> &HashMap<String, Arc<dyn LlmProvider>> {
        &self.providers
    }

    /// Get all registered storages
    pub fn storages(&self) -> &HashMap<String, Arc<dyn MemoryBackend>> {
        &self.storages
    }

    /// Get all registered executors
    pub fn executors(&self) -> &HashMap<String, Arc<dyn CodeExecutor>> {
        &self.executors
    }
}

impl Default for DefaultPluginApi {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginApi for DefaultPluginApi {
    fn register_channel(&mut self, id: &str, channel: Arc<dyn Channel>) -> Result<()> {
        self.channels.insert(id.to_string(), channel);
        Ok(())
    }

    fn unregister_channel(&mut self, id: &str) -> Result<()> {
        self.channels.remove(id);
        Ok(())
    }

    fn register_provider(&mut self, id: &str, provider: Arc<dyn LlmProvider>) -> Result<()> {
        self.providers.insert(id.to_string(), provider);
        Ok(())
    }

    fn unregister_provider(&mut self, id: &str) -> Result<()> {
        self.providers.remove(id);
        Ok(())
    }

    fn register_storage(&mut self, id: &str, storage: Arc<dyn MemoryBackend>) -> Result<()> {
        self.storages.insert(id.to_string(), storage);
        Ok(())
    }

    fn unregister_storage(&mut self, id: &str) -> Result<()> {
        self.storages.remove(id);
        Ok(())
    }

    fn register_executor(&mut self, id: &str, executor: Arc<dyn CodeExecutor>) -> Result<()> {
        self.executors.insert(id.to_string(), executor);
        Ok(())
    }

    fn unregister_executor(&mut self, id: &str) -> Result<()> {
        self.executors.remove(id);
        Ok(())
    }

    fn get_config(&self, plugin_id: &str) -> Option<&serde_json::Value> {
        self.configs.get(plugin_id)
    }

    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Trace => tracing::trace!("{}", message),
            LogLevel::Debug => tracing::debug!("{}", message),
            LogLevel::Info => tracing::info!("{}", message),
            LogLevel::Warn => tracing::warn!("{}", message),
            LogLevel::Error => tracing::error!("{}", message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_plugin_api() {
        let api = DefaultPluginApi::new();
        assert!(api.channels().is_empty());
        assert!(api.providers().is_empty());
    }
}
