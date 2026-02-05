# Configuration System

OpenAgent uses a modular configuration system that splits settings into focused, domain-specific modules. This document describes how configuration works.

## Overview

Configuration is organized into these categories:

| Module | File | Purpose |
|--------|------|---------|
| Provider | `config/types/provider.rs` | LLM provider settings |
| Channel | `config/types/channel.rs` | Messaging channel settings |
| Storage | `config/types/storage.rs` | Database configuration |
| Sandbox | `config/types/sandbox.rs` | Code execution settings |

## Configuration Loading

Configuration is loaded in this priority order:

1. **Default values** - Sensible defaults for all settings
2. **Config file** - `~/.config/openagent/config.toml` or `config.json`
3. **Environment variables** - Override specific settings

```rust
use openagent::config::{Config, load_config};

// Load with priority: defaults < file < env
let config = Config::from_env()?;

// Or load from a specific path
let config = load_config()?;
```

## Config Structure

### Main Config

```rust
pub struct Config {
    /// Agent-level settings
    pub agent: AgentConfig,
    /// LLM provider configuration
    pub provider: ProviderConfig,
    /// Messaging channel configuration
    pub channels: ChannelsConfig,
    /// Storage backend configuration
    pub storage: StorageConfig,
    /// Code execution configuration
    pub sandbox: SandboxConfig,
    /// Gateway server configuration
    pub gateway: GatewayConfig,
    /// Plugin configuration
    pub plugins: HashMap<String, serde_json::Value>,
}
```

### Agent Config

```rust
pub struct AgentConfig {
    /// Default model to use
    pub model: String,
    /// Agent workspace directory
    pub workspace: PathBuf,
    /// System prompt file (SOUL.md)
    pub system_prompt_file: Option<PathBuf>,
    /// Maximum context tokens
    pub max_context_tokens: u32,
    /// Default thinking level
    pub thinking_level: ThinkingLevel,
    /// Enable verbose output
    pub verbose: bool,
}
```

### Provider Config

```rust
pub struct ProviderConfig {
    /// Default provider
    pub default: String,
    /// OpenRouter configuration
    pub openrouter: Option<OpenRouterConfig>,
    /// Anthropic configuration
    pub anthropic: Option<AnthropicConfig>,
    /// OpenAI configuration
    pub openai: Option<OpenAIConfig>,
    /// Custom providers
    pub custom: HashMap<String, CustomProviderConfig>,
}

pub struct OpenRouterConfig {
    /// API key (from OPENROUTER_API_KEY)
    pub api_key: SecretString,
    /// Default model
    pub default_model: String,
    /// Base URL
    pub base_url: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum retries
    pub max_retries: u32,
}
```

### Channel Config

```rust
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
    pub webchat: WebChatConfig,
}

pub struct TelegramConfig {
    /// Bot token (from TELEGRAM_BOT_TOKEN)
    pub bot_token: SecretString,
    /// Allowed user IDs (empty = allow all)
    pub allow_from: Vec<i64>,
    /// DM policy
    pub dm_policy: DmPolicy,
    /// Use long polling
    pub use_long_polling: bool,
    /// Webhook URL (if not polling)
    pub webhook_url: Option<String>,
}

pub enum DmPolicy {
    /// Allow all DMs
    Open,
    /// Require pairing code
    RequirePairing,
    /// Only allow listed users
    AllowList,
}
```

### Storage Config

```rust
pub struct StorageConfig {
    /// Primary storage backend
    pub backend: StorageBackendType,
    /// PostgreSQL configuration
    pub postgres: Option<PostgresConfig>,
    /// OpenSearch configuration
    pub opensearch: Option<OpenSearchConfig>,
    /// SQLite configuration
    pub sqlite: SqliteConfig,
}

pub struct PostgresConfig {
    /// Database URL (from DATABASE_URL)
    pub url: SecretString,
    /// Maximum connections
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connect_timeout_secs: u64,
    /// Enable pgvector extension
    pub enable_pgvector: bool,
}

pub struct OpenSearchConfig {
    /// OpenSearch URL
    pub url: String,
    /// Username
    pub username: Option<String>,
    /// Password
    pub password: Option<SecretString>,
    /// Index prefix
    pub index_prefix: String,
}
```

### Sandbox Config

```rust
pub struct SandboxConfig {
    /// Execution environment
    pub execution_env: ExecutionEnv,
    /// Allowed directory for OS sandbox
    pub allowed_dir: PathBuf,
    /// Default timeout
    pub default_timeout: Duration,
    /// Maximum timeout
    pub max_timeout: Duration,
    /// Container configuration
    pub container: ContainerConfig,
    /// Wasm configuration
    pub wasm: WasmConfig,
}

pub enum ExecutionEnv {
    /// OS-level sandbox (directory restricted)
    Os,
    /// WebAssembly sandbox (Wasmtime)
    Wasm,
    /// Docker container
    Container,
}
```

## Environment Variables

Common environment variables:

```bash
# Provider
OPENROUTER_API_KEY=sk-or-...
DEFAULT_MODEL=anthropic/claude-sonnet-4

# Channels
TELEGRAM_BOT_TOKEN=123456:ABC...
TELEGRAM_ALLOWED_USERS=123456789,987654321

# Storage
DATABASE_URL=postgres://user:pass@localhost:5432/openagent
OPENSEARCH_URL=http://localhost:9200
OPENSEARCH_USERNAME=admin
OPENSEARCH_PASSWORD=admin

# Sandbox
EXECUTION_ENV=os
ALLOWED_DIR=/tmp/openagent-workspace

# Gateway
GATEWAY_PORT=18789
GATEWAY_BIND=127.0.0.1
```

## Config File Example

### TOML Format

```toml
# ~/.config/openagent/config.toml

[agent]
model = "anthropic/claude-sonnet-4"
workspace = "~/.openagent/workspace"
max_context_tokens = 200000
thinking_level = "medium"

[provider]
default = "openrouter"

[provider.openrouter]
default_model = "anthropic/claude-sonnet-4"
timeout_secs = 120
max_retries = 3

[channels.telegram]
use_long_polling = true
dm_policy = "open"

[storage]
backend = "postgres"

[storage.postgres]
max_connections = 5
connect_timeout_secs = 30
enable_pgvector = true

[storage.opensearch]
url = "http://localhost:9200"
index_prefix = "openagent"

[sandbox]
execution_env = "os"
allowed_dir = "/tmp/openagent-workspace"
default_timeout = "30s"
max_timeout = "5m"

[gateway]
port = 18789
bind = "127.0.0.1"
websocket = true
```

### JSON Format

```json
{
  "agent": {
    "model": "anthropic/claude-sonnet-4",
    "workspace": "~/.openagent/workspace",
    "max_context_tokens": 200000,
    "thinking_level": "medium"
  },
  "provider": {
    "default": "openrouter",
    "openrouter": {
      "default_model": "anthropic/claude-sonnet-4",
      "timeout_secs": 120,
      "max_retries": 3
    }
  },
  "channels": {
    "telegram": {
      "use_long_polling": true,
      "dm_policy": "open"
    }
  },
  "storage": {
    "backend": "postgres",
    "postgres": {
      "max_connections": 5,
      "connect_timeout_secs": 30,
      "enable_pgvector": true
    }
  },
  "sandbox": {
    "execution_env": "os",
    "allowed_dir": "/tmp/openagent-workspace"
  }
}
```

## Validation

Configuration is validated on load:

```rust
use openagent::config::{validate_config, ConfigValidationResult};

let config = Config::from_env()?;
let result = validate_config(&config);

if !result.valid {
    for error in &result.errors {
        eprintln!("Error: {} - {}", error.field, error.message);
    }
}

for warning in &result.warnings {
    eprintln!("Warning: {} - {}", warning.field, warning.message);
}
```

### Validation Rules

| Field | Rule |
|-------|------|
| `provider.openrouter.api_key` | Required if using OpenRouter |
| `channels.telegram.bot_token` | Required if Telegram enabled |
| `storage.postgres.url` | Valid PostgreSQL connection string |
| `sandbox.allowed_dir` | Must exist and be writable |
| `gateway.port` | Must be in range 1-65535 |

## Directory Paths

Standard paths used by OpenAgent:

```rust
use openagent::config::paths;

// Configuration directory
let config_dir = paths::config_dir();
// ~/.config/openagent/

// State directory (databases, caches)
let state_dir = paths::state_dir();
// ~/.local/state/openagent/

// Workspace directory
let workspace_dir = paths::workspace_dir();
// ~/.openagent/workspace/

// Config file path
let config_path = paths::config_path();
// ~/.config/openagent/config.toml
```

## Next Steps

- [Gateway Protocol](./gateway-protocol.md) - WebSocket configuration
- [Plugin SDK](./plugin-sdk.md) - Plugin configuration
- [Architecture](./architecture.md) - System overview
