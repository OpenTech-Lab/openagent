//! Database module - PostgreSQL + pgvector and OpenSearch integration
//!
//! Provides hybrid storage for:
//! - PostgreSQL with pgvector: Long-term semantic memory and structured data
//! - OpenSearch: Full-text search across conversation histories

mod opensearch;
mod postgres;
mod memory;

pub use opensearch::OpenSearchClient;
pub use postgres::{PostgresPool, init_pool, init_pool_for_migrations, migrations};
pub use memory::{Memory, MemoryStore};
