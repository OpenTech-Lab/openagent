//! Read file tool
//!
//! Allows the agent to read files from the workspace.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

use super::traits::{Tool, ToolResult};
use crate::error::Result;

/// Built-in tool: Read file
pub struct ReadFileTool {
    allowed_dir: PathBuf,
}

impl ReadFileTool {
    pub fn new(allowed_dir: PathBuf) -> Self {
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
