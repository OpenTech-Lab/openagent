//! Conversation summarizer for automatic episodic memory creation
//!
//! Uses the LLM to summarize conversations into structured episodic memories,
//! extracting key facts and user preferences for separate semantic storage.

use crate::agent::{GenerationOptions, Message, OpenRouterClient, Role};
use crate::error::Result;
use tracing::{info, warn};

/// Summary of a conversation for episodic memory storage
#[derive(Debug, Clone)]
pub struct EpisodicSummary {
    /// One-paragraph summary of the conversation
    pub summary: String,
    /// Key facts the user shared
    pub key_facts: Vec<String>,
    /// User preferences detected
    pub user_preferences: Vec<String>,
    /// Main topics discussed
    pub topics: Vec<String>,
}

/// Summarizes conversations into episodic memories
#[derive(Clone)]
pub struct ConversationSummarizer {
    llm_client: OpenRouterClient,
}

impl ConversationSummarizer {
    /// Create a new conversation summarizer
    pub fn new(llm_client: OpenRouterClient) -> Self {
        ConversationSummarizer { llm_client }
    }

    /// Summarize a conversation into an episodic summary
    pub async fn summarize(&self, messages: &[Message]) -> Result<EpisodicSummary> {
        // Filter to user + assistant messages only (skip system, tool)
        let conversation_text: String = messages
            .iter()
            .filter(|m| m.role == Role::User || m.role == Role::Assistant)
            .map(|m| {
                let role_label = match m.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    _ => "Other",
                };
                format!("{}: {}", role_label, m.content)
            })
            .collect::<Vec<_>>()
            .join("\n");

        if conversation_text.is_empty() {
            return Ok(EpisodicSummary {
                summary: String::new(),
                key_facts: vec![],
                user_preferences: vec![],
                topics: vec![],
            });
        }

        // Truncate if too long (keep first ~4000 chars to fit in context)
        let truncated = if conversation_text.len() > 4000 {
            format!("{}...\n[truncated]", &conversation_text[..4000])
        } else {
            conversation_text
        };

        let prompt = format!(
            r#"Summarize this conversation concisely. Extract key information.

Conversation:
---
{}
---

Respond ONLY with valid JSON in this exact format (no markdown, no code blocks):
{{"summary": "brief 1-2 sentence summary", "key_facts": ["fact1", "fact2"], "user_preferences": ["pref1"], "topics": ["topic1", "topic2"]}}

If a field has no items, use an empty array []. Keep the summary under 200 characters."#,
            truncated
        );

        let response = self
            .llm_client
            .chat(
                vec![Message::user(prompt)],
                GenerationOptions::precise(),
            )
            .await?;

        let content = response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        // Parse the JSON response
        match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(json) => {
                let summary = json
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let key_facts = extract_string_array(&json, "key_facts");
                let user_preferences = extract_string_array(&json, "user_preferences");
                let topics = extract_string_array(&json, "topics");

                info!(
                    "Conversation summarized: {} facts, {} preferences, {} topics",
                    key_facts.len(),
                    user_preferences.len(),
                    topics.len()
                );

                Ok(EpisodicSummary {
                    summary,
                    key_facts,
                    user_preferences,
                    topics,
                })
            }
            Err(e) => {
                warn!("Failed to parse summarization response as JSON: {}. Raw: {}", e, &content[..content.len().min(200)]);
                // Fallback: use the raw content as summary
                Ok(EpisodicSummary {
                    summary: content[..content.len().min(200)].to_string(),
                    key_facts: vec![],
                    user_preferences: vec![],
                    topics: vec![],
                })
            }
        }
    }
}

/// Extract a string array from a JSON value by key
fn extract_string_array(json: &serde_json::Value, key: &str) -> Vec<String> {
    json.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_string_array() {
        let json = serde_json::json!({
            "topics": ["rust", "memory", ""],
            "empty": [],
            "not_array": "hello"
        });

        let topics = extract_string_array(&json, "topics");
        assert_eq!(topics, vec!["rust", "memory"]); // empty string filtered

        let empty = extract_string_array(&json, "empty");
        assert!(empty.is_empty());

        let missing = extract_string_array(&json, "missing");
        assert!(missing.is_empty());

        let not_array = extract_string_array(&json, "not_array");
        assert!(not_array.is_empty());
    }

    #[test]
    fn test_episodic_summary_default() {
        let summary = EpisodicSummary {
            summary: "Test conversation about Rust".into(),
            key_facts: vec!["User likes Rust".into()],
            user_preferences: vec!["Prefers Rust over Python".into()],
            topics: vec!["programming".into(), "rust".into()],
        };

        assert_eq!(summary.key_facts.len(), 1);
        assert_eq!(summary.topics.len(), 2);
    }
}
