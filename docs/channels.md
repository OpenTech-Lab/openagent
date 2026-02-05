# Channels

OpenAgent supports multiple messaging channels for user interaction. This document describes the available channels and how to configure them.

## Overview

Channels are the interfaces through which users communicate with OpenAgent:

| Channel | Status | Features |
|---------|--------|----------|
| **Telegram** | ✅ Stable | Bot API, files, inline buttons |
| **Discord** | 🚧 Planned | Gateway API, threads, reactions |
| **Slack** | 🚧 Planned | Events API, channels, threads |
| **WebChat** | 🚧 Planned | Browser widget, WebSocket |
| **CLI** | ✅ Stable | Interactive terminal |

## Telegram

Telegram is the primary messaging channel for OpenAgent.

### Setup

1. Create a bot via [@BotFather](https://t.me/BotFather)
2. Get the bot token
3. Configure in OpenAgent:

```toml
[channels.telegram]
bot_token = "123456:ABC-DEF..."  # Or use TELEGRAM_BOT_TOKEN env var
use_long_polling = true
dm_policy = "open"
```

### Configuration Options

```toml
[channels.telegram]
# Bot token (required)
# Use TELEGRAM_BOT_TOKEN env var for security
bot_token = "..."

# Use long polling (recommended for development)
use_long_polling = true

# Webhook URL (for production)
# webhook_url = "https://your-domain.com/webhook/telegram"
# webhook_secret = "random-secret-string"

# DM policy
# - "open": Allow all DMs
# - "require_pairing": Require pairing code
# - "allow_list": Only allow listed users
dm_policy = "open"

# Allowed user IDs (for allow_list mode)
allow_from = [123456789, 987654321]

# Group configurations
[channels.telegram.groups.my-group]
chat_id = -1001234567890
name = "My Group"
respond_to_mentions = true
respond_to_replies = true
```

### Commands

Built-in Telegram commands:

| Command | Description |
|---------|-------------|
| `/start` | Initialize conversation |
| `/help` | Show available commands |
| `/clear` | Clear conversation history |
| `/model` | Show current model |
| `/switch <model>` | Switch to a different model |
| `/run <lang> <code>` | Execute code |
| `/status` | Show bot status |
| `/soul` | View/edit agent personality |

### Features

- **Markdown support** - Rich text formatting
- **Code blocks** - Syntax highlighting
- **File handling** - Upload/download files
- **Inline buttons** - Interactive responses
- **Reply threading** - Context-aware replies

### Example Usage

```rust
use openagent::config::TelegramConfig;
use teloxide::prelude::*;

// Configuration is loaded from config file or environment
let config = Config::from_env()?;

if let Some(telegram) = &config.channels.telegram {
    let bot = Bot::new(telegram.bot_token.expose_secret());
    
    // Check user authorization
    if !telegram.allow_from.is_empty() {
        // Only allow specific users
    }
}
```

## Discord (Planned)

Discord integration is planned for a future release.

### Configuration

```toml
[channels.discord]
# Bot token
bot_token = "..."

# Application ID
application_id = "..."

# Guild ID (for development)
guild_id = "..."

# Features
respond_to_mentions = true
respond_to_dms = true
use_threads = true
```

### Planned Features

- Slash commands (`/ask`, `/run`, `/soul`)
- Thread-based conversations
- Reaction-based feedback
- Voice channel integration (future)

## Slack (Planned)

Slack integration is planned for a future release.

### Configuration

```toml
[channels.slack]
# Bot token
bot_token = "xoxb-..."

# App token (for Socket Mode)
app_token = "xapp-..."

# Signing secret
signing_secret = "..."

# Features
respond_to_mentions = true
respond_to_dms = true
use_threads = true
```

### Planned Features

- Events API support
- Socket Mode for development
- Thread-based conversations
- Slack Connect support

## WebChat (Planned)

Browser-based chat widget for embedding in websites.

### Configuration

```toml
[channels.webchat]
# Enable WebChat
enabled = true

# WebSocket endpoint
ws_endpoint = "ws://localhost:18789/chat"

# CORS origins
allowed_origins = ["https://your-site.com"]

# Theme
theme = "light"

# Position
position = "bottom-right"
```

### Planned Features

- Embeddable widget (`<script>` tag)
- Customizable theming
- Mobile-responsive
- Markdown rendering
- Code highlighting

## CLI Channel

The CLI provides an interactive terminal interface.

### Usage

```bash
# Start interactive chat
pnpm openagent chat

# With model selection
pnpm openagent chat --model anthropic/claude-3.5-sonnet
```

### Commands

| Command | Description |
|---------|-------------|
| `/quit` | Exit chat |
| `/clear` | Clear history |
| `/model` | Change model |
| `/soul` | Edit personality |
| `/help` | Show commands |

## Channel Trait

All channels implement the `Channel` trait:

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    /// Channel identifier
    fn name(&self) -> &str;

    /// Start receiving messages
    async fn start(&mut self) -> Result<()>;

    /// Stop receiving messages
    async fn stop(&mut self) -> Result<()>;

    /// Send a reply
    async fn send(&self, reply: ChannelReply) -> Result<()>;

    /// Get capabilities
    fn capabilities(&self) -> ChannelCapabilities;
}
```

### Capabilities

```rust
pub struct ChannelCapabilities {
    /// Supports Markdown
    pub markdown: bool,
    /// Supports code blocks
    pub code_blocks: bool,
    /// Supports images
    pub images: bool,
    /// Supports file uploads
    pub files: bool,
    /// Supports reactions
    pub reactions: bool,
    /// Supports threads
    pub threads: bool,
    /// Maximum message length
    pub max_message_length: Option<usize>,
}
```

## Message Types

### Incoming Message

```rust
pub struct ChannelMessage {
    /// Unique message ID
    pub id: String,
    /// Channel ID (e.g., "telegram")
    pub channel_id: String,
    /// User ID
    pub user_id: String,
    /// Message content
    pub content: String,
    /// Attachments
    pub attachments: Vec<Attachment>,
    /// Reply-to message ID
    pub reply_to: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}
```

### Outgoing Reply

```rust
pub struct ChannelReply {
    /// Channel ID
    pub channel_id: String,
    /// Reply content
    pub content: String,
    /// Reply to specific message
    pub reply_to: Option<String>,
    /// Attachments to send
    pub attachments: Vec<Attachment>,
    /// Parse mode (Markdown, HTML)
    pub parse_mode: Option<ParseMode>,
}
```

## Creating Custom Channels

See the [Plugin SDK](./plugin-sdk.md) for creating custom channel integrations.

```rust
use openagent::core::{Channel, ChannelMessage, ChannelReply};
use openagent::plugin_sdk::{Plugin, PluginApi};

pub struct MyChannel {
    config: MyChannelConfig,
}

#[async_trait]
impl Channel for MyChannel {
    fn name(&self) -> &str {
        "my-channel"
    }

    async fn start(&mut self) -> Result<()> {
        // Connect to your messaging platform
        Ok(())
    }

    async fn send(&self, reply: ChannelReply) -> Result<()> {
        // Send message to platform
        Ok(())
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            markdown: true,
            code_blocks: true,
            images: true,
            files: false,
            reactions: false,
            threads: false,
            max_message_length: Some(4096),
        }
    }
}
```

## Next Steps

- [Configuration](./configuration.md) - Channel configuration details
- [Plugin SDK](./plugin-sdk.md) - Creating custom channels
- [Gateway Protocol](./gateway-protocol.md) - WebSocket integration
