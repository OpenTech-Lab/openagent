//! MCP-to-Tool bridge
//!
//! Adapts MCP server tools into OpenAgent's `Tool` trait so they can
//! be registered in the `ToolRegistry` alongside built-in tools.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use super::client::McpClient;
use super::protocol::McpTool;
use crate::error::Result;
use crate::tools::{Tool, ToolResult};

/// Bridge that wraps an MCP tool as an OpenAgent Tool
pub struct McpToolBridge {
    /// Reference to the MCP client
    client: Arc<McpClient>,
    /// The MCP tool definition
    tool: McpTool,
}

impl McpToolBridge {
    /// Create a new bridge for a specific MCP tool
    pub fn new(client: Arc<McpClient>, tool: McpTool) -> Self {
        McpToolBridge { client, tool }
    }

    /// Create bridges for all tools from an MCP server
    pub async fn from_server(client: Arc<McpClient>) -> Result<Vec<Self>> {
        let tools = client.list_tools().await?;
        Ok(tools
            .into_iter()
            .map(|tool| McpToolBridge::new(Arc::clone(&client), tool))
            .collect())
    }
}

#[async_trait]
impl Tool for McpToolBridge {
    fn name(&self) -> &str {
        &self.tool.name
    }

    fn description(&self) -> &str {
        &self.tool.description
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": self.tool.input_schema.schema_type,
            "properties": self.tool.input_schema.properties,
            "required": self.tool.input_schema.required,
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        match self.client.call_tool(&self.tool.name, args).await {
            Ok(result) => {
                // Combine all text content blocks
                let text: String = result.content
                    .iter()
                    .filter_map(|c| c.text.as_deref())
                    .collect::<Vec<_>>()
                    .join("\n");

                if result.is_error {
                    Ok(ToolResult::failure(text))
                } else {
                    Ok(ToolResult::success(text))
                }
            }
            Err(e) => Ok(ToolResult::failure(format!(
                "MCP tool '{}' failed: {}",
                self.tool.name, e
            ))),
        }
    }
}
