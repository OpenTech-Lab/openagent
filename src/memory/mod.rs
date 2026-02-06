//! Memory module - embedding generation, caching, and retrieval
//!
//! Orchestrates local embeddings (fastembed), in-process caching (moka),
//! and PostgreSQL-backed semantic + full-text search.

pub mod cache;
pub mod embedding;
pub mod retrieval;

pub use cache::MemoryCache;
pub use embedding::EmbeddingService;
pub use retrieval::MemoryRetriever;
