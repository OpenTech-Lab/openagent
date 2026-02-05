//! Tool/function calling support

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::error::Result;
use crate::agent::types::{ToolDefinition, FunctionDefinition};

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

/// Registry of available tools
pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
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
            tools: std::collections::HashMap::new(),
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

/// Built-in tool: Execute Python code
#[allow(dead_code)]
pub struct PythonExecuteTool;

#[async_trait]
impl Tool for PythonExecuteTool {
    fn name(&self) -> &str {
        "python_execute"
    }

    fn description(&self) -> &str {
        "Execute Python code in a sandboxed environment"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "The Python code to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Execution timeout in seconds (default: 30)"
                }
            },
            "required": ["code"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::InvalidInput("Missing 'code' parameter".to_string()))?;

        // This will be implemented to use the sandbox module
        Ok(ToolResult::success(format!(
            "Code execution not yet implemented. Code:\n{}",
            code
        )))
    }
}

/// Built-in tool: Read file
pub struct ReadFileTool {
    allowed_dir: std::path::PathBuf,
}

impl ReadFileTool {
    pub fn new(allowed_dir: std::path::PathBuf) -> Self {
        ReadFileTool { allowed_dir }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read (relative to workspace)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::InvalidInput("Missing 'path' parameter".to_string()))?;

        let full_path = self.allowed_dir.join(path);

        // Security check: ensure path is within allowed directory
        if !full_path.starts_with(&self.allowed_dir) {
            return Ok(ToolResult::failure("Access denied: path outside workspace"));
        }

        match tokio::fs::read_to_string(&full_path).await {
            Ok(content) => Ok(ToolResult::success(content)),
            Err(e) => Ok(ToolResult::failure(format!("Failed to read file: {}", e))),
        }
    }
}

/// Built-in tool: Write file
pub struct WriteFileTool {
    allowed_dir: std::path::PathBuf,
}

impl WriteFileTool {
    pub fn new(allowed_dir: std::path::PathBuf) -> Self {
        WriteFileTool { allowed_dir }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write (relative to workspace)"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::InvalidInput("Missing 'path' parameter".to_string()))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                crate::Error::InvalidInput("Missing 'content' parameter".to_string())
            })?;

        let full_path = self.allowed_dir.join(path);

        // Security check: ensure path is within allowed directory
        if !full_path.starts_with(&self.allowed_dir) {
            return Ok(ToolResult::failure("Access denied: path outside workspace"));
        }

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(ToolResult::failure(format!(
                    "Failed to create directories: {}",
                    e
                )));
            }
        }

        match tokio::fs::write(&full_path, content).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "Successfully wrote {} bytes to {}",
                content.len(),
                path
            ))),
            Err(e) => Ok(ToolResult::failure(format!("Failed to write file: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result() {
        let success = ToolResult::success("Done!");
        assert!(success.success);
        assert_eq!(success.content.as_deref(), Some("Done!"));

        let failure = ToolResult::failure("Oops!");
        assert!(!failure.success);
        assert_eq!(failure.error.as_deref(), Some("Oops!"));
    }

    #[tokio::test]
    async fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(PythonExecuteTool);

        assert_eq!(registry.count(), 1);
        assert!(registry.get("python_execute").is_some());
        assert!(registry.get("unknown").is_none());
    }
}
