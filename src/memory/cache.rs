//! In-process caching for embeddings and search results
//!
//! Uses moka async cache (Send + Sync, TTL-based eviction).
//! No external services required.

use crate::database::Memory;
use moka::future::Cache;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Duration;

/// Cache key helper: hash a string to u64
fn hash_key(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// In-process memory cache
#[derive(Clone)]
pub struct MemoryCache {
    /// Embedding cache: hash(text) -> Vec<f32>
    embeddings: Cache<u64, Vec<f32>>,
    /// Search result cache: hash(user_id + query) -> Vec<Memory>
    search_results: Cache<u64, Vec<Memory>>,
}

impl MemoryCache {
    /// Create a new cache with default settings
    pub fn new() -> Self {
        MemoryCache {
            embeddings: Cache::builder()
                .max_capacity(1000)
                .time_to_live(Duration::from_secs(30 * 60)) // 30 min TTL
                .build(),
            search_results: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(5 * 60)) // 5 min TTL
                .build(),
        }
    }

    /// Get a cached embedding
    pub async fn get_embedding(&self, text: &str) -> Option<Vec<f32>> {
        self.embeddings.get(&hash_key(text)).await
    }

    /// Store an embedding in cache
    pub async fn put_embedding(&self, text: &str, embedding: Vec<f32>) {
        self.embeddings.insert(hash_key(text), embedding).await;
    }

    /// Get cached search results
    pub async fn get_search_results(&self, user_id: &str, query: &str) -> Option<Vec<Memory>> {
        let key = format!("{}:{}", user_id, query);
        self.search_results.get(&hash_key(&key)).await
    }

    /// Store search results in cache
    pub async fn put_search_results(&self, user_id: &str, query: &str, results: Vec<Memory>) {
        let key = format!("{}:{}", user_id, query);
        self.search_results.insert(hash_key(&key), results).await;
    }

    /// Invalidate all search caches for a user (e.g., after saving new memory)
    pub async fn invalidate_user_search(&self, _user_id: &str) {
        // Moka doesn't support prefix-based invalidation, so we invalidate all search results.
        // With a 5-minute TTL this is acceptable — stale results expire quickly.
        self.search_results.invalidate_all();
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_cache() {
        let cache = MemoryCache::new();

        assert!(cache.get_embedding("hello").await.is_none());

        cache.put_embedding("hello", vec![0.1, 0.2, 0.3]).await;

        let result = cache.get_embedding("hello").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 3);
    }
}
