//! PostgreSQL database connection and operations

use crate::config::DatabaseConfig;
use crate::error::{Error, Result};
use secrecy::ExposeSecret;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

/// PostgreSQL connection pool type alias
pub type PostgresPool = PgPool;

/// Initialize the PostgreSQL connection pool
pub async fn init_pool(config: &DatabaseConfig) -> Result<PostgresPool> {
    init_pool_with_options(config, true).await
}

/// Initialize the PostgreSQL connection pool without pgvector check
/// Use this for running migrations before pgvector is installed
pub async fn init_pool_for_migrations(config: &DatabaseConfig) -> Result<PostgresPool> {
    init_pool_with_options(config, false).await
}

/// Initialize the PostgreSQL connection pool with options
async fn init_pool_with_options(config: &DatabaseConfig, require_pgvector: bool) -> Result<PostgresPool> {
    info!("Initializing PostgreSQL connection pool");

    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
        .connect(config.url.expose_secret())
        .await?;

    // Verify connection and optionally check for required extensions
    verify_database(&pool, require_pgvector).await?;

    info!("PostgreSQL connection pool initialized successfully");
    Ok(pool)
}

/// Verify database connection and optionally check for required extensions
async fn verify_database(pool: &PgPool, require_pgvector: bool) -> Result<()> {
    // Check connection
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .map_err(|e| Error::Database(sqlx::Error::from(e)))?;

    if require_pgvector {
        // Check for pgvector extension
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT extname FROM pg_extension WHERE extname = 'vector'"
        )
        .fetch_optional(pool)
        .await?;

        if result.is_none() {
            return Err(Error::Database(sqlx::Error::Configuration(
                "pgvector extension is not installed. Run: CREATE EXTENSION vector;".into()
            )));
        }
    }

    Ok(())
}

/// Database migrations
pub mod migrations {
    use super::*;
    use tracing::warn;

    /// Run all migrations
    pub async fn run(pool: &PgPool) -> Result<()> {
        info!("Running database migrations");

        // Try to create pgvector extension (requires superuser or extension already available)
        match sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(pool)
            .await
        {
            Ok(_) => info!("pgvector extension enabled"),
            Err(e) => {
                warn!("Could not create pgvector extension: {}. Vector features may not work.", e);
                warn!("If you need vector support, run as superuser: CREATE EXTENSION vector;");
            }
        }

        // Create conversations table
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS conversations (
                id UUID PRIMARY KEY,
                user_id TEXT NOT NULL,
                model TEXT NOT NULL,
                system_prompt TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                total_tokens INTEGER NOT NULL DEFAULT 0
            )
        "#)
        .execute(pool)
        .await?;

        // Create messages table
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS messages (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_call_id TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                embedding vector(384)
            )
        "#)
        .execute(pool)
        .await?;

        // Create memories table for long-term storage
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS memories (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id TEXT NOT NULL,
                content TEXT NOT NULL,
                summary TEXT,
                embedding vector(384),
                importance REAL NOT NULL DEFAULT 0.5,
                tags TEXT[] NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                access_count INTEGER NOT NULL DEFAULT 0
            )
        "#)
        .execute(pool)
        .await?;

        // Create indexes (each must be a separate query for SQLx)
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_conversations_user_id ON conversations(user_id)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_conversation_id ON messages(conversation_id)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_user_id ON memories(user_id)")
            .execute(pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_tags ON memories USING GIN(tags)")
            .execute(pool)
            .await?;

        // Create vector similarity search indexes (using IVFFlat for better performance)
        sqlx::query(r#"
            CREATE INDEX IF NOT EXISTS idx_messages_embedding ON messages
            USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100)
        "#)
        .execute(pool)
        .await
        .ok(); // Ignore if not enough data or vector type not available

        sqlx::query(r#"
            CREATE INDEX IF NOT EXISTS idx_memories_embedding ON memories
            USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100)
        "#)
        .execute(pool)
        .await
        .ok(); // Ignore if already exists or not enough data

        // --- tsvector full-text search for memories ---

        // Add tsvector column
        sqlx::query(
            "ALTER TABLE memories ADD COLUMN IF NOT EXISTS search_vector TSVECTOR"
        )
        .execute(pool)
        .await?;

        // GIN index for fast full-text search
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memories_search_vector ON memories USING GIN(search_vector)"
        )
        .execute(pool)
        .await?;

        // Auto-update trigger: uses 'simple' config for CJK/Japanese support
        sqlx::query(r#"
            CREATE OR REPLACE FUNCTION memories_search_vector_update() RETURNS trigger AS $$
            BEGIN
              NEW.search_vector :=
                setweight(to_tsvector('simple', COALESCE(NEW.content, '')), 'A') ||
                setweight(to_tsvector('simple', COALESCE(NEW.summary, '')), 'B') ||
                setweight(to_tsvector('simple', COALESCE(array_to_string(NEW.tags, ' '), '')), 'C');
              RETURN NEW;
            END;
            $$ LANGUAGE plpgsql
        "#)
        .execute(pool)
        .await?;

        sqlx::query("DROP TRIGGER IF EXISTS memories_search_vector_trigger ON memories")
            .execute(pool)
            .await?;

        sqlx::query(r#"
            CREATE TRIGGER memories_search_vector_trigger
              BEFORE INSERT OR UPDATE ON memories
              FOR EACH ROW EXECUTE FUNCTION memories_search_vector_update()
        "#)
        .execute(pool)
        .await?;

        // Backfill existing rows that lack a search_vector
        sqlx::query(r#"
            UPDATE memories SET search_vector =
              setweight(to_tsvector('simple', COALESCE(content, '')), 'A') ||
              setweight(to_tsvector('simple', COALESCE(summary, '')), 'B') ||
              setweight(to_tsvector('simple', COALESCE(array_to_string(tags, ' '), '')), 'C')
            WHERE search_vector IS NULL
        "#)
        .execute(pool)
        .await
        .ok(); // Ignore if no rows

        info!("Database migrations completed");
        Ok(())
    }
}

/// Conversation repository operations
pub mod conversations {
    use super::*;
    use crate::agent::{Conversation, Message, Role};
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    /// Save a conversation to the database
    #[allow(dead_code)]
    pub async fn save(pool: &PgPool, conv: &Conversation) -> Result<()> {
        // Upsert conversation
        sqlx::query(r#"
            INSERT INTO conversations (id, user_id, model, system_prompt, created_at, updated_at, total_tokens)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                updated_at = EXCLUDED.updated_at,
                total_tokens = EXCLUDED.total_tokens
        "#)
        .bind(conv.id)
        .bind(&conv.user_id)
        .bind(&conv.model)
        .bind(&conv.system_prompt)
        .bind(conv.created_at)
        .bind(conv.updated_at)
        .bind(conv.total_tokens as i32)
        .execute(pool)
        .await?;

        // Delete existing messages and re-insert
        sqlx::query("DELETE FROM messages WHERE conversation_id = $1")
            .bind(conv.id)
            .execute(pool)
            .await?;

        for msg in &conv.messages {
            sqlx::query(r#"
                INSERT INTO messages (conversation_id, role, content, tool_call_id)
                VALUES ($1, $2, $3, $4)
            "#)
            .bind(conv.id)
            .bind(msg.role.to_string())
            .bind(&msg.content)
            .bind(&msg.tool_call_id)
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    /// Load a conversation by ID
    #[allow(dead_code)]
    pub async fn load(pool: &PgPool, id: Uuid) -> Result<Option<Conversation>> {
        #[derive(sqlx::FromRow)]
        struct ConvRow {
            id: Uuid,
            user_id: String,
            model: String,
            system_prompt: Option<String>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            total_tokens: i32,
        }

        let conv_row: Option<ConvRow> = sqlx::query_as(
            "SELECT * FROM conversations WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        let Some(row) = conv_row else {
            return Ok(None);
        };

        #[derive(sqlx::FromRow)]
        struct MsgRow {
            role: String,
            content: String,
            tool_call_id: Option<String>,
        }

        let msg_rows: Vec<MsgRow> = sqlx::query_as(
            "SELECT role, content, tool_call_id FROM messages WHERE conversation_id = $1 ORDER BY created_at"
        )
        .bind(id)
        .fetch_all(pool)
        .await?;

        let messages: Vec<Message> = msg_rows
            .into_iter()
            .map(|r| {
                let role = match r.role.as_str() {
                    "system" => Role::System,
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "tool" => Role::Tool,
                    _ => Role::User,
                };
                Message {
                    role,
                    content: r.content,
                    name: None,
                    tool_call_id: r.tool_call_id,
                    tool_calls: None,
                }
            })
            .collect();

        Ok(Some(Conversation {
            id: row.id,
            user_id: row.user_id,
            messages,
            system_prompt: row.system_prompt,
            created_at: row.created_at,
            updated_at: row.updated_at,
            model: row.model,
            total_tokens: row.total_tokens as u32,
        }))
    }

    /// Load the most recent conversation for a user
    #[allow(dead_code)]
    pub async fn load_latest(pool: &PgPool, user_id: &str) -> Result<Option<Conversation>> {
        let id: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM conversations WHERE user_id = $1 ORDER BY updated_at DESC LIMIT 1"
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

        match id {
            Some((id,)) => load(pool, id).await,
            None => Ok(None),
        }
    }

    /// Delete a conversation
    #[allow(dead_code)]
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM conversations WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Database tests would require a test database setup
}
