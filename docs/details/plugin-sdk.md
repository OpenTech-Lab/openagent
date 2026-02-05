# Plugin SDK

OpenAgent provides a Plugin SDK for extending the framework with custom providers, channels, storage backends, and executors. This document describes how to build and use plugins.

## Overview

The Plugin SDK enables:
- **Custom LLM providers** - Integrate new AI models
- **Messaging channels** - Add new communication platforms
- **Storage backends** - Implement custom persistence
- **Code executors** - Add new execution environments

## Plugin Structure

A plugin consists of:

```
my-plugin/
├── plugin.json          # Plugin manifest
├── src/
│   └── lib.rs          # Plugin implementation
└── Cargo.toml          # Rust dependencies
```

## Plugin Manifest

The `plugin.json` file describes your plugin:

```json
{
  "id": "my-custom-plugin",
  "name": "My Custom Plugin",
  "version": "1.0.0",
  "description": "Adds custom functionality to OpenAgent",
  "author": "Your Name",
  "license": "MIT",
  "homepage": "https://github.com/you/my-plugin",
  "repository": "https://github.com/you/my-plugin",
  "kind": "provider",
  "entry_point": "libmy_plugin.so",
  "config_schema": {
    "type": "object",
    "properties": {
      "api_key": {
        "type": "string",
        "description": "API key for the service"
      }
    },
    "required": ["api_key"]
  },
  "dependencies": [],
  "capabilities": ["streaming", "tools"],
  "min_openagent_version": "0.1.0"
}
```

### Manifest Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique plugin identifier |
| `name` | string | Human-readable name |
| `version` | string | Semantic version |
| `kind` | string | `provider`, `channel`, `storage`, `executor`, `tool`, or `mixed` |
| `entry_point` | string | Compiled library filename |
| `config_schema` | object | JSON Schema for plugin configuration |
| `capabilities` | array | List of capabilities provided |
| `min_openagent_version` | string | Minimum OpenAgent version required |

## Plugin Trait

All plugins implement the `Plugin` trait:

```rust
use openagent::plugin_sdk::{Plugin, PluginManifest, PluginApi};
use openagent::error::Result;

pub struct MyPlugin {
    config: MyPluginConfig,
}

impl Plugin for MyPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            id: "my-plugin".to_string(),
            name: "My Plugin".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Provider,
            ..Default::default()
        }
    }

    fn on_load(&mut self, api: &mut dyn PluginApi) -> Result<()> {
        // Register components with the API
        let provider = Box::new(MyProvider::new(&self.config));
        api.register_provider(provider);
        Ok(())
    }

    fn on_unload(&mut self) -> Result<()> {
        // Cleanup when plugin is unloaded
        Ok(())
    }
}
```

## PluginApi

The `PluginApi` provides registration methods:

```rust
pub trait PluginApi {
    /// Register a new LLM provider
    fn register_provider(&mut self, provider: Box<dyn LlmProvider>);
    
    /// Register a new messaging channel
    fn register_channel(&mut self, channel: Box<dyn Channel>);
    
    /// Register a new storage backend
    fn register_storage(&mut self, storage: Box<dyn StorageBackend>);
    
    /// Register a new code executor
    fn register_executor(&mut self, executor: Box<dyn CodeExecutor>);
    
    /// Get the current configuration
    fn config(&self) -> &Config;
    
    /// Log a message
    fn log(&self, level: LogLevel, message: &str);
}
```

## Creating a Provider Plugin

### Example: Custom LLM Provider

```rust
use openagent::core::{LlmProvider, Message, GenerationOptions, LlmResponse, StreamingChunk};
use openagent::plugin_sdk::{Plugin, PluginManifest, PluginApi, PluginKind};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

// Configuration
#[derive(Debug, Clone, Deserialize)]
pub struct MyProviderConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

// Provider implementation
pub struct MyProvider {
    config: MyProviderConfig,
    client: reqwest::Client,
}

impl MyProvider {
    pub fn new(config: MyProviderConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for MyProvider {
    fn name(&self) -> &str {
        "my-provider"
    }

    async fn generate(
        &self,
        messages: Vec<Message>,
        options: GenerationOptions,
    ) -> Result<LlmResponse> {
        // Make API call to your LLM service
        let response = self.client
            .post(&format!("{}/chat", self.config.base_url))
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&ChatRequest {
                model: options.model.unwrap_or(self.config.model.clone()),
                messages: messages.into_iter().map(|m| m.into()).collect(),
                max_tokens: options.max_tokens,
                temperature: options.temperature,
            })
            .send()
            .await?;

        let data: ChatResponse = response.json().await?;
        
        Ok(LlmResponse {
            content: data.content,
            model: data.model,
            finish_reason: data.finish_reason,
            usage: data.usage.map(|u| UsageStats {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                cost: None,
            }),
            tool_calls: vec![],
        })
    }

    fn stream(
        &self,
        messages: Vec<Message>,
        options: GenerationOptions,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamingChunk>> + Send + '_>> {
        Box::pin(async_stream::stream! {
            // Implement streaming logic
            yield Ok(StreamingChunk {
                delta: "Hello".to_string(),
                finish_reason: None,
                usage: None,
            });
        })
    }
}

// Plugin wrapper
pub struct MyProviderPlugin {
    config: MyProviderConfig,
}

impl Plugin for MyProviderPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            id: "my-provider".to_string(),
            name: "My Provider".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Provider,
            description: Some("Custom LLM provider".to_string()),
            ..Default::default()
        }
    }

    fn on_load(&mut self, api: &mut dyn PluginApi) -> Result<()> {
        let provider = Box::new(MyProvider::new(self.config.clone()));
        api.register_provider(provider);
        api.log(LogLevel::Info, "My Provider loaded successfully");
        Ok(())
    }

    fn on_unload(&mut self) -> Result<()> {
        Ok(())
    }
}

// Export function for dynamic loading
#[no_mangle]
pub extern "C" fn create_plugin(config: &str) -> Box<dyn Plugin> {
    let config: MyProviderConfig = serde_json::from_str(config).unwrap();
    Box::new(MyProviderPlugin { config })
}
```

## Creating a Channel Plugin

### Example: Custom Messaging Channel

```rust
use openagent::core::{Channel, ChannelMessage, ChannelReply, ChannelCapabilities};
use openagent::plugin_sdk::{Plugin, PluginManifest, PluginApi, PluginKind};
use async_trait::async_trait;
use tokio::sync::mpsc;

pub struct MyChannel {
    config: MyChannelConfig,
    sender: Option<mpsc::Sender<ChannelMessage>>,
    running: bool,
}

#[async_trait]
impl Channel for MyChannel {
    fn name(&self) -> &str {
        "my-channel"
    }

    async fn start(&mut self) -> Result<()> {
        self.running = true;
        // Start listening for messages
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    async fn send(&self, reply: ChannelReply) -> Result<()> {
        // Send message to the external platform
        Ok(())
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            markdown: true,
            code_blocks: true,
            images: false,
            files: false,
            reactions: true,
            threads: false,
            max_message_length: Some(4096),
        }
    }

    async fn on_message(&self, message: ChannelMessage) -> Result<Option<ChannelReply>> {
        // Process incoming message
        Ok(None)
    }
}
```

## Plugin Registry

OpenAgent uses a `PluginRegistry` to manage plugins:

```rust
use openagent::plugin_sdk::PluginRegistry;

// Create registry
let mut registry = PluginRegistry::new();

// Discover plugins in a directory
let results = registry.discover("/path/to/plugins").await?;

for result in results {
    match result {
        PluginLoadResult::Ok { id, manifest } => {
            println!("Loaded plugin: {} v{}", manifest.name, manifest.version);
        }
        PluginLoadResult::Error { id, error } => {
            eprintln!("Failed to load {}: {}", id, error);
        }
    }
}

// Get a specific plugin
if let Some(plugin) = registry.get("my-plugin") {
    println!("Plugin capabilities: {:?}", plugin.manifest().capabilities);
}

// Unload a plugin
registry.unload("my-plugin")?;
```

## Plugin Configuration

Plugins can be configured in the main config file:

```toml
# config.toml

[plugins.my-provider]
api_key = "sk-..."
base_url = "https://api.myprovider.com"
model = "my-model-v1"

[plugins.my-channel]
token = "..."
server_url = "wss://my-chat-server.com"
```

Access in plugin:

```rust
fn on_load(&mut self, api: &mut dyn PluginApi) -> Result<()> {
    let config = api.config();
    
    if let Some(plugin_config) = config.plugins.get("my-provider") {
        let my_config: MyProviderConfig = serde_json::from_value(plugin_config.clone())?;
        // Use config...
    }
    
    Ok(())
}
```

## Building & Distributing

### Build Commands

```bash
# Build plugin as dynamic library
cargo build --release --lib

# The output will be in target/release/
# - libmy_plugin.so (Linux)
# - libmy_plugin.dylib (macOS)
# - my_plugin.dll (Windows)
```

### Distribution Structure

```
my-plugin-1.0.0/
├── plugin.json
├── libmy_plugin.so      # or .dylib / .dll
├── README.md
└── LICENSE
```

### Installation

Users can install plugins by placing them in the plugins directory:

```bash
# Default plugin directory
~/.config/openagent/plugins/

# Or specify in config
[plugins]
directory = "/path/to/plugins"
```

## Best Practices

### Error Handling

```rust
fn on_load(&mut self, api: &mut dyn PluginApi) -> Result<()> {
    // Validate configuration
    if self.config.api_key.is_empty() {
        return Err(Error::Config("API key is required".into()));
    }
    
    // Test connection
    match self.test_connection().await {
        Ok(_) => api.log(LogLevel::Info, "Connected successfully"),
        Err(e) => {
            api.log(LogLevel::Warn, &format!("Connection test failed: {}", e));
            // Continue anyway - might work later
        }
    }
    
    Ok(())
}
```

### Resource Cleanup

```rust
fn on_unload(&mut self) -> Result<()> {
    // Close connections
    if let Some(client) = self.client.take() {
        client.close().await?;
    }
    
    // Stop background tasks
    if let Some(handle) = self.task_handle.take() {
        handle.abort();
    }
    
    Ok(())
}
```

### Logging

```rust
fn on_load(&mut self, api: &mut dyn PluginApi) -> Result<()> {
    api.log(LogLevel::Debug, "Starting initialization...");
    api.log(LogLevel::Info, "Plugin loaded successfully");
    api.log(LogLevel::Warn, "Using deprecated feature X");
    api.log(LogLevel::Error, "Failed to connect to service");
    Ok(())
}
```

## Next Steps

- [Core Traits](./core-traits.md) - Implement standard traits
- [Configuration](./configuration.md) - Plugin configuration options
- [API Reference](./api-reference.md) - Complete SDK reference
