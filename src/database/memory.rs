//! Memory storage and retrieval

use crate::database::PostgresPool;
use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use pgvector::Vector;

/// A memory entry
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Memory {
    /// Unique memory ID
    pub id: Uuid,
    /// User ID who owns this memory
    pub user_id: String,
    /// Main content of the memory
    pub content: String,
    /// Optional short summary
    #[sqlx(default)]
    pub summary: Option<String>,
    /// Importance score (0.0 - 1.0)
    pub importance: f32,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// When the memory was created
    pub created_at: DateTime<Utc>,
    /// When the memory was last updated
    pub updated_at: DateTime<Utc>,
    /// When the memory was last accessed
    pub accessed_at: DateTime<Utc>,
    /// Number of times this memory has been accessed
    pub access_count: i32,
}

impl Memory {
    /// Create a new memory
    pub fn new(user_id: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Memory {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            content: content.into(),
            summary: None,
            importance: 0.5,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        }
    }

    /// Set the summary
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Set the importance
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Set the tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Add a tag
    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

/// Memory store backed by PostgreSQL + pgvector
#[derive(Clone)]
pub struct MemoryStore {
    pg_pool: PostgresPool,
}

impl MemoryStore {
    /// Create a new memory store
    pub fn new(pg_pool: PostgresPool) -> Self {
        MemoryStore { pg_pool }
    }

    /// Save a memory
    pub async fn save(&self, memory: &Memory, embedding: Option<Vec<f32>>) -> Result<()> {
        let embedding_vec = embedding.map(Vector::from);

        sqlx::query(r#"
            INSERT INTO memories (id, user_id, content, summary, embedding, importance, tags, created_at, updated_at, accessed_at, access_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) DO UPDATE SET
                content = EXCLUDED.content,
                summary = EXCLUDED.summary,
                embedding = EXCLUDED.embedding,
                importance = EXCLUDED.importance,
                tags = EXCLUDED.tags,
                updated_at = EXCLUDED.updated_at
        "#)
        .bind(memory.id)
        .bind(&memory.user_id)
        .bind(&memory.content)
        .bind(&memory.summary)
        .bind(embedding_vec)
        .bind(memory.importance)
        .bind(&memory.tags)
        .bind(memory.created_at)
        .bind(memory.updated_at)
        .bind(memory.accessed_at)
        .bind(memory.access_count)
        .execute(&self.pg_pool)
        .await?;

        Ok(())
    }

    /// Get a memory by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<Memory>> {
        let memory: Option<Memory> = sqlx::query_as(
            "SELECT id, user_id, content, summary, importance, tags, created_at, updated_at, accessed_at, access_count FROM memories WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pg_pool)
        .await?;

        Ok(memory)
    }

    /// Update access timestamp and count
    pub async fn record_access(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE memories SET accessed_at = NOW(), access_count = access_count + 1 WHERE id = $1"
        )
        .bind(id)
        .execute(&self.pg_pool)
        .await?;

        Ok(())
    }

    /// Delete a memory
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM memories WHERE id = $1")
            .bind(id)
            .execute(&self.pg_pool)
            .await?;

        Ok(())
    }

    /// Search memories by semantic similarity using pgvector
    pub async fn search_semantic(
        &self,
        user_id: &str,
        query_embedding: Vec<f32>,
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<(Memory, f32)>> {
        let embedding = Vector::from(query_embedding);

        #[derive(FromRow)]
        struct MemoryWithScore {
            id: Uuid,
            user_id: String,
            content: String,
            summary: Option<String>,
            importance: f32,
            tags: Vec<String>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            accessed_at: DateTime<Utc>,
            access_count: i32,
            similarity: f32,
        }

        let results: Vec<MemoryWithScore> = sqlx::query_as(r#"
            SELECT
                id, user_id, content, summary, importance, tags,
                created_at, updated_at, accessed_at, access_count,
                1 - (embedding <=> $1) as similarity
            FROM memories
            WHERE user_id = $2 AND embedding IS NOT NULL
            AND 1 - (embedding <=> $1) > $3
            ORDER BY embedding <=> $1
            LIMIT $4
        "#)
        .bind(&embedding)
        .bind(user_id)
        .bind(min_similarity)
        .bind(limit as i32)
        .fetch_all(&self.pg_pool)
        .await?;

        Ok(results
            .into_iter()
            .map(|r| {
                (
                    Memory {
                        id: r.id,
                        user_id: r.user_id,
                        content: r.content,
                        summary: r.summary,
                        importance: r.importance,
                        tags: r.tags,
                        created_at: r.created_at,
                        updated_at: r.updated_at,
                        accessed_at: r.accessed_at,
                        access_count: r.access_count,
                    },
                    r.similarity,
                )
            })
            .collect())
    }

    /// Search memories by full-text using PostgreSQL tsvector
    pub async fn search_fulltext(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        // Build tsquery from space-separated words (joined with &)
        let tsquery_str: String = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" & ");

        if tsquery_str.is_empty() {
            return Ok(vec![]);
        }

        // Use tsvector search with ts_rank for relevance ordering
        let memories: Vec<Memory> = sqlx::query_as(r#"
            SELECT id, user_id, content, summary, importance, tags,
                   created_at, updated_at, accessed_at, access_count
            FROM memories
            WHERE user_id = $1
              AND search_vector @@ to_tsquery('simple', $2)
            ORDER BY ts_rank(search_vector, to_tsquery('simple', $2)) DESC,
                     importance DESC, accessed_at DESC
            LIMIT $3
        "#)
        .bind(user_id)
        .bind(&tsquery_str)
        .bind(limit as i32)
        .fetch_all(&self.pg_pool)
        .await?;

        Ok(memories)
    }

    /// Get all memories for a user
    pub async fn get_all(&self, user_id: &str, limit: usize) -> Result<Vec<Memory>> {
        let memories: Vec<Memory> = sqlx::query_as(r#"
            SELECT id, user_id, content, summary, importance, tags,
                   created_at, updated_at, accessed_at, access_count
            FROM memories
            WHERE user_id = $1
            ORDER BY importance DESC, accessed_at DESC
            LIMIT $2
        "#)
        .bind(user_id)
        .bind(limit as i32)
        .fetch_all(&self.pg_pool)
        .await?;

        Ok(memories)
    }

    /// Get memories by tag
    pub async fn get_by_tag(&self, user_id: &str, tag: &str, limit: usize) -> Result<Vec<Memory>> {
        let memories: Vec<Memory> = sqlx::query_as(r#"
            SELECT id, user_id, content, summary, importance, tags,
                   created_at, updated_at, accessed_at, access_count
            FROM memories
            WHERE user_id = $1 AND $2 = ANY(tags)
            ORDER BY importance DESC, accessed_at DESC
            LIMIT $3
        "#)
        .bind(user_id)
        .bind(tag)
        .bind(limit as i32)
        .fetch_all(&self.pg_pool)
        .await?;

        Ok(memories)
    }

    /// Get high-importance memories
    pub async fn get_important(
        &self,
        user_id: &str,
        min_importance: f32,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let memories: Vec<Memory> = sqlx::query_as(r#"
            SELECT id, user_id, content, summary, importance, tags,
                   created_at, updated_at, accessed_at, access_count
            FROM memories
            WHERE user_id = $1 AND importance >= $2
            ORDER BY importance DESC, accessed_at DESC
            LIMIT $3
        "#)
        .bind(user_id)
        .bind(min_importance)
        .bind(limit as i32)
        .fetch_all(&self.pg_pool)
        .await?;

        Ok(memories)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_creation() {
        let memory = Memory::new("user123", "Test content")
            .with_summary("A test")
            .with_importance(0.8)
            .with_tags(vec!["test".to_string()]);

        assert_eq!(memory.user_id, "user123");
        assert_eq!(memory.content, "Test content");
        assert_eq!(memory.summary.as_deref(), Some("A test"));
        assert_eq!(memory.importance, 0.8);
        assert_eq!(memory.tags, vec!["test"]);
    }

    #[test]
    fn test_importance_clamping() {
        let memory = Memory::new("user", "content").with_importance(1.5);
        assert_eq!(memory.importance, 1.0);

        let memory = Memory::new("user", "content").with_importance(-0.5);
        assert_eq!(memory.importance, 0.0);
    }
}
