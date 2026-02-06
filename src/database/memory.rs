//! Memory storage and retrieval

use crate::database::PostgresPool;
use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use pgvector::Vector;

/// Memory type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    /// Events, conversations, experiences
    Episodic,
    /// Facts, knowledge, preferences
    Semantic,
    /// How-to knowledge, workflows, procedures
    Procedural,
}

impl MemoryType {
    pub fn as_str(&self) -> &str {
        match self {
            MemoryType::Episodic => "episodic",
            MemoryType::Semantic => "semantic",
            MemoryType::Procedural => "procedural",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "episodic" => MemoryType::Episodic,
            "procedural" => MemoryType::Procedural,
            _ => MemoryType::Semantic,
        }
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for MemoryType {
    fn default() -> Self {
        MemoryType::Semantic
    }
}

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
    /// Memory type: "episodic", "semantic", "procedural"
    #[sqlx(default)]
    pub memory_type: String,
    /// Structured metadata (source conversation, step details, etc.)
    #[sqlx(default)]
    pub metadata: serde_json::Value,
    /// Source of this memory (e.g., "tool:memory_save", "auto:episodic")
    #[sqlx(default)]
    pub source: String,
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
            memory_type: "semantic".to_string(),
            metadata: serde_json::json!({}),
            source: "unknown".to_string(),
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

    /// Set the memory type
    pub fn with_memory_type(mut self, memory_type: MemoryType) -> Self {
        self.memory_type = memory_type.as_str().to_string();
        self
    }

    /// Set structured metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the source
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// Get the parsed memory type
    pub fn parsed_type(&self) -> MemoryType {
        MemoryType::from_str(&self.memory_type)
    }
}

/// Pre-built parameterized SQL templates for memory operations
pub(crate) mod sql {
    /// Standard column list for SELECT queries
    pub const COLUMNS: &str = "id, user_id, content, summary, importance, tags, memory_type, metadata, source, created_at, updated_at, accessed_at, access_count";

    /// Semantic search with optional type filter
    pub const SEARCH_SEMANTIC_TYPED: &str = r#"
        SELECT id, user_id, content, summary, importance, tags,
               memory_type, metadata, source,
               created_at, updated_at, accessed_at, access_count,
               1 - (embedding <=> $1) as similarity
        FROM memories
        WHERE user_id = $2 AND embedding IS NOT NULL
          AND 1 - (embedding <=> $1) > $3
          AND ($5::text IS NULL OR memory_type = $5)
        ORDER BY embedding <=> $1
        LIMIT $4
    "#;

    /// Full-text search with BM25 score and optional type filter
    pub const SEARCH_FULLTEXT_SCORED_TYPED: &str = r#"
        SELECT id, user_id, content, summary, importance, tags,
               memory_type, metadata, source,
               created_at, updated_at, accessed_at, access_count,
               ts_rank(search_vector, to_tsquery('simple', $2)) as rank_score
        FROM memories
        WHERE user_id = $1
          AND search_vector @@ to_tsquery('simple', $2)
          AND ($4::text IS NULL OR memory_type = $4)
        ORDER BY rank_score DESC
        LIMIT $3
    "#;

    /// Recent memories by type
    pub const RECENT_BY_TYPE: &str = r#"
        SELECT id, user_id, content, summary, importance, tags,
               memory_type, metadata, source,
               created_at, updated_at, accessed_at, access_count
        FROM memories
        WHERE user_id = $1 AND memory_type = $2
        ORDER BY created_at DESC
        LIMIT $3
    "#;

    /// Find similar memories by embedding (for duplicate detection)
    pub const FIND_SIMILAR_BY_EMBEDDING: &str = r#"
        SELECT id, user_id, content, summary, importance, tags,
               memory_type, metadata, source,
               created_at, updated_at, accessed_at, access_count,
               1 - (embedding <=> $1) as similarity
        FROM memories
        WHERE user_id = $2 AND embedding IS NOT NULL
          AND 1 - (embedding <=> $1) > $3
        ORDER BY embedding <=> $1
        LIMIT $4
    "#;
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
            INSERT INTO memories (id, user_id, content, summary, embedding, importance, tags,
                                  memory_type, metadata, source,
                                  created_at, updated_at, accessed_at, access_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (id) DO UPDATE SET
                content = EXCLUDED.content,
                summary = EXCLUDED.summary,
                embedding = EXCLUDED.embedding,
                importance = EXCLUDED.importance,
                tags = EXCLUDED.tags,
                memory_type = EXCLUDED.memory_type,
                metadata = EXCLUDED.metadata,
                source = EXCLUDED.source,
                updated_at = EXCLUDED.updated_at
        "#)
        .bind(memory.id)
        .bind(&memory.user_id)
        .bind(&memory.content)
        .bind(&memory.summary)
        .bind(embedding_vec)
        .bind(memory.importance)
        .bind(&memory.tags)
        .bind(&memory.memory_type)
        .bind(&memory.metadata)
        .bind(&memory.source)
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
        let query = format!("SELECT {} FROM memories WHERE id = $1", sql::COLUMNS);
        let memory: Option<Memory> = sqlx::query_as(&query)
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

    /// Search memories by semantic similarity using pgvector, with optional type filter
    pub async fn search_semantic(
        &self,
        user_id: &str,
        query_embedding: Vec<f32>,
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<(Memory, f32)>> {
        self.search_semantic_typed(user_id, query_embedding, limit, min_similarity, None).await
    }

    /// Search memories by semantic similarity with optional type filter
    pub async fn search_semantic_typed(
        &self,
        user_id: &str,
        query_embedding: Vec<f32>,
        limit: usize,
        min_similarity: f32,
        memory_type: Option<&str>,
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
            memory_type: String,
            metadata: serde_json::Value,
            source: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            accessed_at: DateTime<Utc>,
            access_count: i32,
            similarity: f32,
        }

        let results: Vec<MemoryWithScore> = sqlx::query_as(sql::SEARCH_SEMANTIC_TYPED)
            .bind(&embedding)
            .bind(user_id)
            .bind(min_similarity)
            .bind(limit as i32)
            .bind(memory_type)
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
                        memory_type: r.memory_type,
                        metadata: r.metadata,
                        source: r.source,
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

    /// Search memories by full-text using PostgreSQL tsvector (returns scores)
    pub async fn search_fulltext_scored(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(Memory, f32)>> {
        self.search_fulltext_scored_typed(user_id, query, limit, None).await
    }

    /// Search memories by full-text with optional type filter, returning BM25-style scores
    pub async fn search_fulltext_scored_typed(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
        memory_type: Option<&str>,
    ) -> Result<Vec<(Memory, f32)>> {
        let tsquery_str: String = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" & ");

        if tsquery_str.is_empty() {
            return Ok(vec![]);
        }

        #[derive(FromRow)]
        struct MemoryWithRank {
            id: Uuid,
            user_id: String,
            content: String,
            summary: Option<String>,
            importance: f32,
            tags: Vec<String>,
            memory_type: String,
            metadata: serde_json::Value,
            source: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            accessed_at: DateTime<Utc>,
            access_count: i32,
            rank_score: f32,
        }

        let results: Vec<MemoryWithRank> = sqlx::query_as(sql::SEARCH_FULLTEXT_SCORED_TYPED)
            .bind(user_id)
            .bind(&tsquery_str)
            .bind(limit as i32)
            .bind(memory_type)
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
                        memory_type: r.memory_type,
                        metadata: r.metadata,
                        source: r.source,
                        created_at: r.created_at,
                        updated_at: r.updated_at,
                        accessed_at: r.accessed_at,
                        access_count: r.access_count,
                    },
                    r.rank_score,
                )
            })
            .collect())
    }

    /// Search memories by full-text using PostgreSQL tsvector (backward compat)
    pub async fn search_fulltext(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let scored = self.search_fulltext_scored(user_id, query, limit).await?;
        Ok(scored.into_iter().map(|(m, _)| m).collect())
    }

    /// Get all memories for a user
    pub async fn get_all(&self, user_id: &str, limit: usize) -> Result<Vec<Memory>> {
        let query = format!(
            "SELECT {} FROM memories WHERE user_id = $1 ORDER BY importance DESC, accessed_at DESC LIMIT $2",
            sql::COLUMNS
        );
        let memories: Vec<Memory> = sqlx::query_as(&query)
            .bind(user_id)
            .bind(limit as i32)
            .fetch_all(&self.pg_pool)
            .await?;

        Ok(memories)
    }

    /// Get memories by tag
    pub async fn get_by_tag(&self, user_id: &str, tag: &str, limit: usize) -> Result<Vec<Memory>> {
        let query = format!(
            "SELECT {} FROM memories WHERE user_id = $1 AND $2 = ANY(tags) ORDER BY importance DESC, accessed_at DESC LIMIT $3",
            sql::COLUMNS
        );
        let memories: Vec<Memory> = sqlx::query_as(&query)
            .bind(user_id)
            .bind(tag)
            .bind(limit as i32)
            .fetch_all(&self.pg_pool)
            .await?;

        Ok(memories)
    }

    /// Get memories by type
    pub async fn search_by_type(
        &self,
        user_id: &str,
        memory_type: &str,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let memories: Vec<Memory> = sqlx::query_as(sql::RECENT_BY_TYPE)
            .bind(user_id)
            .bind(memory_type)
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
        let query = format!(
            "SELECT {} FROM memories WHERE user_id = $1 AND importance >= $2 ORDER BY importance DESC, accessed_at DESC LIMIT $3",
            sql::COLUMNS
        );
        let memories: Vec<Memory> = sqlx::query_as(&query)
            .bind(user_id)
            .bind(min_importance)
            .bind(limit as i32)
            .fetch_all(&self.pg_pool)
            .await?;

        Ok(memories)
    }

    /// Find similar memories by embedding (for duplicate detection)
    pub async fn find_similar_by_embedding(
        &self,
        user_id: &str,
        query_embedding: Vec<f32>,
        min_similarity: f32,
        limit: usize,
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
            memory_type: String,
            metadata: serde_json::Value,
            source: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            accessed_at: DateTime<Utc>,
            access_count: i32,
            similarity: f32,
        }

        let results: Vec<MemoryWithScore> = sqlx::query_as(sql::FIND_SIMILAR_BY_EMBEDDING)
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
                        memory_type: r.memory_type,
                        metadata: r.metadata,
                        source: r.source,
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
        assert_eq!(memory.memory_type, "semantic");
        assert_eq!(memory.source, "unknown");
    }

    #[test]
    fn test_importance_clamping() {
        let memory = Memory::new("user", "content").with_importance(1.5);
        assert_eq!(memory.importance, 1.0);

        let memory = Memory::new("user", "content").with_importance(-0.5);
        assert_eq!(memory.importance, 0.0);
    }

    #[test]
    fn test_memory_type() {
        let memory = Memory::new("user", "content")
            .with_memory_type(MemoryType::Episodic);
        assert_eq!(memory.memory_type, "episodic");
        assert_eq!(memory.parsed_type(), MemoryType::Episodic);

        let memory = Memory::new("user", "content")
            .with_memory_type(MemoryType::Procedural);
        assert_eq!(memory.memory_type, "procedural");
    }

    #[test]
    fn test_memory_type_from_str() {
        assert_eq!(MemoryType::from_str("episodic"), MemoryType::Episodic);
        assert_eq!(MemoryType::from_str("semantic"), MemoryType::Semantic);
        assert_eq!(MemoryType::from_str("procedural"), MemoryType::Procedural);
        assert_eq!(MemoryType::from_str("unknown"), MemoryType::Semantic); // default
    }

    #[test]
    fn test_memory_metadata_and_source() {
        let memory = Memory::new("user", "content")
            .with_metadata(serde_json::json!({"key": "value"}))
            .with_source("tool:memory_save");

        assert_eq!(memory.metadata, serde_json::json!({"key": "value"}));
        assert_eq!(memory.source, "tool:memory_save");
    }
}
