//! Storage traits - Abstract interfaces for persistence backends
//!
//! This module defines storage traits that allow OpenAgent to work with
//! different storage backends:
//! - `StorageBackend`: Generic key-value storage
//! - `MemoryBackend`: Vector storage for semantic memory (embeddings)
//! - `SearchBackend`: Full-text search capabilities
//!
//! This follows openclaw's pattern of abstracting storage to enable:
//! - PostgreSQL + pgvector for embeddings
//! - PostgreSQL tsvector for full-text search
//! - SQLite for lightweight deployments
//! - Custom backends for specialized use cases

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::Result;

/// Unique identifier for stored items
pub type StorageKey = String;

/// Metadata for stored items
pub type Metadata = HashMap<String, serde_json::Value>;

/// A memory entry with embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier
    pub id: StorageKey,
    /// Text content
    pub content: String,
    /// Embedding vector
    pub embedding: Vec<f32>,
    /// Associated metadata
    pub metadata: Metadata,
    /// When the entry was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the entry was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Search result from vector search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    /// The memory entry
    pub entry: MemoryEntry,
    /// Similarity score (0.0 - 1.0, higher is more similar)
    pub score: f32,
}

/// A conversation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    /// Unique conversation ID
    pub id: StorageKey,
    /// User/session ID
    pub user_id: String,
    /// Channel ID
    pub channel_id: String,
    /// Conversation messages (serialized)
    pub messages: serde_json::Value,
    /// Conversation metadata
    pub metadata: Metadata,
    /// When the conversation started
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last activity time
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Full-text search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Document ID
    pub id: StorageKey,
    /// Document content (or snippet)
    pub content: String,
    /// Relevance score
    pub score: f32,
    /// Highlighted snippets
    pub highlights: Vec<String>,
    /// Document metadata
    pub metadata: Metadata,
}

/// Abstract interface for key-value storage
///
/// Implement this trait for persistent storage backends.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get the backend ID
    fn id(&self) -> &str;

    /// Store a value
    async fn set<T: Serialize + Send + Sync>(&self, key: &str, value: &T) -> Result<()>;

    /// Retrieve a value
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;

    /// Delete a value
    async fn delete(&self, key: &str) -> Result<()>;

    /// Check if a key exists
    async fn exists(&self, key: &str) -> Result<bool>;

    /// List keys with a prefix
    async fn list_keys(&self, prefix: &str) -> Result<Vec<String>>;

    /// Health check
    async fn health_check(&self) -> Result<bool>;
}

/// Abstract interface for vector/embedding storage
///
/// Implement this trait for semantic memory backends that support
/// similarity search on embeddings.
#[async_trait]
pub trait MemoryBackend: Send + Sync {
    /// Get the backend ID
    fn id(&self) -> &str;

    /// Store a memory entry
    async fn store(&self, entry: &MemoryEntry) -> Result<()>;

    /// Retrieve a memory entry by ID
    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>>;

    /// Delete a memory entry
    async fn delete(&self, id: &str) -> Result<()>;

    /// Search for similar entries
    async fn search(
        &self,
        embedding: &[f32],
        limit: usize,
        filter: Option<&Metadata>,
    ) -> Result<Vec<MemorySearchResult>>;

    /// Store a conversation
    async fn store_conversation(&self, conversation: &ConversationRecord) -> Result<()>;

    /// Retrieve a conversation by ID
    async fn get_conversation(&self, id: &str) -> Result<Option<ConversationRecord>>;

    /// List conversations for a user
    async fn list_conversations(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<ConversationRecord>>;

    /// Health check
    async fn health_check(&self) -> Result<bool>;
}

/// Abstract interface for full-text search
///
/// Implement this trait for search backends like PostgreSQL tsvector.
#[async_trait]
pub trait SearchBackend: Send + Sync {
    /// Get the backend ID
    fn id(&self) -> &str;

    /// Index a document
    async fn index(
        &self,
        index: &str,
        id: &str,
        content: &str,
        metadata: &Metadata,
    ) -> Result<()>;

    /// Search for documents
    async fn search(
        &self,
        index: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>>;

    /// Delete a document
    async fn delete(&self, index: &str, id: &str) -> Result<()>;

    /// Create an index (if not exists)
    async fn create_index(&self, index: &str) -> Result<()>;

    /// Delete an index
    async fn delete_index(&self, index: &str) -> Result<()>;

    /// Health check
    async fn health_check(&self) -> Result<bool>;
}

/// Combined storage that implements all traits
///
/// Useful for backends that support all operations (e.g., PostgreSQL with pgvector).
pub trait UnifiedStorage: StorageBackend + MemoryBackend + SearchBackend {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_creation() {
        let entry = MemoryEntry {
            id: "test-1".to_string(),
            content: "Test content".to_string(),
            embedding: vec![0.1, 0.2, 0.3],
            metadata: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert_eq!(entry.id, "test-1");
        assert_eq!(entry.embedding.len(), 3);
    }
}
