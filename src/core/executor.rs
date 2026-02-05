//! Executor trait - Abstract interface for code execution
//!
//! This module defines the `CodeExecutor` trait that allows OpenAgent to
//! execute code in various sandboxed environments:
//! - OS sandbox (restricted paths)
//! - WebAssembly (Wasmtime)
//! - Container (Docker)
//!
//! The trait-based approach enables:
//! - Runtime selection of execution environment
//! - Easy addition of new execution backends
//! - Consistent security policies across backends

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::Result;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Python
    Python,
    /// JavaScript (Node.js)
    JavaScript,
    /// TypeScript
    TypeScript,
    /// Rust
    Rust,
    /// Shell/Bash
    Shell,
    /// Go
    Go,
    /// Ruby
    Ruby,
}

impl std::str::FromStr for Language {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Language::Python),
            "javascript" | "js" | "node" => Ok(Language::JavaScript),
            "typescript" | "ts" => Ok(Language::TypeScript),
            "rust" | "rs" => Ok(Language::Rust),
            "shell" | "bash" | "sh" => Ok(Language::Shell),
            "go" | "golang" => Ok(Language::Go),
            "ruby" | "rb" => Ok(Language::Ruby),
            _ => Err(crate::error::Error::InvalidInput(format!(
                "Unknown language: {}. Supported: python, javascript, typescript, rust, shell, go, ruby",
                s
            ))),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::Python => write!(f, "python"),
            Language::JavaScript => write!(f, "javascript"),
            Language::TypeScript => write!(f, "typescript"),
            Language::Rust => write!(f, "rust"),
            Language::Shell => write!(f, "shell"),
            Language::Go => write!(f, "go"),
            Language::Ruby => write!(f, "ruby"),
        }
    }
}

/// Request to execute code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// Programming language
    pub language: Language,
    /// Source code to execute
    pub code: String,
    /// Timeout for execution
    #[serde(with = "humantime_serde", default = "default_timeout")]
    pub timeout: Duration,
    /// Working directory
    pub working_dir: Option<PathBuf>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Stdin input
    pub stdin: Option<String>,
    /// Command-line arguments
    #[serde(default)]
    pub args: Vec<String>,
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

impl ExecutionRequest {
    /// Create a new execution request
    pub fn new(language: Language, code: impl Into<String>) -> Self {
        ExecutionRequest {
            language,
            code: code.into(),
            timeout: default_timeout(),
            working_dir: None,
            env: HashMap::new(),
            stdin: None,
            args: Vec::new(),
        }
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the working directory
    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Add an environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set stdin input
    pub fn with_stdin(mut self, stdin: impl Into<String>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    /// Add command-line arguments
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

/// Result of code execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether the execution was successful (exit code 0)
    pub success: bool,
    /// Exit code
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution time
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    /// Whether the execution timed out
    pub timed_out: bool,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl ExecutionResult {
    /// Create a successful result
    pub fn success(stdout: impl Into<String>, duration: Duration) -> Self {
        ExecutionResult {
            success: true,
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
            duration,
            timed_out: false,
            metadata: None,
        }
    }

    /// Create a failed result
    pub fn failure(exit_code: i32, stderr: impl Into<String>, duration: Duration) -> Self {
        ExecutionResult {
            success: false,
            exit_code,
            stdout: String::new(),
            stderr: stderr.into(),
            duration,
            timed_out: false,
            metadata: None,
        }
    }

    /// Create a timeout result
    pub fn timeout(duration: Duration) -> Self {
        ExecutionResult {
            success: false,
            exit_code: -1,
            stdout: String::new(),
            stderr: "Execution timed out".to_string(),
            duration,
            timed_out: true,
            metadata: None,
        }
    }

    /// Get combined output (stdout + stderr)
    pub fn combined_output(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

/// Metadata about an executor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorMeta {
    /// Unique executor identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description
    pub description: String,
    /// Supported languages
    pub supported_languages: Vec<Language>,
    /// Security level (higher = more isolated)
    pub security_level: u8,
}

/// Abstract interface for code executors
///
/// Implement this trait to add support for new execution environments.
#[async_trait]
pub trait CodeExecutor: Send + Sync {
    /// Get executor metadata
    fn meta(&self) -> &ExecutorMeta;

    /// Get the executor ID
    fn id(&self) -> &str {
        &self.meta().id
    }

    /// Get supported languages
    fn supported_languages(&self) -> &[Language] {
        &self.meta().supported_languages
    }

    /// Check if a language is supported
    fn supports_language(&self, language: &Language) -> bool {
        self.supported_languages().contains(language)
    }

    /// Execute code
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult>;

    /// Health check
    async fn health_check(&self) -> Result<bool>;

    /// Cleanup any resources (containers, temp files, etc.)
    async fn cleanup(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_parsing() {
        assert_eq!("python".parse::<Language>().unwrap(), Language::Python);
        assert_eq!("py".parse::<Language>().unwrap(), Language::Python);
        assert_eq!("js".parse::<Language>().unwrap(), Language::JavaScript);
        assert!("unknown".parse::<Language>().is_err());
    }

    #[test]
    fn test_execution_request_builder() {
        let request = ExecutionRequest::new(Language::Python, "print('hello')")
            .with_timeout(Duration::from_secs(60))
            .with_env("PATH", "/usr/bin");

        assert_eq!(request.language, Language::Python);
        assert_eq!(request.timeout, Duration::from_secs(60));
        assert_eq!(request.env.get("PATH"), Some(&"/usr/bin".to_string()));
    }

    #[test]
    fn test_execution_result() {
        let result = ExecutionResult::success("Hello!", Duration::from_millis(100));
        assert!(result.success);
        assert_eq!(result.exit_code, 0);

        let failed = ExecutionResult::failure(1, "Error!", Duration::from_millis(50));
        assert!(!failed.success);
        assert_eq!(failed.exit_code, 1);

        let timeout = ExecutionResult::timeout(Duration::from_secs(30));
        assert!(timeout.timed_out);
    }
}
