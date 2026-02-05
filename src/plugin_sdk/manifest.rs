//! Plugin manifest - Metadata and configuration schema
//!
//! Following openclaw's pattern for plugin manifests.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin manifest filename
pub const PLUGIN_MANIFEST_FILENAME: &str = "openagent.plugin.json";

/// Plugin kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginKind {
    /// Channel plugin
    Channel,
    /// Provider plugin
    Provider,
    /// Storage plugin
    Storage,
    /// Tool/skill plugin
    Tool,
    /// Extension (multiple capabilities)
    Extension,
}

impl Default for PluginKind {
    fn default() -> Self {
        PluginKind::Extension
    }
}

/// Plugin manifest
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin ID
    pub id: String,
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    #[serde(default)]
    pub description: String,
    /// Plugin kind
    #[serde(default)]
    pub kind: PluginKind,
    /// Channels provided by this plugin
    #[serde(default)]
    pub channels: Vec<String>,
    /// Providers provided by this plugin
    #[serde(default)]
    pub providers: Vec<String>,
    /// Skills/tools provided by this plugin
    #[serde(default)]
    pub skills: Vec<String>,
    /// Configuration schema (JSON Schema)
    #[serde(default)]
    pub config_schema: serde_json::Value,
    /// UI hints for configuration
    #[serde(default)]
    pub ui_hints: HashMap<String, UiHint>,
    /// Required OpenAgent version
    pub openagent_version: Option<String>,
    /// Plugin author
    pub author: Option<String>,
    /// Plugin homepage
    pub homepage: Option<String>,
    /// Plugin repository
    pub repository: Option<String>,
    /// Plugin license
    pub license: Option<String>,
}

/// UI hint for configuration fields
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiHint {
    /// Display label
    pub label: Option<String>,
    /// Description/help text
    pub description: Option<String>,
    /// Input type (text, password, select, etc.)
    pub input_type: Option<String>,
    /// Placeholder text
    pub placeholder: Option<String>,
    /// Whether the field is required
    #[serde(default)]
    pub required: bool,
    /// Whether the field is secret
    #[serde(default)]
    pub secret: bool,
}

impl PluginManifest {
    /// Create a new plugin manifest
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        PluginManifest {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            ..Default::default()
        }
    }

    /// Set the plugin kind
    pub fn with_kind(mut self, kind: PluginKind) -> Self {
        self.kind = kind;
        self
    }

    /// Add a channel
    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channels.push(channel.into());
        self
    }

    /// Add a provider
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.providers.push(provider.into());
        self
    }

    /// Add a skill
    pub fn with_skill(mut self, skill: impl Into<String>) -> Self {
        self.skills.push(skill.into());
        self
    }
}

/// Load a plugin manifest from a file
pub fn load_manifest(path: &std::path::Path) -> crate::Result<PluginManifest> {
    let content = std::fs::read_to_string(path)?;
    let manifest: PluginManifest = serde_json::from_str(&content)?;
    Ok(manifest)
}

/// Result of loading a plugin manifest
#[allow(dead_code)]
#[derive(Debug)]
pub enum ManifestLoadResult {
    /// Successfully loaded
    Ok(PluginManifest),
    /// Failed to load
    Error {
        /// Path to the manifest
        path: std::path::PathBuf,
        /// Error message
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest_builder() {
        let manifest = PluginManifest::new("test-plugin", "Test Plugin", "1.0.0")
            .with_kind(PluginKind::Channel)
            .with_channel("telegram");

        assert_eq!(manifest.id, "test-plugin");
        assert_eq!(manifest.kind, PluginKind::Channel);
        assert_eq!(manifest.channels, vec!["telegram"]);
    }
}
