//! Memory retrieval orchestrator
//!
//! Ties together embedding generation, caching, semantic search (pgvector),
//! and full-text search (tsvector) into a single retrieval pipeline.

use crate::database::{Memory, MemoryStore};
use crate::error::Result;
use std::collections::HashSet;
use tracing::{info, warn};

use super::cache::MemoryCache;
use super::embedding::EmbeddingService;

/// Orchestrates memory retrieval across semantic and full-text search
#[derive(Clone)]
pub struct MemoryRetriever {
    store: MemoryStore,
    embedding: EmbeddingService,
    cache: MemoryCache,
}

impl MemoryRetriever {
    /// Create a new memory retriever
    pub fn new(store: MemoryStore, embedding: EmbeddingService, cache: MemoryCache) -> Self {
        MemoryRetriever {
            store,
            embedding,
            cache,
        }
    }

    /// Retrieve relevant memories for a query, formatted as context string
    pub async fn retrieve(&self, user_id: &str, query: &str, limit: usize) -> Result<String> {
        // 1. Check search result cache
        if let Some(cached) = self.cache.get_search_results(user_id, query).await {
            info!("Memory cache hit for user={}", user_id);
            return Ok(format_memories(&cached));
        }

        // 2. Generate/get embedding for query
        let query_embedding = self.get_or_create_embedding(query).await?;

        // 3. Run semantic search (pgvector)
        let semantic_results = match self
            .store
            .search_semantic(user_id, query_embedding, limit, 0.3)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                warn!("Semantic search failed: {}", e);
                vec![]
            }
        };

        // 4. Run full-text search (tsvector)
        let fulltext_results = match self.store.search_fulltext(user_id, query, limit).await {
            Ok(results) => results,
            Err(e) => {
                warn!("Full-text search failed: {}", e);
                vec![]
            }
        };

        // 5. Deduplicate by memory ID
        let mut seen = HashSet::new();
        let mut combined: Vec<Memory> = Vec::new();

        // Semantic results first (more relevant)
        for (memory, _score) in semantic_results {
            if seen.insert(memory.id) {
                combined.push(memory);
            }
        }

        // Then full-text results
        for memory in fulltext_results {
            if seen.insert(memory.id) {
                combined.push(memory);
            }
        }

        // 6. Truncate to limit
        combined.truncate(limit);

        // 7. Record access for retrieved memories
        for memory in &combined {
            let _ = self.store.record_access(memory.id).await;
        }

        // 8. Cache results
        self.cache
            .put_search_results(user_id, query, combined.clone())
            .await;

        info!(
            "Retrieved {} memories for user={}",
            combined.len(),
            user_id
        );

        // 9. Format as context string
        Ok(format_memories(&combined))
    }

    /// Save a memory with embedding
    pub async fn save_memory(&self, memory: &Memory) -> Result<()> {
        // Generate embedding for the content
        let embedding = match self.embedding.embed(&memory.content).await {
            Ok(emb) => Some(emb),
            Err(e) => {
                warn!("Failed to generate embedding: {}", e);
                None
            }
        };

        // Save to store
        self.store.save(memory, embedding).await?;

        // Invalidate user's search cache
        self.cache.invalidate_user_search(&memory.user_id).await;

        Ok(())
    }

    /// Get or create an embedding (using cache)
    async fn get_or_create_embedding(&self, text: &str) -> Result<Vec<f32>> {
        if let Some(cached) = self.cache.get_embedding(text).await {
            return Ok(cached);
        }

        let embedding = self.embedding.embed(text).await?;
        self.cache.put_embedding(text, embedding.clone()).await;
        Ok(embedding)
    }

    /// Get a reference to the underlying store
    pub fn store(&self) -> &MemoryStore {
        &self.store
    }
}

/// Format memories into a context string for injection into system prompt
fn format_memories(memories: &[Memory]) -> String {
    if memories.is_empty() {
        return String::new();
    }

    let mut output = String::from("\n\n---\n\n## Relevant Memories\n\n");

    for (i, memory) in memories.iter().enumerate() {
        output.push_str(&format!("{}. {}", i + 1, memory.content));
        if !memory.tags.is_empty() {
            output.push_str(&format!(" [{}]", memory.tags.join(", ")));
        }
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_memories_empty() {
        assert_eq!(format_memories(&[]), "");
    }

    #[test]
    fn test_format_memories() {
        let memories = vec![
            Memory::new("user", "First memory").with_tags(vec!["tag1".into()]),
            Memory::new("user", "Second memory"),
        ];

        let result = format_memories(&memories);
        assert!(result.contains("## Relevant Memories"));
        assert!(result.contains("1. First memory [tag1]"));
        assert!(result.contains("2. Second memory"));
    }
}
