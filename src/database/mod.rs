//! Database module - PostgreSQL + pgvector
//!
//! Provides storage for:
//! - PostgreSQL with pgvector: Long-term semantic memory and structured data
//! - PostgreSQL tsvector: Full-text search across memories

mod config_params;
mod postgres;
mod memory;

pub use config_params::{ConfigParam, ConfigParamStore, ConfigValueType};
pub use postgres::{PostgresPool, init_pool, init_pool_for_migrations, migrations};
pub use memory::{Memory, MemoryStore, MemoryType};
