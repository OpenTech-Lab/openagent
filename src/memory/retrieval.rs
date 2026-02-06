//! Memory retrieval orchestrator
//!
//! Ties together embedding generation, caching, semantic search (pgvector),
//! and full-text search (tsvector) into a single retrieval pipeline.
//! Uses Reciprocal Rank Fusion (RRF) for hybrid scoring.

use crate::database::{Memory, MemoryStore, MemoryType};
use crate::error::Result;
use chrono::Utc;
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

use super::cache::MemoryCache;
use super::embedding::EmbeddingService;

/// RRF constant (standard value from the original RRF paper)
const RRF_K: f64 = 60.0;

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
        self.retrieve_typed(user_id, query, limit, None).await
    }

    /// Retrieve relevant memories with optional type filter
    pub async fn retrieve_typed(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
        memory_type: Option<MemoryType>,
    ) -> Result<String> {
        // 1. Check search result cache (only for untyped queries)
        if memory_type.is_none() {
            if let Some(cached) = self.cache.get_search_results(user_id, query).await {
                info!("Memory cache hit for user={}", user_id);
                return Ok(format_memories_simple(&cached));
            }
        }

        // 2. Generate/get embedding for query
        let query_embedding = self.get_or_create_embedding(query).await?;

        // 3. Run both searches in parallel
        let type_filter = memory_type.map(|t| t.as_str().to_string());
        let type_ref = type_filter.as_deref();
        let fetch_limit = limit * 2; // Fetch more for better fusion

        let (semantic_result, fulltext_result) = tokio::join!(
            self.store.search_semantic_typed(
                user_id,
                query_embedding,
                fetch_limit,
                0.2,
                type_ref,
            ),
            self.store.search_fulltext_scored_typed(
                user_id,
                query,
                fetch_limit,
                type_ref,
            ),
        );

        let semantic_results = match semantic_result {
            Ok(results) => results,
            Err(e) => {
                warn!("Semantic search failed: {}", e);
                vec![]
            }
        };

        let fulltext_results = match fulltext_result {
            Ok(results) => results,
            Err(e) => {
                warn!("Full-text search failed: {}", e);
                vec![]
            }
        };

        // 4. Build RRF-scored results
        let scored = compute_rrf_scores(&semantic_results, &fulltext_results, limit);

        // 5. Record access for retrieved memories
        for (memory, _) in &scored {
            let _ = self.store.record_access(memory.id).await;
        }

        // 6. Cache results (only for untyped queries)
        if memory_type.is_none() {
            let cache_memories: Vec<Memory> = scored.iter().map(|(m, _)| m.clone()).collect();
            self.cache
                .put_search_results(user_id, query, cache_memories)
                .await;
        }

        info!(
            "Retrieved {} memories for user={} (semantic={}, fulltext={})",
            scored.len(),
            user_id,
            semantic_results.len(),
            fulltext_results.len(),
        );

        // 7. Format as context string
        Ok(format_memories(&scored))
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
    pub(crate) async fn get_or_create_embedding(&self, text: &str) -> Result<Vec<f32>> {
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

    /// Get a reference to the embedding service
    pub fn embedding(&self) -> &EmbeddingService {
        &self.embedding
    }
}

/// Compute Reciprocal Rank Fusion scores across semantic and fulltext results.
///
/// RRF_score(d) = sum over all ranking lists: 1 / (k + rank(d))
///
/// Additionally applies time decay, importance weighting, and access frequency boost.
fn compute_rrf_scores(
    semantic: &[(Memory, f32)],
    fulltext: &[(Memory, f32)],
    limit: usize,
) -> Vec<(Memory, f64)> {
    let now = Utc::now();

    // Build rank maps (0-based rank)
    let mut semantic_rank: HashMap<Uuid, usize> = HashMap::new();
    for (i, (m, _)) in semantic.iter().enumerate() {
        semantic_rank.insert(m.id, i);
    }

    let mut fulltext_rank: HashMap<Uuid, usize> = HashMap::new();
    for (i, (m, _)) in fulltext.iter().enumerate() {
        fulltext_rank.insert(m.id, i);
    }

    // Collect all unique memories
    let mut memory_map: HashMap<Uuid, &Memory> = HashMap::new();
    for (m, _) in semantic {
        memory_map.insert(m.id, m);
    }
    for (m, _) in fulltext {
        memory_map.entry(m.id).or_insert(m);
    }

    // Compute RRF + adjustments for each memory
    let mut scored: Vec<(Memory, f64)> = memory_map
        .iter()
        .map(|(id, memory)| {
            // Base RRF score
            let mut rrf = 0.0_f64;
            if let Some(&rank) = semantic_rank.get(id) {
                rrf += 1.0 / (RRF_K + rank as f64);
            }
            if let Some(&rank) = fulltext_rank.get(id) {
                rrf += 1.0 / (RRF_K + rank as f64);
            }

            // Time decay: halve relevance every 30 days since last access
            let age_days = (now - memory.accessed_at).num_days().max(0) as f64;
            let time_decay = 0.5_f64.powf(age_days / 30.0);

            // Access frequency boost: log(1 + count) / log(11), normalized to ~1.0 at 10 accesses
            let access_boost = (1.0 + memory.access_count as f64).ln() / 11.0_f64.ln();

            // Importance weight
            let importance = memory.importance as f64;

            // Final score
            let final_score = rrf * time_decay * (0.5 + 0.3 * importance + 0.2 * access_boost);

            ((*memory).clone(), final_score)
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Truncate to limit
    scored.truncate(limit);

    scored
}

/// Format memories with type labels and scores for injection into system prompt
fn format_memories(memories: &[(Memory, f64)]) -> String {
    if memories.is_empty() {
        return String::new();
    }

    let mut output = String::from("\n\n---\n\n## Relevant Memories\n\n");

    for (i, (memory, score)) in memories.iter().enumerate() {
        let type_label = match memory.memory_type.as_str() {
            "episodic" => "[event]",
            "procedural" => "[how-to]",
            _ => "[fact]",
        };

        output.push_str(&format!("{}. {} ", i + 1, type_label));

        if let Some(ref summary) = memory.summary {
            output.push_str(summary);
        } else {
            let content = if memory.content.len() > 200 {
                format!("{}...", &memory.content[..200])
            } else {
                memory.content.clone()
            };
            output.push_str(&content);
        }

        if !memory.tags.is_empty() {
            output.push_str(&format!(" [{}]", memory.tags.join(", ")));
        }

        output.push_str(&format!(" (relevance: {:.2})\n", score));
    }

    output
}

/// Simple format for cached results (no scores available)
fn format_memories_simple(memories: &[Memory]) -> String {
    if memories.is_empty() {
        return String::new();
    }

    let mut output = String::from("\n\n---\n\n## Relevant Memories\n\n");

    for (i, memory) in memories.iter().enumerate() {
        let type_label = match memory.memory_type.as_str() {
            "episodic" => "[event]",
            "procedural" => "[how-to]",
            _ => "[fact]",
        };

        output.push_str(&format!("{}. {} ", i + 1, type_label));

        if let Some(ref summary) = memory.summary {
            output.push_str(summary);
        } else {
            let content = if memory.content.len() > 200 {
                format!("{}...", &memory.content[..200])
            } else {
                memory.content.clone()
            };
            output.push_str(&content);
        }

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
    use crate::database::Memory;

    #[test]
    fn test_format_memories_empty() {
        assert_eq!(format_memories(&[]), "");
        assert_eq!(format_memories_simple(&[]), "");
    }

    #[test]
    fn test_format_memories_with_types() {
        let scored = vec![
            (
                Memory::new("user", "First memory")
                    .with_tags(vec!["tag1".into()])
                    .with_memory_type(MemoryType::Semantic),
                0.85,
            ),
            (
                Memory::new("user", "How to deploy")
                    .with_memory_type(MemoryType::Procedural),
                0.72,
            ),
            (
                Memory::new("user", "Conversation about Rust")
                    .with_memory_type(MemoryType::Episodic),
                0.65,
            ),
        ];

        let result = format_memories(&scored);
        assert!(result.contains("## Relevant Memories"));
        assert!(result.contains("[fact]"));
        assert!(result.contains("[how-to]"));
        assert!(result.contains("[event]"));
        assert!(result.contains("relevance: 0.85"));
    }

    #[test]
    fn test_rrf_scoring_basic() {
        let now = Utc::now();
        let m1 = Memory {
            id: Uuid::new_v4(),
            user_id: "user".into(),
            content: "memory one".into(),
            summary: None,
            importance: 0.8,
            tags: vec![],
            memory_type: "semantic".into(),
            metadata: serde_json::json!({}),
            source: "test".into(),
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 5,
        };
        let m2 = Memory {
            id: Uuid::new_v4(),
            user_id: "user".into(),
            content: "memory two".into(),
            summary: None,
            importance: 0.3,
            tags: vec![],
            memory_type: "semantic".into(),
            metadata: serde_json::json!({}),
            source: "test".into(),
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        };

        // m1 appears in both lists at rank 0, m2 only in semantic at rank 1
        let semantic = vec![(m1.clone(), 0.9), (m2.clone(), 0.7)];
        let fulltext = vec![(m1.clone(), 0.8)];

        let results = compute_rrf_scores(&semantic, &fulltext, 10);

        assert_eq!(results.len(), 2);
        // m1 should score higher (in both lists, higher importance, more accesses)
        assert_eq!(results[0].0.id, m1.id);
        assert!(results[0].1 > results[1].1);
    }

    #[test]
    fn test_rrf_time_decay() {
        let now = Utc::now();
        let old_time = now - chrono::Duration::days(60);

        let recent = Memory {
            id: Uuid::new_v4(),
            user_id: "user".into(),
            content: "recent".into(),
            summary: None,
            importance: 0.5,
            tags: vec![],
            memory_type: "semantic".into(),
            metadata: serde_json::json!({}),
            source: "test".into(),
            created_at: now,
            updated_at: now,
            accessed_at: now,
            access_count: 0,
        };
        let old = Memory {
            id: Uuid::new_v4(),
            user_id: "user".into(),
            content: "old".into(),
            summary: None,
            importance: 0.5,
            tags: vec![],
            memory_type: "semantic".into(),
            metadata: serde_json::json!({}),
            source: "test".into(),
            created_at: old_time,
            updated_at: old_time,
            accessed_at: old_time,
            access_count: 0,
        };

        // Both at same semantic rank but different ages
        let semantic = vec![(recent.clone(), 0.9), (old.clone(), 0.9)];
        let fulltext: Vec<(Memory, f32)> = vec![];

        let results = compute_rrf_scores(&semantic, &fulltext, 10);

        // Recent memory should score higher due to time decay
        assert_eq!(results[0].0.id, recent.id);
        assert!(results[0].1 > results[1].1);
    }
}
