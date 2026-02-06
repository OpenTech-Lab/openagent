//! Agent task storage and lifecycle management
//!
//! Tracks tasks created from user requests with status lifecycle:
//! pending → processing → finish/fail/cancel/stop

use crate::database::PostgresPool;
use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Task status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Processing,
    Finish,
    Fail,
    Cancel,
    Stop,
}

impl TaskStatus {
    pub fn as_str(&self) -> &str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Processing => "processing",
            TaskStatus::Finish => "finish",
            TaskStatus::Fail => "fail",
            TaskStatus::Cancel => "cancel",
            TaskStatus::Stop => "stop",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "processing" => TaskStatus::Processing,
            "finish" => TaskStatus::Finish,
            "fail" => TaskStatus::Fail,
            "cancel" => TaskStatus::Cancel,
            "stop" => TaskStatus::Stop,
            _ => TaskStatus::Pending,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Finish | TaskStatus::Fail | TaskStatus::Cancel | TaskStatus::Stop
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An agent task
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AgentTask {
    pub id: Uuid,
    pub user_id: String,
    pub chat_id: Option<i64>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: i32,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl AgentTask {
    pub fn status_enum(&self) -> TaskStatus {
        TaskStatus::from_str(&self.status)
    }
}

/// Task store backed by PostgreSQL
#[derive(Clone)]
pub struct TaskStore {
    pool: PostgresPool,
}

impl TaskStore {
    pub fn new(pool: PostgresPool) -> Self {
        Self { pool }
    }

    /// Create a new task
    pub async fn create(
        &self,
        user_id: &str,
        chat_id: Option<i64>,
        title: &str,
        description: &str,
        priority: i32,
    ) -> Result<AgentTask> {
        let task: AgentTask = sqlx::query_as(r#"
            INSERT INTO agent_tasks (user_id, chat_id, title, description, priority)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
        "#)
        .bind(user_id)
        .bind(chat_id)
        .bind(title)
        .bind(description)
        .bind(priority)
        .fetch_one(&self.pool)
        .await?;
        Ok(task)
    }

    /// Get a task by ID
    pub async fn get(&self, id: Uuid) -> Result<Option<AgentTask>> {
        let task: Option<AgentTask> = sqlx::query_as(
            "SELECT * FROM agent_tasks WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(task)
    }

    /// Get the next pending task (ordered by priority desc, created_at asc).
    /// Uses FOR UPDATE SKIP LOCKED for safe concurrent access.
    pub async fn next_pending(&self) -> Result<Option<AgentTask>> {
        let task: Option<AgentTask> = sqlx::query_as(r#"
            SELECT * FROM agent_tasks
            WHERE status = 'pending'
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        "#)
        .fetch_optional(&self.pool)
        .await?;
        Ok(task)
    }

    /// Transition a task to processing
    pub async fn start_processing(&self, id: Uuid) -> Result<()> {
        sqlx::query(r#"
            UPDATE agent_tasks
            SET status = 'processing', started_at = NOW(), updated_at = NOW()
            WHERE id = $1
        "#)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a task as finished with optional result
    pub async fn finish(&self, id: Uuid, result: Option<&str>) -> Result<()> {
        sqlx::query(r#"
            UPDATE agent_tasks
            SET status = 'finish', result = $2, completed_at = NOW(), updated_at = NOW()
            WHERE id = $1
        "#)
        .bind(id)
        .bind(result)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a task as failed with error message
    pub async fn fail(&self, id: Uuid, error: &str) -> Result<()> {
        sqlx::query(r#"
            UPDATE agent_tasks
            SET status = 'fail', error_message = $2, completed_at = NOW(), updated_at = NOW()
            WHERE id = $1
        "#)
        .bind(id)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Cancel a task
    pub async fn cancel(&self, id: Uuid) -> Result<()> {
        sqlx::query(r#"
            UPDATE agent_tasks
            SET status = 'cancel', completed_at = NOW(), updated_at = NOW()
            WHERE id = $1
        "#)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Stop a task
    pub async fn stop(&self, id: Uuid) -> Result<()> {
        sqlx::query(r#"
            UPDATE agent_tasks
            SET status = 'stop', completed_at = NOW(), updated_at = NOW()
            WHERE id = $1
        "#)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get tasks by user_id with optional status filter
    pub async fn get_by_user(
        &self,
        user_id: &str,
        status: Option<TaskStatus>,
        limit: i64,
    ) -> Result<Vec<AgentTask>> {
        let tasks: Vec<AgentTask> = sqlx::query_as(r#"
            SELECT * FROM agent_tasks
            WHERE user_id = $1
              AND ($2::text IS NULL OR status = $2)
            ORDER BY created_at DESC
            LIMIT $3
        "#)
        .bind(user_id)
        .bind(status.map(|s| s.as_str().to_string()))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(tasks)
    }

    /// Count pending tasks
    pub async fn count_pending(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agent_tasks WHERE status = 'pending'",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }
}
