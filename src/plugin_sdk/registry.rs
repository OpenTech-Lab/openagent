//! Plugin registry - Loading and managing plugins
//!
//! Following openclaw's pattern for plugin discovery and loading.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::manifest::{load_manifest, PluginManifest, PLUGIN_MANIFEST_FILENAME};
use super::traits::{DefaultPluginApi, Plugin};
use crate::error::{Error, Result};

/// Result of loading a plugin
#[derive(Debug)]
pub enum PluginLoadResult {
    /// Successfully loaded
    Ok {
        /// Plugin ID
        id: String,
        /// Plugin manifest
        manifest: PluginManifest,
    },
    /// Failed to load
    Error {
        /// Path to the plugin
        path: std::path::PathBuf,
        /// Error message
        error: String,
    },
}

/// Plugin registry - Manages loaded plugins
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<dyn Plugin>>,
    manifests: HashMap<String, PluginManifest>,
    api: DefaultPluginApi,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        PluginRegistry {
            plugins: HashMap::new(),
            manifests: HashMap::new(),
            api: DefaultPluginApi::new(),
        }
    }

    /// Register a plugin
    pub fn register(&mut self, plugin: Arc<dyn Plugin>) -> Result<()> {
        let manifest = plugin.manifest();
        let id = manifest.id.clone();

        // Check for duplicate
        if self.plugins.contains_key(&id) {
            return Err(Error::Config(format!(
                "Plugin '{}' is already registered",
                id
            )));
        }

        // Register the plugin
        plugin.register(&mut self.api)?;

        // Store plugin and manifest
        self.plugins.insert(id.clone(), plugin);
        self.manifests.insert(id, manifest);

        Ok(())
    }

    /// Unregister a plugin
    pub fn unregister(&mut self, id: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.remove(id) {
            plugin.unregister()?;
            self.manifests.remove(id);
        }
        Ok(())
    }

    /// Get a plugin by ID
    pub fn get(&self, id: &str) -> Option<&Arc<dyn Plugin>> {
        self.plugins.get(id)
    }

    /// Get a plugin manifest by ID
    pub fn get_manifest(&self, id: &str) -> Option<&PluginManifest> {
        self.manifests.get(id)
    }

    /// List all registered plugins
    pub fn list(&self) -> Vec<&PluginManifest> {
        self.manifests.values().collect()
    }

    /// Get the plugin API
    pub fn api(&self) -> &DefaultPluginApi {
        &self.api
    }

    /// Get mutable access to the plugin API
    pub fn api_mut(&mut self) -> &mut DefaultPluginApi {
        &mut self.api
    }

    /// Discover plugins in a directory
    pub fn discover(&self, dir: &Path) -> Vec<PluginLoadResult> {
        let mut results = Vec::new();

        if !dir.exists() || !dir.is_dir() {
            return results;
        }

        // Scan for plugin directories
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let manifest_path = path.join(PLUGIN_MANIFEST_FILENAME);
                    if manifest_path.exists() {
                        match load_manifest(&manifest_path) {
                            Ok(manifest) => {
                                results.push(PluginLoadResult::Ok {
                                    id: manifest.id.clone(),
                                    manifest,
                                });
                            }
                            Err(e) => {
                                results.push(PluginLoadResult::Error {
                                    path: manifest_path,
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        results
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Scan extension directories for plugins
#[allow(dead_code)]
pub fn scan_extension_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    // Check OPENAGENT_EXTENSIONS_DIR
    if let Ok(dir) = std::env::var("OPENAGENT_EXTENSIONS_DIR") {
        dirs.push(std::path::PathBuf::from(dir));
    }

    // Check ~/.openagent/extensions
    if let Some(home) = dirs::home_dir() {
        let ext_dir = home.join(".openagent").join("extensions");
        if ext_dir.exists() {
            dirs.push(ext_dir);
        }
    }

    // Check system extension directory
    #[cfg(target_os = "linux")]
    {
        let system_dir = std::path::PathBuf::from("/usr/share/openagent/extensions");
        if system_dir.exists() {
            dirs.push(system_dir);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let system_dir = std::path::PathBuf::from("/Library/Application Support/OpenAgent/extensions");
        if system_dir.exists() {
            dirs.push(system_dir);
        }
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_registry_new() {
        let registry = PluginRegistry::new();
        assert!(registry.list().is_empty());
    }
}
