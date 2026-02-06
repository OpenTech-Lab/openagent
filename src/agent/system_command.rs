//! System command execution tool
//!
//! Allows the agent to execute OS commands like `apt update`, `mv a b`, etc.
//! Supports allowlist/denylist for security control.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

use crate::agent::tools::{Tool, ToolResult};
use crate::error::Result;

/// Tool for executing system commands
///
/// This tool allows the agent to run OS commands with optional
/// working directory restriction, timeout control, and command filtering.
pub struct SystemCommandTool {
    /// Optional working directory for command execution
    working_dir: Option<PathBuf>,
    /// Command execution timeout in seconds
    timeout_secs: u64,
    /// Allowlist of permitted commands (empty = allow all not in denylist)
    allowed_commands: HashSet<String>,
    /// Denylist of forbidden commands (checked first)
    denied_commands: HashSet<String>,
    /// Optional user to run commands as (uses `sudo -u <user>`)
    run_as_user: Option<String>,
    /// Allow shell metacharacters in arguments (&&, |, ;, etc.)
    /// When true, shell injection checks are skipped (for full OS access mode)
    allow_shell_metacharacters: bool,
}

impl Default for SystemCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemCommandTool {
    /// Create a new SystemCommandTool with default settings
    /// By default, dangerous commands are denied
    pub fn new() -> Self {
        let mut denied = HashSet::new();
        // Default denylist for dangerous commands (reduced - allow more operations)
        // Note: sudo is now allowed because package installation requires it
        for cmd in &["rm", "mkfs", "dd", "shutdown", "reboot", "init", "kill", "pkill", "killall"] {
            denied.insert(cmd.to_string());
        }

        SystemCommandTool {
            working_dir: None,
            timeout_secs: 120, // Increased timeout for package installations
            allowed_commands: HashSet::new(),
            denied_commands: denied,
            run_as_user: None,
            allow_shell_metacharacters: false,
        }
    }

    /// Create with a specific user to run commands as
    /// This user should have passwordless sudo configured
    pub fn with_run_as_user(mut self, user: impl Into<String>) -> Self {
        self.run_as_user = Some(user.into());
        self
    }

    /// Create with a specific working directory
    pub fn with_working_dir(working_dir: PathBuf) -> Self {
        let mut tool = Self::new();
        tool.working_dir = Some(working_dir);
        tool
    }

    /// Create with a specific working directory and optional agent user
    pub fn with_config(working_dir: PathBuf, agent_user: Option<String>) -> Self {
        let mut tool = Self::new();
        tool.working_dir = Some(working_dir);
        tool.run_as_user = agent_user;
        tool
    }

    /// Create with full OS access (no denylist, shell metacharacters allowed)
    /// This is for OS mode where the agent acts as a co-worker with full sudo access.
    /// Use with caution - this gives the agent unrestricted command execution.
    pub fn with_full_access(working_dir: Option<PathBuf>) -> Self {
        SystemCommandTool {
            working_dir,
            timeout_secs: 300, // Longer timeout for system operations
            allowed_commands: HashSet::new(),
            denied_commands: HashSet::new(), // No denied commands
            run_as_user: None,
            allow_shell_metacharacters: true, // Allow pipes, &&, etc.
        }
    }

    /// Create with config, choosing full access based on execution environment
    pub fn with_config_and_env(
        working_dir: PathBuf,
        agent_user: Option<String>,
        execution_env: &str,
    ) -> Self {
        if execution_env == "os" {
            // OS mode: full access, no restrictions
            Self::with_full_access(Some(working_dir))
        } else {
            // Sandbox/Container mode: use default restrictions
            Self::with_config(working_dir, agent_user)
        }
    }

    /// Set the timeout in seconds
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set allowed commands (whitelist)
    /// When set, only these commands can be executed
    pub fn with_allowed_commands(mut self, commands: Vec<String>) -> Self {
        self.allowed_commands = commands.into_iter().collect();
        self
    }

    /// Set denied commands (blacklist)
    /// These commands are always blocked
    pub fn with_denied_commands(mut self, commands: Vec<String>) -> Self {
        self.denied_commands = commands.into_iter().collect();
        self
    }

    /// Add a command to the allowlist
    pub fn allow_command(mut self, command: impl Into<String>) -> Self {
        self.allowed_commands.insert(command.into());
        self
    }

    /// Add a command to the denylist
    pub fn deny_command(mut self, command: impl Into<String>) -> Self {
        self.denied_commands.insert(command.into());
        self
    }

    /// Clear the default denylist (use with caution!)
    pub fn clear_denylist(mut self) -> Self {
        self.denied_commands.clear();
        self
    }

    /// Check if a command is allowed
    fn is_command_allowed(&self, command: &str) -> bool {
        // Extract base command (handle paths like /usr/bin/ls)
        let base_cmd = command.rsplit('/').next().unwrap_or(command);

        // Check denylist first (always takes precedence)
        if self.denied_commands.contains(base_cmd) {
            return false;
        }

        // If allowlist is empty, allow all (that aren't denied)
        if self.allowed_commands.is_empty() {
            return true;
        }

        // Check allowlist
        self.allowed_commands.contains(base_cmd)
    }

    /// Get list of denied commands (for error messages)
    pub fn denied_commands_list(&self) -> Vec<&str> {
        self.denied_commands.iter().map(|s| s.as_str()).collect()
    }

    /// Get list of allowed commands (for error messages)
    pub fn allowed_commands_list(&self) -> Vec<&str> {
        self.allowed_commands.iter().map(|s| s.as_str()).collect()
    }
}

#[async_trait]
impl Tool for SystemCommandTool {
    fn name(&self) -> &str {
        "system_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command on the system. Commands are run through /bin/sh for full shell compatibility. Can install packages (sudo apt update && sudo apt install -y nginx), manage services (sudo systemctl start nginx), execute utilities (ls, cat, curl, wget, mkdir, cp, mv, etc.). Returns stdout, stderr, and exit code. Some dangerous commands (rm, kill, shutdown) may be blocked for safety. For package installation, use sudo."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The base command to execute (e.g., 'sudo', 'ls', 'apt', 'curl'). For package installation, use 'sudo' as command."
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Arguments to pass to the command. For 'sudo apt install nginx', use command='sudo' and args=['apt', 'update'] first, then command='sudo' and args=['apt', 'install', '-y', 'nginx']"
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

        // Check if command is allowed
        if !self.is_command_allowed(command) {
            return Ok(ToolResult::failure(format!(
                "Command '{}' is not allowed. Denied commands: {:?}",
                command,
                self.denied_commands_list()
            )));
        }

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

        // Security: Check for dangerous patterns in arguments (unless full access mode)
        if !self.allow_shell_metacharacters {
            for arg in &cmd_args {
                // Block shell injection attempts
                if arg.contains(';') || arg.contains('|') || arg.contains('`')
                    || arg.contains("$(") || arg.contains("&&") || arg.contains("||") {
                    return Ok(ToolResult::failure(format!(
                        "Argument '{}' contains potentially dangerous shell characters",
                        arg
                    )));
                }
            }
        }

        // Parse working directory (optional, can override instance default)
        let working_dir = args
            .get("working_dir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .or_else(|| self.working_dir.clone());

        // Build the command using a shell wrapper for better compatibility
        // This allows shell features like pipes, redirects, and proper PATH resolution
        let full_command = if cmd_args.is_empty() {
            command.to_string()
        } else {
            format!("{} {}", command, cmd_args.join(" "))
        };

        // If run_as_user is configured, wrap command with sudo -u <user>
        let (shell_cmd, shell_args): (&str, Vec<&str>) = if let Some(ref user) = self.run_as_user {
            // Use sudo -n (non-interactive) -u <user> to run as the agent user
            // The agent user should have passwordless sudo configured
            ("sudo", vec!["-n", "-u", user.as_str(), "/bin/sh", "-c", &full_command])
        } else {
            ("/bin/sh", vec!["-c", &full_command])
        };

        let mut cmd = Command::new(shell_cmd);
        cmd.args(&shell_args);

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
    async fn test_system_command_denied() {
        let tool = SystemCommandTool::new();
        let args = serde_json::json!({
            "command": "rm",
            "args": ["-rf", "/"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_system_command_rm_denied() {
        let tool = SystemCommandTool::new();
        let args = serde_json::json!({
            "command": "rm",
            "args": ["-rf", "/tmp/test"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_system_command_allowlist() {
        let tool = SystemCommandTool::new()
            .with_allowed_commands(vec!["echo".to_string(), "cat".to_string()]);

        // Allowed command should work
        let args = serde_json::json!({
            "command": "echo",
            "args": ["test"]
        });
        let result = tool.execute(args).await.unwrap();
        assert!(result.success);

        // Non-allowed command should fail
        let args = serde_json::json!({
            "command": "ls"
        });
        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_system_command_shell_injection_blocked() {
        let tool = SystemCommandTool::new();
        let args = serde_json::json!({
            "command": "echo",
            "args": ["hello; rm -rf /"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("dangerous"));
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
