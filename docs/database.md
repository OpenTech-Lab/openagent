# Database Module

OpenAgent supports multiple database backends for storing conversations, memories, and vector embeddings. This document describes the database architecture.

## Overview

The database module provides:

| Backend | Purpose | Features |
|---------|---------|----------|
| **PostgreSQL** | Primary storage | Conversations, users, pgvector |
| **OpenSearch** | Search/Vector | Full-text search, kNN vectors |
| **SQLite** | Local storage | Development, offline mode |

## Architecture

```mermaid
graph TB
    subgraph "Storage Layer"
        SM[StorageManager]
    end

    subgraph "Backends"
        PG[PostgreSQL + pgvector]
        OS[OpenSearch]
        SQ[SQLite]
    end

    subgraph "Data Types"
        CONV[Conversations]
        MSG[Messages]
        MEM[Memories]
        VEC[Vectors]
    end

    SM --> PG
    SM --> OS
    SM --> SQ

    PG --> CONV
    PG --> MSG
    PG --> VEC
    OS --> MEM
    OS --> VEC
    SQ --> CONV
    SQ --> MSG
```

## PostgreSQL

PostgreSQL is the recommended production database with pgvector for embeddings.

### Configuration

```toml
[storage]
backend = "postgres"

[storage.postgres]
# Connection URL (use DATABASE_URL env var)
url = "postgres://user:password@localhost:5432/openagent"

# Connection pool
max_connections = 10
min_connections = 2
connect_timeout_secs = 30
idle_timeout_secs = 300

# pgvector settings
enable_pgvector = true
vector_dimensions = 1536

# SSL mode
ssl_mode = "prefer"  # disable, prefer, require
```

### Schema

```sql
-- Conversations table
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    channel_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    model VARCHAR(255),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'
);

-- Messages table
CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL,
    content TEXT NOT NULL,
    tool_calls JSONB,
    tool_results JSONB,
    model VARCHAR(255),
    tokens_used INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'
);

-- Vector memories (with pgvector)
CREATE TABLE memories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    embedding vector(1536),  -- pgvector extension
    importance FLOAT DEFAULT 0.5,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'
);

-- Create index for vector similarity search
CREATE INDEX ON memories USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);
```

### Usage

```rust
use openagent::database::{PostgresStorage, StorageBackend};
use openagent::config::PostgresConfig;

let config = PostgresConfig {
    url: "postgres://localhost/openagent".into(),
    max_connections: 10,
    enable_pgvector: true,
    ..Default::default()
};

let storage = PostgresStorage::connect(config).await?;

// Store a conversation
let conv = Conversation::new("telegram", "user123");
storage.save_conversation(&conv).await?;

// Add a message
let msg = Message::user("Hello!");
storage.add_message(conv.id, &msg).await?;

// Search similar memories
let query_embedding = embedding_model.embed("previous discussion").await?;
let memories = storage.search_similar(
    &query_embedding,
    5,  // limit
    0.7 // min similarity
).await?;
```

### Migrations

Migrations are managed with SQLx:

```bash
# Install SQLx CLI
cargo install sqlx-cli

# Create a new migration
sqlx migrate add create_conversations

# Run migrations
sqlx migrate run

# Revert last migration
sqlx migrate revert
```

## OpenSearch

OpenSearch provides full-text search and vector kNN capabilities.

### Configuration

```toml
[storage.opensearch]
# Connection
url = "http://localhost:9200"
username = "admin"
password = "admin"  # Use OPENSEARCH_PASSWORD env var

# Index settings
index_prefix = "openagent"
number_of_shards = 1
number_of_replicas = 0

# Vector settings
vector_dimensions = 1536
ef_construction = 512
m = 16  # HNSW M parameter
```

### Index Mappings

```json
{
  "mappings": {
    "properties": {
      "conversation_id": { "type": "keyword" },
      "content": { "type": "text", "analyzer": "standard" },
      "embedding": {
        "type": "knn_vector",
        "dimension": 1536,
        "method": {
          "name": "hnsw",
          "space_type": "cosinesimil",
          "engine": "nmslib",
          "parameters": {
            "ef_construction": 512,
            "m": 16
          }
        }
      },
      "importance": { "type": "float" },
      "created_at": { "type": "date" },
      "metadata": { "type": "object", "enabled": false }
    }
  }
}
```

### Usage

```rust
use openagent::database::{OpenSearchStorage, StorageBackend};

let storage = OpenSearchStorage::connect(config).await?;

// Full-text search
let results = storage.search_text(
    "machine learning",
    SearchOptions {
        limit: 10,
        conversation_id: Some(conv_id),
        ..Default::default()
    }
).await?;

// Vector similarity search
let results = storage.search_similar(
    &query_embedding,
    5,
    0.8
).await?;

// Hybrid search (text + vector)
let results = storage.search_hybrid(
    "neural networks",
    &query_embedding,
    HybridOptions {
        text_weight: 0.3,
        vector_weight: 0.7,
        limit: 10,
    }
).await?;
```

## SQLite

SQLite is used for local development and offline mode.

### Configuration

```toml
[storage]
backend = "sqlite"

[storage.sqlite]
# Database file path
path = "~/.local/state/openagent/db.sqlite"

# Journal mode
journal_mode = "wal"  # delete, truncate, persist, memory, wal, off

# Busy timeout (ms)
busy_timeout = 5000
```

### Usage

```rust
use openagent::database::{SqliteStorage, StorageBackend};

let storage = SqliteStorage::open("openagent.db").await?;

// Same interface as PostgreSQL
let conv = Conversation::new("cli", "local-user");
storage.save_conversation(&conv).await?;
```

## StorageBackend Trait

All backends implement the `StorageBackend` trait:

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get backend name
    fn name(&self) -> &str;

    /// Store a conversation
    async fn save_conversation(&self, conversation: &Conversation) -> Result<()>;
    
    /// Get a conversation by ID
    async fn get_conversation(&self, id: &Uuid) -> Result<Option<Conversation>>;
    
    /// List conversations for a user
    async fn list_conversations(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Conversation>>;
    
    /// Delete a conversation
    async fn delete_conversation(&self, id: &Uuid) -> Result<()>;

    /// Add a message to a conversation
    async fn add_message(&self, conversation_id: &Uuid, message: &Message) -> Result<()>;
    
    /// Get messages for a conversation
    async fn get_messages(
        &self,
        conversation_id: &Uuid,
        limit: usize,
    ) -> Result<Vec<Message>>;

    /// Store a memory with embedding
    async fn store_memory(&self, memory: &Memory) -> Result<()>;
    
    /// Search similar memories by vector
    async fn search_similar(
        &self,
        embedding: &[f32],
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<Memory>>;
}
```

## Memory Storage

Memories are vector-indexed for semantic retrieval:

```rust
use openagent::database::Memory;

let memory = Memory {
    id: Uuid::new_v4(),
    conversation_id: Some(conv.id),
    content: "User prefers detailed technical explanations".to_string(),
    embedding: embedding_model.embed(&content).await?,
    importance: 0.8,
    metadata: serde_json::json!({
        "type": "preference",
        "source": "inferred"
    }),
    created_at: Utc::now(),
};

storage.store_memory(&memory).await?;
```

### Memory Retrieval

```rust
// Get relevant memories for a query
let query = "How should I explain this concept?";
let query_embedding = embedding_model.embed(query).await?;

let memories = storage.search_similar(
    &query_embedding,
    5,      // top 5 results
    0.75    // minimum similarity threshold
).await?;

for memory in memories {
    println!("Memory: {} (score: {})", memory.content, memory.score);
}
```

## Connection Pooling

PostgreSQL connections are pooled for efficiency:

```rust
use openagent::database::PostgresPool;

let pool = PostgresPool::connect(&config).await?;

// Get connection from pool
let conn = pool.acquire().await?;

// Connection returned to pool when dropped
```

## Error Handling

```rust
use openagent::error::{Error, Result};
use openagent::database::StorageBackend;

async fn get_history(storage: &dyn StorageBackend, conv_id: Uuid) -> Result<Vec<Message>> {
    storage.get_messages(&conv_id, 100).await
        .map_err(|e| Error::Database(format!("Failed to get messages: {}", e)))
}
```

## Best Practices

### Development

```toml
[storage]
backend = "sqlite"

[storage.sqlite]
path = ":memory:"  # In-memory for testing
```

### Production

```toml
[storage]
backend = "postgres"

[storage.postgres]
max_connections = 20
min_connections = 5
enable_pgvector = true

[storage.opensearch]
url = "https://opensearch.example.com:9200"
```

## Next Steps

- [Configuration](./configuration.md) - Storage configuration
- [Core Traits](./core-traits.md) - StorageBackend trait
- [Architecture](./architecture.md) - System overview
