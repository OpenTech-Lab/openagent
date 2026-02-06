//! MCP (Model Context Protocol) module
//!
//! Provides integration with MCP servers, allowing the agent to connect
//! to external tool providers that implement the Model Context Protocol.
//!
//! ## Architecture
//!
//! - **client**: MCP client for connecting to MCP servers
//! - **protocol**: Wire protocol types (JSON-RPC based)
//! - **bridge**: Adapts MCP tools into OpenAgent's `Tool` trait
//!
//! ## Usage
//!
//! ```rust,no_run
//! use openagent::mcp::McpClient;
//!
//! # async fn example() -> openagent::Result<()> {
//! // Connect to a local MCP server
//! let client = McpClient::connect_stdio("my-mcp-server").await?;
//!
//! // List available tools
//! let tools = client.list_tools().await?;
//!
//! // Call a tool
//! let result = client.call_tool("tool_name", serde_json::json!({"arg": "value"})).await?;
//! # Ok(())
//! # }
//! ```

mod client;
mod protocol;
mod bridge;

pub use client::McpClient;
pub use protocol::{McpRequest, McpResponse, McpTool, McpToolInput};
pub use bridge::McpToolBridge;
