//! Gateway protocol schema
//!
//! Defines the wire format for gateway messages.

use serde::{Deserialize, Serialize};

/// Protocol version
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Get the protocol version
pub fn protocol_version() -> &'static str {
    PROTOCOL_VERSION
}

/// Protocol version struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolVersion {
    /// Major version
    pub major: u32,
    /// Minor version
    pub minor: u32,
    /// Patch version
    pub patch: u32,
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        ProtocolVersion {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }
}

/// Gateway frame - Top-level message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GatewayFrame {
    /// Request from client
    Request(RequestFrame),
    /// Response from server
    Response(ResponseFrame),
    /// Event pushed by server
    Event(EventFrame),
    /// Error
    Error(ErrorFrame),
    /// Ping
    Ping { id: String },
    /// Pong
    Pong { id: String },
}

/// Request frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestFrame {
    /// Unique request ID
    pub id: String,
    /// Method name
    pub method: String,
    /// Parameters
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Response frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseFrame {
    /// Request ID this responds to
    pub id: String,
    /// Result (success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error (failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolError>,
}

/// Event frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventFrame {
    /// Event name
    pub event: String,
    /// Event data
    pub data: serde_json::Value,
    /// Optional session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

/// Error frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorFrame {
    /// Request ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Error details
    pub error: ProtocolError,
}

/// Protocol error
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Standard error codes
pub mod error_codes {
    /// Parse error
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid request
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Authentication required
    pub const AUTH_REQUIRED: i32 = -32000;
    /// Authentication failed
    pub const AUTH_FAILED: i32 = -32001;
    /// Rate limited
    pub const RATE_LIMITED: i32 = -32002;
    /// Session not found
    pub const SESSION_NOT_FOUND: i32 = -32003;
    /// Channel not available
    pub const CHANNEL_NOT_AVAILABLE: i32 = -32004;
}

impl ProtocolError {
    /// Create a new protocol error
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        ProtocolError {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Add data to the error
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Create a parse error
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(error_codes::PARSE_ERROR, message)
    }

    /// Create an invalid request error
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(error_codes::INVALID_REQUEST, message)
    }

    /// Create a method not found error
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            error_codes::METHOD_NOT_FOUND,
            format!("Method not found: {}", method),
        )
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(error_codes::INTERNAL_ERROR, message)
    }

    /// Create an auth required error
    pub fn auth_required() -> Self {
        Self::new(error_codes::AUTH_REQUIRED, "Authentication required")
    }

    /// Create an auth failed error
    pub fn auth_failed(message: impl Into<String>) -> Self {
        Self::new(error_codes::AUTH_FAILED, message)
    }
}

impl ResponseFrame {
    /// Create a success response
    pub fn success(id: impl Into<String>, result: serde_json::Value) -> Self {
        ResponseFrame {
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: impl Into<String>, error: ProtocolError) -> Self {
        ResponseFrame {
            id: id.into(),
            result: None,
            error: Some(error),
        }
    }
}

impl EventFrame {
    /// Create a new event
    pub fn new(event: impl Into<String>, data: serde_json::Value) -> Self {
        EventFrame {
            event: event.into(),
            data,
            session_id: None,
            timestamp: Some(chrono::Utc::now().timestamp()),
        }
    }

    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_frame_serialization() {
        let frame = GatewayFrame::Request(RequestFrame {
            id: "1".to_string(),
            method: "agent.send".to_string(),
            params: serde_json::json!({"message": "hello"}),
        });

        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("request"));
        assert!(json.contains("agent.send"));
    }

    #[test]
    fn test_response_frame() {
        let success = ResponseFrame::success("1", serde_json::json!({"ok": true}));
        assert!(success.result.is_some());
        assert!(success.error.is_none());

        let error = ResponseFrame::error("2", ProtocolError::internal("test error"));
        assert!(error.result.is_none());
        assert!(error.error.is_some());
    }
}
