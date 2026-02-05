//! Gateway protocol types
//!
//! Request/response types for gateway methods.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Authentication
// ============================================================================

/// Authentication request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRequest {
    /// Authentication method
    pub method: AuthMethod,
    /// Token (for token auth)
    pub token: Option<String>,
    /// Password (for password auth)
    pub password: Option<String>,
}

/// Authentication method
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    /// No authentication
    None,
    /// Token-based auth
    Token,
    /// Password-based auth
    Password,
}

/// Authentication response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    /// Whether auth was successful
    pub success: bool,
    /// Session ID (if successful)
    pub session_id: Option<String>,
    /// User/client ID
    pub client_id: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

// ============================================================================
// Sessions
// ============================================================================

/// Session info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// Session ID
    pub id: String,
    /// Channel ID
    pub channel_id: Option<String>,
    /// Conversation ID
    pub conversation_id: Option<String>,
    /// User ID
    pub user_id: Option<String>,
    /// Current model
    pub model: String,
    /// Created at
    pub created_at: i64,
    /// Last activity
    pub last_activity_at: i64,
    /// Message count
    pub message_count: u32,
    /// Session metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// List sessions request
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsListRequest {
    /// Filter by channel
    pub channel_id: Option<String>,
    /// Filter by user
    pub user_id: Option<String>,
    /// Maximum results
    pub limit: Option<u32>,
}

/// List sessions response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsListResponse {
    /// Sessions
    pub sessions: Vec<SessionInfo>,
}

// ============================================================================
// Agent Methods
// ============================================================================

/// Send message to agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSendRequest {
    /// Session ID (optional, creates new if not provided)
    pub session_id: Option<String>,
    /// Message content
    pub message: String,
    /// Enable streaming
    #[serde(default)]
    pub stream: bool,
    /// Model override
    pub model: Option<String>,
    /// Thinking level
    pub thinking_level: Option<String>,
}

/// Agent response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResponse {
    /// Session ID
    pub session_id: String,
    /// Response content
    pub content: String,
    /// Model used
    pub model: String,
    /// Finish reason
    pub finish_reason: Option<String>,
    /// Usage statistics
    pub usage: Option<UsageStats>,
}

/// Usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStats {
    /// Prompt tokens
    pub prompt_tokens: u32,
    /// Completion tokens
    pub completion_tokens: u32,
    /// Total tokens
    pub total_tokens: u32,
    /// Cost (if available)
    pub cost: Option<f64>,
}

// ============================================================================
// Channel Methods
// ============================================================================

/// List channels request
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelsListRequest {}

/// Channel status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    /// Channel ID
    pub id: String,
    /// Channel label
    pub label: String,
    /// Whether configured
    pub configured: bool,
    /// Whether running
    pub running: bool,
    /// Last error
    pub last_error: Option<String>,
}

/// List channels response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelsListResponse {
    /// Channels
    pub channels: Vec<ChannelStatus>,
}

// ============================================================================
// Events
// ============================================================================

/// Event names
pub mod events {
    /// Message received
    pub const MESSAGE_RECEIVED: &str = "message.received";
    /// Message sent
    pub const MESSAGE_SENT: &str = "message.sent";
    /// Streaming chunk
    pub const STREAM_CHUNK: &str = "stream.chunk";
    /// Streaming done
    pub const STREAM_DONE: &str = "stream.done";
    /// Session created
    pub const SESSION_CREATED: &str = "session.created";
    /// Session updated
    pub const SESSION_UPDATED: &str = "session.updated";
    /// Session deleted
    pub const SESSION_DELETED: &str = "session.deleted";
    /// Channel status changed
    pub const CHANNEL_STATUS: &str = "channel.status";
    /// Error occurred
    pub const ERROR: &str = "error";
    /// Heartbeat
    pub const HEARTBEAT: &str = "heartbeat";
}

/// Streaming chunk event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunkEvent {
    /// Session ID
    pub session_id: String,
    /// Chunk index
    pub index: u32,
    /// Content delta
    pub delta: String,
    /// Whether this is the final chunk
    pub is_final: bool,
}

/// Message event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEvent {
    /// Session ID
    pub session_id: String,
    /// Message ID
    pub message_id: String,
    /// Channel ID
    pub channel_id: Option<String>,
    /// Role
    pub role: String,
    /// Content
    pub content: String,
    /// Timestamp
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_send_request() {
        let req = AgentSendRequest {
            session_id: None,
            message: "Hello".to_string(),
            stream: true,
            model: None,
            thinking_level: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Hello"));
    }
}
