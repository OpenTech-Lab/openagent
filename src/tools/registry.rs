//! Tool registry - manages available tools for the agent

use std::collections::HashMap;

use crate::agent::types::ToolDefinition;
use crate::error::Result;

use super::traits::{Tool, ToolCall, ToolResult};

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        ToolRegistry {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Get all tool definitions
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.to_definition()).collect()
    }

    /// Execute a tool call
    pub async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        match self.get(&call.name) {
            Some(tool) => tool.execute(call.arguments.clone()).await,
            None => Ok(ToolResult::failure(format!(
                "Unknown tool: {}",
                call.name
            ))),
        }
    }

    /// Get tool count
    pub fn count(&self) -> usize {
        self.tools.len()
    }

    /// List tool names
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::traits::ToolResult;

    #[test]
    fn test_tool_result() {
        let success = ToolResult::success("Done!");
        assert!(success.success);
        assert_eq!(success.content.as_deref(), Some("Done!"));

        let failure = ToolResult::failure("Oops!");
        assert!(!failure.success);
        assert_eq!(failure.error.as_deref(), Some("Oops!"));
    }
}
