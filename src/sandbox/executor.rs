//! Common executor trait and types

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::error::Result;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Bash,
}

impl std::str::FromStr for Language {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Language::Python),
            "javascript" | "js" => Ok(Language::JavaScript),
            "typescript" | "ts" => Ok(Language::TypeScript),
            "rust" | "rs" => Ok(Language::Rust),
            "go" | "golang" => Ok(Language::Go),
            "bash" | "sh" | "shell" => Ok(Language::Bash),
            _ => Err(crate::Error::InvalidInput(format!(
                "Unsupported language: {}",
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
            Language::Go => write!(f, "go"),
            Language::Bash => write!(f, "bash"),
        }
    }
}

/// Request to execute code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// The code to execute
    pub code: String,
    /// Programming language
    pub language: Language,
    /// Standard input
    #[serde(default)]
    pub stdin: Option<String>,
    /// Execution timeout
    #[serde(default = "default_timeout")]
    pub timeout: Duration,
    /// Environment variables
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Working directory (relative to allowed dir)
    #[serde(default)]
    pub working_dir: Option<String>,
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

impl ExecutionRequest {
    /// Create a new execution request
    pub fn new(code: impl Into<String>, language: Language) -> Self {
        ExecutionRequest {
            code: code.into(),
            language,
            stdin: None,
            timeout: default_timeout(),
            env: std::collections::HashMap::new(),
            working_dir: None,
        }
    }

    /// Set stdin
    pub fn with_stdin(mut self, stdin: impl Into<String>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set working directory
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

/// Result of code execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether execution was successful
    pub success: bool,
    /// Exit code (if applicable)
    pub exit_code: Option<i32>,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution time
    pub execution_time: Duration,
    /// Was execution terminated due to timeout?
    pub timed_out: bool,
    /// Memory usage in bytes (if available)
    pub memory_used: Option<u64>,
}

impl ExecutionResult {
    /// Create a successful result
    pub fn success(stdout: String, execution_time: Duration) -> Self {
        ExecutionResult {
            success: true,
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
            execution_time,
            timed_out: false,
            memory_used: None,
        }
    }

    /// Create a failure result
    pub fn failure(stderr: String, exit_code: i32, execution_time: Duration) -> Self {
        ExecutionResult {
            success: false,
            exit_code: Some(exit_code),
            stdout: String::new(),
            stderr,
            execution_time,
            timed_out: false,
            memory_used: None,
        }
    }

    /// Create a timeout result
    pub fn timeout(partial_stdout: String, partial_stderr: String, timeout: Duration) -> Self {
        ExecutionResult {
            success: false,
            exit_code: None,
            stdout: partial_stdout,
            stderr: partial_stderr,
            execution_time: timeout,
            timed_out: true,
            memory_used: None,
        }
    }

    /// Get combined output
    pub fn combined_output(&self) -> String {
        let mut output = String::new();
        if !self.stdout.is_empty() {
            output.push_str(&self.stdout);
        }
        if !self.stderr.is_empty() {
            if !output.is_empty() {
                output.push_str("\n--- stderr ---\n");
            }
            output.push_str(&self.stderr);
        }
        output
    }
}

/// Trait for code execution backends
#[async_trait]
pub trait CodeExecutor: Send + Sync {
    /// Get the executor name
    fn name(&self) -> &str;

    /// Check if a language is supported
    fn supports_language(&self, language: Language) -> bool;

    /// Execute code
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult>;

    /// Get supported languages
    fn supported_languages(&self) -> Vec<Language> {
        vec![
            Language::Python,
            Language::JavaScript,
            Language::Bash,
        ]
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
    fn test_execution_request() {
        let req = ExecutionRequest::new("print('hello')", Language::Python)
            .with_stdin("input")
            .with_timeout(Duration::from_secs(60))
            .with_env("KEY", "VALUE");

        assert_eq!(req.code, "print('hello')");
        assert_eq!(req.language, Language::Python);
        assert_eq!(req.stdin.as_deref(), Some("input"));
        assert_eq!(req.timeout, Duration::from_secs(60));
        assert_eq!(req.env.get("KEY").map(|s| s.as_str()), Some("VALUE"));
    }

    #[test]
    fn test_execution_result() {
        let success = ExecutionResult::success("output".to_string(), Duration::from_secs(1));
        assert!(success.success);
        assert!(!success.timed_out);

        let timeout = ExecutionResult::timeout(
            "partial".to_string(),
            "".to_string(),
            Duration::from_secs(30),
        );
        assert!(!timeout.success);
        assert!(timeout.timed_out);
    }
}
