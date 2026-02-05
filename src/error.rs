//! Error types for OpenAgent

use thiserror::Error;

/// Result type alias using OpenAgent's Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for OpenAgent
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// OpenRouter API error
    #[error("OpenRouter API error: {0}")]
    OpenRouter(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// OpenSearch error
    #[error("OpenSearch error: {0}")]
    OpenSearch(String),

    /// Telegram bot error
    #[error("Telegram error: {0}")]
    Telegram(String),

    /// Sandbox execution error
    #[error("Sandbox error: {0}")]
    Sandbox(String),

    /// Wasm runtime error
    #[error("Wasm runtime error: {0}")]
    Wasm(String),

    /// Docker/container error
    #[error("Container error: {0}")]
    Container(String),

    /// HTTP request error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

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

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Timeout error
    #[error("Timeout: {0}")]
    Timeout(String),

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
        )
    }

    /// Check if error is a client error (user's fault)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Error::InvalidInput(_) | Error::NotFound(_) | Error::Unauthorized(_)
        )
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
