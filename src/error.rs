//! Error types for OpenAgent
//!
//! Modular error handling following openclaw's pattern of focused error types.

use thiserror::Error;

/// Result type alias using OpenAgent's Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for OpenAgent
///
/// Organized by domain following openclaw's pattern.
#[derive(Error, Debug)]
pub enum Error {
    // ========================================================================
    // Configuration Errors
    // ========================================================================
    
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    // ========================================================================
    // Provider Errors
    // ========================================================================
    
    /// LLM provider error
    #[error("Provider error: {0}")]
    Provider(String),

    /// OpenRouter API error
    #[error("OpenRouter API error: {0}")]
    OpenRouter(String),

    /// Anthropic API error
    #[error("Anthropic API error: {0}")]
    Anthropic(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Authentication/authorization error
    #[error("Auth error: {0}")]
    Auth(String),

    // ========================================================================
    // Channel Errors
    // ========================================================================
    
    /// Channel error
    #[error("Channel error: {0}")]
    Channel(String),

    /// Telegram bot error
    #[error("Telegram error: {0}")]
    Telegram(String),

    /// Discord bot error
    #[error("Discord error: {0}")]
    Discord(String),

    /// Slack error
    #[error("Slack error: {0}")]
    Slack(String),

    // ========================================================================
    // Storage Errors
    // ========================================================================
    
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// OpenSearch error
    #[error("OpenSearch error: {0}")]
    OpenSearch(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    // ========================================================================
    // Execution Errors
    // ========================================================================
    
    /// Sandbox execution error
    #[error("Sandbox error: {0}")]
    Sandbox(String),

    /// Wasm runtime error
    #[error("Wasm runtime error: {0}")]
    Wasm(String),

    /// Docker/container error
    #[error("Container error: {0}")]
    Container(String),

    /// Execution timeout
    #[error("Execution timeout: {0}")]
    ExecutionTimeout(String),

    // ========================================================================
    // Network Errors
    // ========================================================================
    
    /// HTTP request error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// WebSocket error
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Timeout error
    #[error("Timeout: {0}")]
    Timeout(String),

    // ========================================================================
    // Serialization Errors
    // ========================================================================
    
    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing error
    #[error("TOML error: {0}")]
    Toml(String),

    // ========================================================================
    // I/O Errors
    // ========================================================================
    
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(String),

    // ========================================================================
    // Generic Errors
    // ========================================================================
    
    /// Environment variable error
    #[error("Environment error: {0}")]
    Env(#[from] std::env::VarError),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Unauthorized access
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Operation not supported
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// Cancelled operation
    #[error("Cancelled: {0}")]
    Cancelled(String),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::Http(_)
                | Error::OpenSearch(_)
                | Error::RateLimit(_)
                | Error::Timeout(_)
                | Error::Database(_)
                | Error::Connection(_)
                | Error::WebSocket(_)
        )
    }

    /// Check if error is a client error (user's fault)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Error::InvalidInput(_)
                | Error::Validation(_)
                | Error::NotFound(_)
                | Error::Unauthorized(_)
                | Error::Auth(_)
        )
    }

    /// Check if error is a server/internal error
    pub fn is_server_error(&self) -> bool {
        !self.is_client_error() && !self.is_retryable()
    }

    /// Get error code for protocol responses
    pub fn error_code(&self) -> u32 {
        match self {
            Error::Config(_) | Error::Validation(_) => 4001,
            Error::Provider(_) | Error::OpenRouter(_) | Error::Anthropic(_) => 5001,
            Error::RateLimit(_) => 4029,
            Error::Auth(_) | Error::Unauthorized(_) => 4010,
            Error::Channel(_) | Error::Telegram(_) | Error::Discord(_) | Error::Slack(_) => 5002,
            Error::Database(_) | Error::OpenSearch(_) | Error::Storage(_) => 5003,
            Error::Sandbox(_) | Error::Wasm(_) | Error::Container(_) | Error::ExecutionTimeout(_) => 5004,
            Error::Http(_) | Error::WebSocket(_) | Error::Connection(_) | Error::Timeout(_) => 5005,
            Error::Json(_) | Error::Toml(_) => 4002,
            Error::Io(_) | Error::FileNotFound(_) => 5006,
            Error::Env(_) => 4003,
            Error::InvalidInput(_) => 4000,
            Error::NotFound(_) => 4040,
            Error::NotSupported(_) => 4015,
            Error::Cancelled(_) => 4990,
            Error::Internal(_) => 5000,
        }
    }

    /// Get error category for logging/metrics
    pub fn category(&self) -> &'static str {
        match self {
            Error::Config(_) | Error::Validation(_) => "config",
            Error::Provider(_) | Error::OpenRouter(_) | Error::Anthropic(_) | Error::RateLimit(_) => "provider",
            Error::Auth(_) | Error::Unauthorized(_) => "auth",
            Error::Channel(_) | Error::Telegram(_) | Error::Discord(_) | Error::Slack(_) => "channel",
            Error::Database(_) | Error::OpenSearch(_) | Error::Storage(_) => "storage",
            Error::Sandbox(_) | Error::Wasm(_) | Error::Container(_) | Error::ExecutionTimeout(_) => "sandbox",
            Error::Http(_) | Error::WebSocket(_) | Error::Connection(_) | Error::Timeout(_) => "network",
            Error::Json(_) | Error::Toml(_) => "serialization",
            Error::Io(_) | Error::FileNotFound(_) | Error::Env(_) => "io",
            Error::InvalidInput(_) | Error::NotFound(_) | Error::NotSupported(_) | Error::Cancelled(_) | Error::Internal(_) => "general",
        }
    }
}

impl From<config::ConfigError> for Error {
    fn from(err: config::ConfigError) -> Self {
        Error::Config(err.to_string())
    }
}

impl From<bollard::errors::Error> for Error {
    fn from(err: bollard::errors::Error) -> Self {
        Error::Container(err.to_string())
    }
}

impl From<wasmtime::Error> for Error {
    fn from(err: wasmtime::Error) -> Self {
        Error::Wasm(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        assert!(Error::RateLimit("test".to_string()).is_retryable());
        assert!(Error::Timeout("test".to_string()).is_retryable());
        assert!(!Error::InvalidInput("test".to_string()).is_retryable());
    }

    #[test]
    fn test_error_client_error() {
        assert!(Error::InvalidInput("test".to_string()).is_client_error());
        assert!(Error::Unauthorized("test".to_string()).is_client_error());
        assert!(!Error::Internal("test".to_string()).is_client_error());
    }

    #[test]
    fn test_error_category() {
        assert_eq!(Error::Config("test".to_string()).category(), "config");
        assert_eq!(Error::OpenRouter("test".to_string()).category(), "provider");
        assert_eq!(Error::Telegram("test".to_string()).category(), "channel");
    }
}
