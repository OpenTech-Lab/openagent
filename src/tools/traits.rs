//! Core tool trait and result types

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::types::{ToolDefinition, FunctionDefinition};
use crate::error::Result;

/// A tool that can be called by the LLM
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Get the JSON Schema for tool parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with given arguments
    async fn execute(&self, args: Value) -> Result<ToolResult>;

    /// Convert to OpenRouter tool definition
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: self.name().to_string(),
                description: self.description().to_string(),
                parameters: self.parameters_schema(),
            },
        }
    }
}

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the execution was successful
    pub success: bool,
    /// Result content (for successful execution)
    pub content: Option<String>,
    /// Error message (for failed execution)
    pub error: Option<String>,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl ToolResult {
    /// Create a successful result
    pub fn success(content: impl Into<String>) -> Self {
        ToolResult {
            success: true,
            content: Some(content.into()),
            error: None,
            metadata: None,
        }
    }

    /// Create a successful result with metadata
    pub fn success_with_metadata(content: impl Into<String>, metadata: Value) -> Self {
        ToolResult {
            success: true,
            content: Some(content.into()),
            error: None,
            metadata: Some(metadata),
        }
    }

    /// Create a failed result
    pub fn failure(error: impl Into<String>) -> Self {
        ToolResult {
            success: false,
            content: None,
            error: Some(error.into()),
            metadata: None,
        }
    }

    /// Convert to a string for the LLM
    pub fn to_string(&self) -> String {
        if self.success {
            self.content.clone().unwrap_or_default()
        } else {
            format!("Error: {}", self.error.clone().unwrap_or_default())
        }
    }
}

/// A tool call request from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool arguments as JSON
    pub arguments: Value,
}

impl ToolCall {
    /// Parse arguments into a specific type
    pub fn parse_arguments<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        serde_json::from_value(self.arguments.clone())
            .map_err(|e| crate::Error::InvalidInput(format!("Invalid tool arguments: {}", e)))
    }
}
