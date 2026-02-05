//! Channel trait - Abstract interface for messaging platforms
//!
//! This module defines the `Channel` trait that allows OpenAgent to work with
//! any messaging platform (Telegram, Discord, Slack, WhatsApp, etc.)
//!
//! Following openclaw's multi-channel architecture, each channel:
//! - Implements the Channel trait
//! - Can be enabled/disabled independently
//! - Has its own configuration section
//! - Supports platform-specific features via capabilities

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Result;

/// Unique identifier for a channel type
pub type ChannelId = String;

/// Unique identifier for a message within a channel
pub type MessageId = String;

/// Unique identifier for a user/sender
pub type SenderId = String;

/// Unique identifier for a conversation/chat
pub type ConversationId = String;

/// Metadata about a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMeta {
    /// Unique channel identifier
    pub id: ChannelId,
    /// Human-readable label
    pub label: String,
    /// Longer description
    pub description: String,
    /// Documentation path
    pub docs_path: Option<String>,
}

/// Capabilities supported by a channel
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    /// Can send text messages
    pub text: bool,
    /// Can send images
    pub images: bool,
    /// Can send audio
    pub audio: bool,
    /// Can send video
    pub video: bool,
    /// Can send files/documents
    pub files: bool,
    /// Can send reactions
    pub reactions: bool,
    /// Can edit sent messages
    pub editing: bool,
    /// Can delete messages
    pub deletion: bool,
    /// Can handle group chats
    pub groups: bool,
    /// Can send typing indicators
    pub typing_indicators: bool,
    /// Can receive read receipts
    pub read_receipts: bool,
    /// Can handle inline queries (e.g., Telegram)
    pub inline_queries: bool,
    /// Can handle webhooks
    pub webhooks: bool,
    /// Maximum message length
    pub max_message_length: Option<usize>,
}

/// An incoming message from a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    /// Unique message ID
    pub id: MessageId,
    /// Channel this message came from
    pub channel_id: ChannelId,
    /// Conversation/chat ID
    pub conversation_id: ConversationId,
    /// Sender ID
    pub sender_id: SenderId,
    /// Sender display name
    pub sender_name: Option<String>,
    /// Message content
    pub content: MessageContent,
    /// When the message was sent
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Reply-to message ID (if this is a reply)
    pub reply_to: Option<MessageId>,
    /// Whether this is from a group
    pub is_group: bool,
    /// Raw platform-specific data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    /// Plain text message
    Text { text: String },
    /// Image with optional caption
    Image {
        url: String,
        caption: Option<String>,
        mime_type: Option<String>,
    },
    /// Audio message
    Audio {
        url: String,
        duration_secs: Option<u32>,
        mime_type: Option<String>,
    },
    /// Video message
    Video {
        url: String,
        duration_secs: Option<u32>,
        caption: Option<String>,
        mime_type: Option<String>,
    },
    /// File/document
    File {
        url: String,
        filename: String,
        mime_type: Option<String>,
        size_bytes: Option<u64>,
    },
    /// Location
    Location { latitude: f64, longitude: f64 },
    /// Multiple content items (for platforms that support it)
    Mixed { parts: Vec<MessageContent> },
}

impl MessageContent {
    /// Create a text message content
    pub fn text(text: impl Into<String>) -> Self {
        MessageContent::Text { text: text.into() }
    }

    /// Get the text content if this is a text message
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// Outgoing reply to send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelReply {
    /// Conversation to reply to
    pub conversation_id: ConversationId,
    /// Content to send
    pub content: MessageContent,
    /// Message ID to reply to (for threading)
    pub reply_to: Option<MessageId>,
    /// Parse mode for rich text (markdown, html, etc.)
    pub parse_mode: Option<String>,
}

impl ChannelReply {
    /// Create a simple text reply
    pub fn text(conversation_id: impl Into<String>, text: impl Into<String>) -> Self {
        ChannelReply {
            conversation_id: conversation_id.into(),
            content: MessageContent::text(text),
            reply_to: None,
            parse_mode: None,
        }
    }

    /// Set the message to reply to
    pub fn with_reply_to(mut self, message_id: impl Into<String>) -> Self {
        self.reply_to = Some(message_id.into());
        self
    }

    /// Set parse mode
    pub fn with_parse_mode(mut self, mode: impl Into<String>) -> Self {
        self.parse_mode = Some(mode.into());
        self
    }
}

/// Status of a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    /// Whether the channel is configured
    pub configured: bool,
    /// Whether the channel is currently running
    pub running: bool,
    /// Last start time
    pub last_start_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Last stop time
    pub last_stop_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Last error message
    pub last_error: Option<String>,
}

/// Handler for incoming messages
pub type MessageHandler = Arc<dyn Fn(ChannelMessage) -> futures::future::BoxFuture<'static, Result<()>> + Send + Sync>;

/// Abstract interface for messaging channels
///
/// Implement this trait to add support for new messaging platforms.
/// The channel handles connection, message sending/receiving, and platform-specific features.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Get channel metadata
    fn meta(&self) -> &ChannelMeta;

    /// Get the channel ID
    fn id(&self) -> &ChannelId {
        &self.meta().id
    }

    /// Get channel capabilities
    fn capabilities(&self) -> &ChannelCapabilities;

    /// Get current channel status
    async fn status(&self) -> Result<ChannelStatus>;

    /// Start the channel (begin receiving messages)
    async fn start(&self, handler: MessageHandler) -> Result<()>;

    /// Stop the channel
    async fn stop(&self) -> Result<()>;

    /// Send a reply
    async fn send(&self, reply: ChannelReply) -> Result<MessageId>;

    /// Send a typing indicator
    async fn send_typing(&self, conversation_id: &str) -> Result<()> {
        // Default: no-op for channels that don't support typing
        let _ = conversation_id;
        Ok(())
    }

    /// Edit a previously sent message
    async fn edit(&self, message_id: &MessageId, content: MessageContent) -> Result<()> {
        let _ = (message_id, content);
        Err(crate::error::Error::NotSupported(
            "Message editing not supported".to_string(),
        ))
    }

    /// Delete a message
    async fn delete(&self, message_id: &MessageId) -> Result<()> {
        let _ = message_id;
        Err(crate::error::Error::NotSupported(
            "Message deletion not supported".to_string(),
        ))
    }

    /// React to a message
    async fn react(&self, message_id: &MessageId, reaction: &str) -> Result<()> {
        let _ = (message_id, reaction);
        Err(crate::error::Error::NotSupported(
            "Reactions not supported".to_string(),
        ))
    }

    /// Health check
    async fn health_check(&self) -> Result<bool> {
        self.status().await.map(|s| s.running)
    }
}

/// Plugin interface for channels
///
/// This allows channels to be loaded dynamically and registered at runtime.
pub trait ChannelPlugin: Send + Sync {
    /// Get the plugin ID
    fn id(&self) -> &str;

    /// Get the plugin name
    fn name(&self) -> &str;

    /// Get the plugin description
    fn description(&self) -> &str;

    /// Create a channel instance from configuration
    fn create(&self, config: &HashMap<String, serde_json::Value>) -> Result<Box<dyn Channel>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_content_text() {
        let content = MessageContent::text("Hello, world!");
        assert_eq!(content.as_text(), Some("Hello, world!"));
    }

    #[test]
    fn test_channel_reply_builder() {
        let reply = ChannelReply::text("conv123", "Hello!")
            .with_reply_to("msg456")
            .with_parse_mode("markdown");

        assert_eq!(reply.conversation_id, "conv123");
        assert_eq!(reply.reply_to, Some("msg456".to_string()));
        assert_eq!(reply.parse_mode, Some("markdown".to_string()));
    }
}
