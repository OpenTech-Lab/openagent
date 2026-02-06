//! MCP wire protocol types
//!
//! Based on the Model Context Protocol specification (JSON-RPC 2.0).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request to an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpRequest {
    /// Create a new MCP request
    pub fn new(id: u64, method: impl Into<String>, params: Option<Value>) -> Self {
        McpRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }

    /// Create an initialize request
    pub fn initialize(id: u64) -> Self {
        Self::new(id, "initialize", Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "openagent",
                "version": env!("CARGO_PKG_VERSION")
            }
        })))
    }

    /// Create a tools/list request
    pub fn list_tools(id: u64) -> Self {
        Self::new(id, "tools/list", None)
    }

    /// Create a tools/call request
    pub fn call_tool(id: u64, name: impl Into<String>, arguments: Value) -> Self {
        Self::new(id, "tools/call", Some(serde_json::json!({
            "name": name.into(),
            "arguments": arguments
        })))
    }
}

/// JSON-RPC response from an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

/// MCP error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Tool definition from an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: String,
    /// Input schema (JSON Schema)
    #[serde(rename = "inputSchema")]
    pub input_schema: McpToolInput,
}

/// Tool input schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInput {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(default)]
    pub properties: Value,
    #[serde(default)]
    pub required: Vec<String>,
}

/// Content block returned by a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(rename = "mimeType", default)]
    pub mime_type: Option<String>,
}

/// Result of a tools/call response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}
