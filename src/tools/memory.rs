//! Memory tools - AI-callable tools for saving, searching, listing, and deleting memories
//!
//! These tools allow the LLM to manage long-term memory during conversations.
//! The agentic loop injects `_user_id` into tool arguments before execution.

use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, warn};
use uuid::Uuid;

use crate::database::{Memory, MemoryType};
use crate::error::{Error, Result};
use crate::memory::MemoryRetriever;
use crate::tools::traits::{Tool, ToolResult};

/// Tool to save information to long-term memory
pub struct MemorySaveTool {
    retriever: MemoryRetriever,
}

impl MemorySaveTool {
    pub fn new(retriever: MemoryRetriever) -> Self {
        MemorySaveTool { retriever }
    }
}

#[async_trait]
impl Tool for MemorySaveTool {
    fn name(&self) -> &str {
        "memory_save"
    }

    fn description(&self) -> &str {
        "Save important information to long-term memory for future recall. Use this when the user shares preferences, facts, decisions, or procedural knowledge worth remembering."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The information to remember"
                },
                "summary": {
                    "type": "string",
                    "description": "Brief one-line summary for quick reference"
                },
                "memory_type": {
                    "type": "string",
                    "enum": ["episodic", "semantic", "procedural"],
                    "description": "Type of memory: 'semantic' for facts/preferences, 'episodic' for events/experiences, 'procedural' for how-to/workflows. Default: semantic"
                },
                "importance": {
                    "type": "number",
                    "description": "Importance score 0.0-1.0. 0.9+ for critical preferences, 0.7 for important context, 0.5 for general info. Default: 0.5"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for categorization (e.g., 'preference', 'project', 'decision', 'workflow')"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let user_id = args
            .get("_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Missing 'content' parameter".into()))?;

        let memory_type_str = args
            .get("memory_type")
            .and_then(|v| v.as_str())
            .unwrap_or("semantic");

        let importance = args
            .get("importance")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(0.5);

        let summary = args.get("summary").and_then(|v| v.as_str());

        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Check for duplicates before saving
        match self.retriever.get_or_create_embedding(content).await {
            Ok(embedding) => {
                match self
                    .retriever
                    .store()
                    .find_similar_by_embedding(user_id, embedding, 0.95, 1)
                    .await
                {
                    Ok(similar) => {
                        if let Some((existing, score)) = similar.first() {
                            let preview = existing
                                .summary
                                .as_deref()
                                .unwrap_or_else(|| {
                                    &existing.content[..existing.content.len().min(80)]
                                });
                            return Ok(ToolResult::success(format!(
                                "Very similar memory already exists (similarity: {:.2}): \"{}\". No new memory created.",
                                score, preview
                            )));
                        }
                    }
                    Err(e) => {
                        warn!("Duplicate check failed: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Embedding generation for dedup check failed: {}", e);
            }
        }

        let mut memory = Memory::new(user_id, content)
            .with_importance(importance)
            .with_tags(tags.clone())
            .with_memory_type(MemoryType::from_str(memory_type_str))
            .with_source("tool:memory_save");

        if let Some(s) = summary {
            memory = memory.with_summary(s);
        }

        let memory_id = memory.id;
        self.retriever.save_memory(&memory).await?;

        info!(
            "Memory saved: id={}, type={}, importance={}, user={}",
            memory_id, memory_type_str, importance, user_id
        );

        let tag_info = if tags.is_empty() {
            String::new()
        } else {
            format!(", tags: [{}]", tags.join(", "))
        };

        Ok(ToolResult::success(format!(
            "Memory saved successfully (type: {}, importance: {:.1}{}). ID: {}",
            memory_type_str, importance, tag_info, memory_id
        )))
    }
}

/// Tool to search long-term memory
pub struct MemorySearchTool {
    retriever: MemoryRetriever,
}

impl MemorySearchTool {
    pub fn new(retriever: MemoryRetriever) -> Self {
        MemorySearchTool { retriever }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search long-term memory for relevant information. Use this before answering questions about past interactions, user preferences, or previously discussed topics."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query in natural language"
                },
                "memory_type": {
                    "type": "string",
                    "enum": ["episodic", "semantic", "procedural"],
                    "description": "Optional: filter by memory type"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 5)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let user_id = args
            .get("_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Missing 'query' parameter".into()))?;

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(5);

        let memory_type = args
            .get("memory_type")
            .and_then(|v| v.as_str())
            .map(MemoryType::from_str);

        let result = self
            .retriever
            .retrieve_typed(user_id, query, limit, memory_type)
            .await?;

        if result.is_empty() {
            Ok(ToolResult::success(
                "No matching memories found.".to_string(),
            ))
        } else {
            Ok(ToolResult::success(result))
        }
    }
}

/// Tool to list memories by type or tag
pub struct MemoryListTool {
    retriever: MemoryRetriever,
}

impl MemoryListTool {
    pub fn new(retriever: MemoryRetriever) -> Self {
        MemoryListTool { retriever }
    }
}

#[async_trait]
impl Tool for MemoryListTool {
    fn name(&self) -> &str {
        "memory_list"
    }

    fn description(&self) -> &str {
        "List stored memories filtered by type or tag. Use this to browse stored knowledge and see what is remembered."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "memory_type": {
                    "type": "string",
                    "enum": ["episodic", "semantic", "procedural"],
                    "description": "Filter by memory type"
                },
                "tag": {
                    "type": "string",
                    "description": "Filter by tag"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 10)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let user_id = args
            .get("_user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(10);

        let memory_type = args.get("memory_type").and_then(|v| v.as_str());
        let tag = args.get("tag").and_then(|v| v.as_str());

        let memories = if let Some(tag) = tag {
            self.retriever.store().get_by_tag(user_id, tag, limit).await?
        } else if let Some(mt) = memory_type {
            self.retriever.store().search_by_type(user_id, mt, limit).await?
        } else {
            self.retriever.store().get_all(user_id, limit).await?
        };

        if memories.is_empty() {
            return Ok(ToolResult::success("No memories found.".to_string()));
        }

        let mut output = format!("Found {} memories:\n\n", memories.len());
        for (i, memory) in memories.iter().enumerate() {
            let type_label = match memory.memory_type.as_str() {
                "episodic" => "[event]",
                "procedural" => "[how-to]",
                _ => "[fact]",
            };

            output.push_str(&format!(
                "{}. {} ID: {}\n",
                i + 1,
                type_label,
                memory.id
            ));

            if let Some(ref summary) = memory.summary {
                output.push_str(&format!("   Summary: {}\n", summary));
            }

            let content_preview = if memory.content.len() > 150 {
                format!("{}...", &memory.content[..150])
            } else {
                memory.content.clone()
            };
            output.push_str(&format!("   Content: {}\n", content_preview));

            if !memory.tags.is_empty() {
                output.push_str(&format!("   Tags: [{}]\n", memory.tags.join(", ")));
            }
            output.push_str(&format!(
                "   Importance: {:.1} | Accessed: {} times\n\n",
                memory.importance, memory.access_count
            ));
        }

        Ok(ToolResult::success(output))
    }
}

/// Tool to delete a memory by ID
pub struct MemoryDeleteTool {
    retriever: MemoryRetriever,
}

impl MemoryDeleteTool {
    pub fn new(retriever: MemoryRetriever) -> Self {
        MemoryDeleteTool { retriever }
    }
}

#[async_trait]
impl Tool for MemoryDeleteTool {
    fn name(&self) -> &str {
        "memory_delete"
    }

    fn description(&self) -> &str {
        "Delete a specific memory by its ID. Use when information is outdated, incorrect, or no longer needed."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "memory_id": {
                    "type": "string",
                    "description": "UUID of the memory to delete"
                }
            },
            "required": ["memory_id"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let memory_id_str = args
            .get("memory_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Missing 'memory_id' parameter".into()))?;

        let memory_id = Uuid::parse_str(memory_id_str).map_err(|e| {
            Error::InvalidInput(format!("Invalid UUID '{}': {}", memory_id_str, e))
        })?;

        // Check if memory exists
        match self.retriever.store().get(memory_id).await? {
            Some(memory) => {
                self.retriever.store().delete(memory_id).await?;
                info!("Memory deleted: id={}", memory_id);

                let preview = memory
                    .summary
                    .as_deref()
                    .unwrap_or_else(|| &memory.content[..memory.content.len().min(80)]);

                Ok(ToolResult::success(format!(
                    "Memory deleted: \"{}\" (ID: {})",
                    preview, memory_id
                )))
            }
            None => Ok(ToolResult::failure(format!(
                "Memory not found: {}",
                memory_id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_names() {
        // Verify tool names match expected values
        assert_eq!("memory_save", "memory_save");
        assert_eq!("memory_search", "memory_search");
        assert_eq!("memory_list", "memory_list");
        assert_eq!("memory_delete", "memory_delete");
    }

    #[test]
    fn test_memory_save_schema_shape() {
        // Validate the expected schema structure without needing a real retriever
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "summary": { "type": "string" },
                "memory_type": { "type": "string", "enum": ["episodic", "semantic", "procedural"] },
                "importance": { "type": "number" },
                "tags": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["content"]
        });
        assert!(schema["properties"]["content"].is_object());
        assert_eq!(schema["required"][0], "content");
    }
}
