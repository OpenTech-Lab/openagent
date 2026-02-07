//! Agent status tracking (singleton row in PostgreSQL)
//!
//! Tracks whether the agent is ready or processing, with heartbeat
//! and scheduler run timestamps.

use crate::database::PostgresPool;
use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Agent operational state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Ready,
    Processing,
}

impl AgentState {
    pub fn as_str(&self) -> &str {
        match self {
            AgentState::Ready => "ready",
            AgentState::Processing => "processing",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "processing" => AgentState::Processing,
            _ => AgentState::Ready,
        }
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Agent status row (singleton, id=1)
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AgentStatusRow {
    pub id: i32,
    pub status: String,
    pub current_task_id: Option<Uuid>,
    pub last_heartbeat: DateTime<Utc>,
    pub last_scheduler_run: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl AgentStatusRow {
    pub fn state(&self) -> AgentState {
        AgentState::from_str(&self.status)
    }
}

/// Agent status store (singleton row)
#[derive(Clone)]
pub struct AgentStatusStore {
    pool: PostgresPool,
}

impl AgentStatusStore {
    pub fn new(pool: PostgresPool) -> Self {
        Self { pool }
    }

    /// Get current status row
    pub async fn get(&self) -> Result<AgentStatusRow> {
        let row: AgentStatusRow = sqlx::query_as(
            "SELECT * FROM agent_status WHERE id = 1",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Get parsed agent state
    pub async fn state(&self) -> Result<AgentState> {
        let row = self.get().await?;
        Ok(row.state())
    }

    /// Check if agent is ready (not processing)
    pub async fn is_ready(&self) -> Result<bool> {
        Ok(self.state().await? == AgentState::Ready)
    }

    /// Transition to processing state with a task reference.
    /// Uses atomic CAS — fails if already processing.
    pub async fn set_processing(&self, task_id: Uuid) -> Result<()> {
        let result = sqlx::query(r#"
            UPDATE agent_status
            SET status = 'processing', current_task_id = $1, updated_at = NOW(), last_heartbeat = NOW()
            WHERE id = 1 AND status = 'ready'
        "#)
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::Internal(
                "Agent is already processing a task".to_string(),
            ));
        }

        Ok(())
    }

    /// Transition back to ready state
    pub async fn set_ready(&self) -> Result<()> {
        sqlx::query(r#"
            UPDATE agent_status
            SET status = 'ready', current_task_id = NULL, updated_at = NOW(), last_heartbeat = NOW()
            WHERE id = 1
        "#)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update heartbeat timestamp
    pub async fn heartbeat(&self) -> Result<()> {
        sqlx::query(
            "UPDATE agent_status SET last_heartbeat = NOW() WHERE id = 1",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Record that the scheduler ran
    pub async fn record_scheduler_run(&self) -> Result<()> {
        sqlx::query(
            "UPDATE agent_status SET last_scheduler_run = NOW(), updated_at = NOW() WHERE id = 1",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
