//! Soul section storage and retrieval from PostgreSQL
//!
//! Persists the agent's SOUL.md as individual sections in the database.
//! Some sections are immutable (Identity, Core Values, Boundaries) while
//! others can evolve over time (Personality, Communication Style, etc.).

use crate::database::PostgresPool;
use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;
use uuid::Uuid;

/// Sections that cannot be updated after initialization
const IMMUTABLE_SECTIONS: &[&str] = &["Identity", "Core Values", "Boundaries"];

/// A soul section stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SoulSection {
    pub id: Uuid,
    pub section_name: String,
    pub section_order: i32,
    pub content: String,
    pub is_mutable: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Soul store backed by PostgreSQL
#[derive(Clone)]
pub struct SoulStore {
    pool: PostgresPool,
}

impl SoulStore {
    pub fn new(pool: PostgresPool) -> Self {
        Self { pool }
    }

    /// Check if the soul has been initialized (any rows exist)
    pub async fn is_initialized(&self) -> Result<bool> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agent_soul_sections",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0 > 0)
    }

    /// Initialize soul from SOUL.md content, parsing sections by `## ` headers.
    /// Called once on first startup.
    pub async fn initialize_from_content(&self, soul_md: &str) -> Result<()> {
        let sections = parse_soul_sections(soul_md);

        for (order, (name, body)) in sections.iter().enumerate() {
            let is_mutable = !IMMUTABLE_SECTIONS.contains(&name.as_str());

            sqlx::query(r#"
                INSERT INTO agent_soul_sections (section_name, section_order, content, is_mutable)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (section_name) DO NOTHING
            "#)
            .bind(name)
            .bind(order as i32)
            .bind(body)
            .bind(is_mutable)
            .execute(&self.pool)
            .await?;
        }

        let immutable_count = sections.iter().filter(|(n, _)| IMMUTABLE_SECTIONS.contains(&n.as_str())).count();
        info!(
            "Soul initialized with {} sections ({} immutable, {} mutable)",
            sections.len(),
            immutable_count,
            sections.len() - immutable_count,
        );

        Ok(())
    }

    /// Get all sections ordered by section_order
    pub async fn get_all_sections(&self) -> Result<Vec<SoulSection>> {
        let sections: Vec<SoulSection> = sqlx::query_as(
            "SELECT * FROM agent_soul_sections ORDER BY section_order ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(sections)
    }

    /// Get a single section by name
    pub async fn get_section(&self, name: &str) -> Result<Option<SoulSection>> {
        let section: Option<SoulSection> = sqlx::query_as(
            "SELECT * FROM agent_soul_sections WHERE section_name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(section)
    }

    /// Update a mutable section's content, incrementing version.
    /// Returns error if section is immutable or doesn't exist.
    pub async fn update_section(&self, name: &str, new_content: &str) -> Result<()> {
        let result = sqlx::query(r#"
            UPDATE agent_soul_sections
            SET content = $1, version = version + 1, updated_at = NOW()
            WHERE section_name = $2 AND is_mutable = TRUE
        "#)
        .bind(new_content)
        .bind(name)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::Internal(format!(
                "Cannot update section '{}': does not exist or is immutable",
                name
            )));
        }

        Ok(())
    }

    /// Reconstruct the full soul markdown from all sections
    pub async fn render_full_soul(&self) -> Result<String> {
        let sections = self.get_all_sections().await?;
        let mut parts = Vec::with_capacity(sections.len());

        for section in &sections {
            parts.push(format!("## {}\n\n{}", section.section_name, section.content));
        }

        Ok(parts.join("\n\n"))
    }
}

/// Parse SOUL.md content into (section_name, body) pairs by splitting on `## ` headers.
/// Text before the first `## ` header is stored as a "Preamble" section.
fn parse_soul_sections(content: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_body = String::new();

    for line in content.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            // Save previous section
            if let Some(name) = current_name.take() {
                sections.push((name, current_body.trim().to_string()));
            } else if !current_body.trim().is_empty() {
                // Text before first header
                sections.push(("Preamble".to_string(), current_body.trim().to_string()));
            }
            current_name = Some(header.trim().to_string());
            current_body = String::new();
        } else {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }

    // Save last section
    if let Some(name) = current_name {
        sections.push((name, current_body.trim().to_string()));
    } else if !current_body.trim().is_empty() {
        sections.push(("Preamble".to_string(), current_body.trim().to_string()));
    }

    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_soul_sections() {
        let content = r#"# Agent Soul

This is the preamble.

## Identity

**Name:** OpenAgent

## Core Values

1. Accuracy
2. Safety

## Personality Traits

- Helpful
- Curious
"#;
        let sections = parse_soul_sections(content);
        assert_eq!(sections.len(), 4); // Preamble + 3 sections
        assert_eq!(sections[0].0, "Preamble");
        assert!(sections[0].1.contains("Agent Soul"));
        assert_eq!(sections[1].0, "Identity");
        assert!(sections[1].1.contains("OpenAgent"));
        assert_eq!(sections[2].0, "Core Values");
        assert_eq!(sections[3].0, "Personality Traits");
    }

    #[test]
    fn test_immutable_sections() {
        assert!(!IMMUTABLE_SECTIONS.contains(&"Personality Traits"));
        assert!(IMMUTABLE_SECTIONS.contains(&"Identity"));
        assert!(IMMUTABLE_SECTIONS.contains(&"Core Values"));
        assert!(IMMUTABLE_SECTIONS.contains(&"Boundaries"));
    }
}
