//! Database module - PostgreSQL + pgvector
//!
//! Provides storage for:
//! - PostgreSQL with pgvector: Long-term semantic memory and structured data
//! - PostgreSQL tsvector: Full-text search across memories

mod config_params;
mod postgres;
mod memory;
mod soul;
mod tasks;
mod agent_status;

pub use config_params::{ConfigParam, ConfigParamStore, ConfigValueType};
pub use postgres::{PostgresPool, init_pool, init_pool_for_migrations, migrations};
pub use memory::{Memory, MemoryStore, MemoryType};
pub use soul::{SoulSection, SoulStore};
pub use tasks::{AgentTask, TaskStatus, TaskStore};
pub use agent_status::{AgentState, AgentStatusRow, AgentStatusStore};
