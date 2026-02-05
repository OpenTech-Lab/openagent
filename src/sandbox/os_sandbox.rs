//! OS-level sandboxed execution
//!
//! Runs code in a restricted directory with minimal permissions.
//! This is the least secure option but works without additional dependencies.

use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::error::{Error, Result};
use crate::sandbox::executor::{CodeExecutor, ExecutionRequest, ExecutionResult, Language};

/// OS-level sandbox executor
pub struct OsSandbox {
    /// Allowed directory for execution
    allowed_dir: PathBuf,
}

impl OsSandbox {
    /// Create a new OS sandbox
    pub fn new(allowed_dir: PathBuf) -> Self {
        OsSandbox { allowed_dir }
    }

    /// Get the command for a language
    fn get_command(&self, language: Language) -> Result<(String, Vec<String>)> {
        match language {
            Language::Python => Ok(("python3".to_string(), vec!["-c".to_string()])),
            Language::JavaScript => Ok(("node".to_string(), vec!["-e".to_string()])),
            Language::Bash => Ok(("bash".to_string(), vec!["-c".to_string()])),
            Language::TypeScript => {
                // Use ts-node or deno for TypeScript
                if which::which("deno").is_ok() {
                    Ok(("deno".to_string(), vec!["eval".to_string()]))
                } else if which::which("ts-node").is_ok() {
                    Ok(("ts-node".to_string(), vec!["-e".to_string()]))
                } else {
                    Err(Error::Sandbox(
                        "TypeScript runtime not found (deno or ts-node)".to_string(),
                    ))
                }
            }
            Language::Rust => Err(Error::Sandbox(
                "Rust inline execution not supported in OS mode".to_string(),
            )),
            Language::Go => Err(Error::Sandbox(
                "Go inline execution not supported in OS mode".to_string(),
            )),
        }
    }

    /// Validate that a path is within the allowed directory
    fn validate_path(&self, path: &PathBuf) -> Result<()> {
        let canonical = path
            .canonicalize()
            .unwrap_or_else(|_| path.clone());

        if !canonical.starts_with(&self.allowed_dir) {
            return Err(Error::Sandbox(format!(
                "Path {} is outside allowed directory",
                path.display()
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl CodeExecutor for OsSandbox {
    fn name(&self) -> &str {
        "os"
    }

    fn supports_language(&self, language: Language) -> bool {
        matches!(
            language,
            Language::Python | Language::JavaScript | Language::Bash | Language::TypeScript
        )
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
            Language::Bash,
        ]
    }

    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult> {
        // Get command for language
        let (cmd, args) = self.get_command(request.language)?;

        // Ensure allowed directory exists
        tokio::fs::create_dir_all(&self.allowed_dir).await?;

        // Determine working directory
        let working_dir = match &request.working_dir {
            Some(dir) => {
                let path = self.allowed_dir.join(dir);
                self.validate_path(&path)?;
                path
            }
            None => self.allowed_dir.clone(),
        };

        debug!(
            "Executing {} code in OS sandbox (working_dir: {})",
            request.language,
            working_dir.display()
        );

        let start = Instant::now();

        // Build command
        let mut command = Command::new(&cmd);
        command
            .args(&args)
            .arg(&request.code)
            .current_dir(&working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add environment variables
        for (key, value) in &request.env {
            command.env(key, value);
        }

        // Spawn process
        let mut child = command.spawn().map_err(|e| {
            Error::Sandbox(format!("Failed to spawn process: {}", e))
        })?;

        // Write stdin if provided
        if let Some(stdin_data) = &request.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(stdin_data.as_bytes()).await?;
            }
        }

        // Wait with timeout
        let result = tokio::time::timeout(request.timeout, child.wait_with_output()).await;

        let execution_time = start.elapsed();

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                if output.status.success() {
                    Ok(ExecutionResult {
                        success: true,
                        exit_code: Some(exit_code),
                        stdout,
                        stderr,
                        execution_time,
                        timed_out: false,
                        memory_used: None,
                    })
                } else {
                    Ok(ExecutionResult {
                        success: false,
                        exit_code: Some(exit_code),
                        stdout,
                        stderr,
                        execution_time,
                        timed_out: false,
                        memory_used: None,
                    })
                }
            }
            Ok(Err(e)) => Err(Error::Sandbox(format!("Process error: {}", e))),
            Err(_) => {
                // Timeout - try to kill the process
                warn!("Execution timed out after {:?}", request.timeout);

                // Note: child is dropped here, which should kill the process
                Ok(ExecutionResult::timeout(
                    String::new(),
                    "Execution timed out".to_string(),
                    request.timeout,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_python_execution() {
        let dir = tempdir().unwrap();
        let sandbox = OsSandbox::new(dir.path().to_path_buf());

        let request = ExecutionRequest::new("print('Hello, World!')", Language::Python);
        let result = sandbox.execute(request).await.unwrap();

        assert!(result.success);
        assert!(result.stdout.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_timeout() {
        let dir = tempdir().unwrap();
        let sandbox = OsSandbox::new(dir.path().to_path_buf());

        let request = ExecutionRequest::new("import time; time.sleep(10)", Language::Python)
            .with_timeout(Duration::from_millis(100));

        let result = sandbox.execute(request).await.unwrap();

        assert!(!result.success);
        assert!(result.timed_out);
    }
}
