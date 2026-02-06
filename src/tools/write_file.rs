//! Write file tool
//!
//! Allows the agent to write/create files in the workspace.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

use super::traits::{Tool, ToolResult};
use crate::error::Result;

/// Built-in tool: Write file
pub struct WriteFileTool {
    allowed_dir: PathBuf,
}

impl WriteFileTool {
    pub fn new(allowed_dir: PathBuf) -> Self {
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
