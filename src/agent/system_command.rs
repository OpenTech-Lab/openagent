//! System command execution tool
//!
//! Allows the agent to execute OS commands like `apt update`, `mv a b`, etc.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

use crate::agent::tools::{Tool, ToolResult};
use crate::error::Result;

/// Tool for executing system commands
///
/// This tool allows the agent to run OS commands with optional
/// working directory restriction and timeout control.
pub struct SystemCommandTool {
    /// Optional working directory for command execution
    working_dir: Option<PathBuf>,
    /// Command execution timeout in seconds
    timeout_secs: u64,
}

impl Default for SystemCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemCommandTool {
    /// Create a new SystemCommandTool with default settings
    pub fn new() -> Self {
        SystemCommandTool {
            working_dir: None,
            timeout_secs: 60,
        }
    }

    /// Create with a specific working directory
    pub fn with_working_dir(working_dir: PathBuf) -> Self {
        SystemCommandTool {
            working_dir: Some(working_dir),
            timeout_secs: 60,
        }
    }

    /// Set the timeout in seconds
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }
}

#[async_trait]
impl Tool for SystemCommandTool {
    fn name(&self) -> &str {
        "system_command"
    }

    fn description(&self) -> &str {
        "Execute a system/shell command on the OS. Can run commands like 'apt update', 'mv a b', 'ls -la', 'cat file.txt', etc. Returns stdout, stderr, and exit code."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute (e.g., 'ls', 'apt', 'mv')"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Arguments to pass to the command (e.g., ['-la'] for 'ls -la', or ['update'] for 'apt update')"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory for the command (defaults to current directory)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        // Parse command
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::InvalidInput("Missing 'command' parameter".to_string()))?;

        // Parse arguments (optional)
        let cmd_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Parse working directory (optional, can override instance default)
        let working_dir = args
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.working_dir.clone());

        // Build the command
        let mut cmd = Command::new(command);
        cmd.args(&cmd_args);

        // Set working directory if specified
        if let Some(ref dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Execute with timeout
        let timeout = Duration::from_secs(self.timeout_secs);

        let result = tokio::time::timeout(timeout, cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                let success = output.status.success();

                // Build response content
                let mut content = String::new();

                if !stdout.is_empty() {
                    content.push_str("STDOUT:\n");
                    content.push_str(&stdout);
                }

                if !stderr.is_empty() {
                    if !content.is_empty() {
                        content.push_str("\n");
                    }
                    content.push_str("STDERR:\n");
                    content.push_str(&stderr);
                }

                if content.is_empty() {
                    content = format!("Command completed with exit code {}", exit_code);
                }

                // Include metadata with exit code
                let metadata = serde_json::json!({
                    "exit_code": exit_code,
                    "success": success,
                    "command": command,
                    "args": cmd_args,
                });

                if success {
                    Ok(ToolResult::success_with_metadata(content, metadata))
                } else {
                    // Command failed but executed - return as failure with details
                    Ok(ToolResult {
                        success: false,
                        content: Some(content),
                        error: Some(format!("Command exited with code {}", exit_code)),
                        metadata: Some(metadata),
                    })
                }
            }
            Ok(Err(e)) => {
                // Failed to execute command (e.g., command not found)
                Ok(ToolResult::failure(format!("Failed to execute command '{}': {}", command, e)))
            }
            Err(_) => {
                // Timeout
                Ok(ToolResult::failure(format!(
                    "Command '{}' timed out after {} seconds",
                    command, self.timeout_secs
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_command_echo() {
        let tool = SystemCommandTool::new();
        let args = serde_json::json!({
            "command": "echo",
            "args": ["hello", "world"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        assert!(result.content.unwrap().contains("hello world"));
    }

    #[tokio::test]
    async fn test_system_command_ls() {
        let tool = SystemCommandTool::new();
        let args = serde_json::json!({
            "command": "ls",
            "args": ["-la"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_system_command_not_found() {
        let tool = SystemCommandTool::new();
        let args = serde_json::json!({
            "command": "nonexistent_command_xyz"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_system_command_with_working_dir() {
        let tool = SystemCommandTool::with_working_dir(PathBuf::from("/tmp"));
        let args = serde_json::json!({
            "command": "pwd"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        assert!(result.content.unwrap().contains("/tmp"));
    }
}
