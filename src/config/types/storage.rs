//! Storage configuration types
//!
//! Configuration for storage backends (PostgreSQL, SQLite, etc.)

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Primary storage backend
    #[serde(default)]
    pub backend: StorageBackendType,
    /// PostgreSQL configuration
    pub postgres: Option<PostgresConfig>,
    /// SQLite configuration
    #[serde(default)]
    pub sqlite: SqliteConfig,
    /// Memory configuration
    #[serde(default)]
    pub memory: MemoryStorageConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig {
            backend: StorageBackendType::Sqlite,
            postgres: None,
            sqlite: SqliteConfig::default(),
            memory: MemoryStorageConfig::default(),
        }
    }
}

/// Storage backend type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackendType {
    /// SQLite (default, local file)
    #[default]
    Sqlite,
    /// PostgreSQL with pgvector
    Postgres,
    /// In-memory (no persistence)
    Memory,
}

/// PostgreSQL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// Database URL
    #[serde(skip_serializing)]
    pub url: SecretString,
    /// Maximum connections in pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Connection timeout in seconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// Enable pgvector extension
    #[serde(default = "default_true")]
    pub enable_pgvector: bool,
}

fn default_max_connections() -> u32 {
    5
}

fn default_connect_timeout() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

/// SQLite configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    /// Database file path
    #[serde(default = "default_sqlite_path")]
    pub path: String,
    /// Enable WAL mode
    #[serde(default = "default_true")]
    pub wal_mode: bool,
    /// Busy timeout in milliseconds
    #[serde(default = "default_busy_timeout")]
    pub busy_timeout_ms: u64,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        SqliteConfig {
            path: default_sqlite_path(),
            wal_mode: true,
            busy_timeout_ms: default_busy_timeout(),
        }
    }
}

fn default_sqlite_path() -> String {
    dirs::data_dir()
        .map(|d| d.join("openagent").join("openagent.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("openagent.db"))
        .to_string_lossy()
        .to_string()
}

fn default_busy_timeout() -> u64 {
    5000
}

/// Memory storage configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryStorageConfig {
    /// Memory backend type
    #[serde(default)]
    pub backend: MemoryBackendType,
    /// Enable citations
    #[serde(default)]
    pub citations: CitationsMode,
    /// Embedding configuration
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}

/// Memory backend type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryBackendType {
    /// Built-in memory
    #[default]
    Builtin,
    /// External QMD
    Qmd,
}

/// Citations mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CitationsMode {
    /// Automatic
    #[default]
    Auto,
    /// Always on
    On,
    /// Always off
    Off,
}

/// Embedding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Embedding provider
    #[serde(default = "default_embedding_provider")]
    pub provider: String,
    /// Embedding model
    #[serde(default = "default_embedding_model")]
    pub model: String,
    /// Embedding dimensions
    #[serde(default = "default_embedding_dims")]
    pub dimensions: u32,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        EmbeddingConfig {
            provider: default_embedding_provider(),
            model: default_embedding_model(),
            dimensions: default_embedding_dims(),
        }
    }
}

fn default_embedding_provider() -> String {
    "local".to_string()
}

fn default_embedding_model() -> String {
    "multilingual-e5-small".to_string()
}

fn default_embedding_dims() -> u32 {
    384
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.backend, StorageBackendType::Sqlite);
    }

    #[test]
    fn test_sqlite_config_default() {
        let config = SqliteConfig::default();
        assert!(config.wal_mode);
    }
}
