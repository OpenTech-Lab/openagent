//! Conversation management

use crate::agent::types::{Message, Role};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A conversation session
#[derive(Debug, Clone)]
pub struct Conversation {
    /// Unique conversation ID
    pub id: Uuid,
    /// User ID (e.g., Telegram user ID)
    pub user_id: String,
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// System prompt for this conversation
    pub system_prompt: Option<String>,
    /// When the conversation started
    pub created_at: DateTime<Utc>,
    /// When the conversation was last updated
    pub updated_at: DateTime<Utc>,
    /// Model being used
    pub model: String,
    /// Total tokens used
    pub total_tokens: u32,
}

impl Conversation {
    /// Create a new conversation
    pub fn new(user_id: impl Into<String>, model: impl Into<String>) -> Self {
        let now = Utc::now();
        Conversation {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            messages: Vec::new(),
            system_prompt: None,
            created_at: now,
            updated_at: now,
            model: model.into(),
            total_tokens: 0,
        }
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::user(content));
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::assistant(content));
    }

    /// Get messages formatted for API request (includes system prompt)
    pub fn get_api_messages(&self) -> Vec<Message> {
        let mut messages = Vec::with_capacity(self.messages.len() + 1);

        if let Some(ref system) = self.system_prompt {
            messages.push(Message::system(system));
        }

        messages.extend(self.messages.clone());
        messages
    }

    /// Get the last N messages
    pub fn get_recent_messages(&self, n: usize) -> Vec<Message> {
        let start = self.messages.len().saturating_sub(n);
        self.messages[start..].to_vec()
    }

    /// Truncate conversation to fit within token limit
    /// Keeps system prompt and most recent messages
    pub fn truncate_to_tokens(&mut self, max_tokens: u32) {
        // Simple approximation: ~4 chars per token
        let max_chars = (max_tokens * 4) as usize;

        let mut total_chars: usize = self
            .system_prompt
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0);

        // Start from the end and keep messages that fit
        let mut keep_from = self.messages.len();
        for (i, msg) in self.messages.iter().enumerate().rev() {
            let msg_chars = msg.content.len();
            if total_chars + msg_chars > max_chars {
                keep_from = i + 1;
                break;
            }
            total_chars += msg_chars;
        }

        if keep_from > 0 {
            self.messages = self.messages[keep_from..].to_vec();
        }
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
        self.updated_at = Utc::now();
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Check if conversation is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get the last assistant message, if any
    pub fn last_assistant_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
    }
}

/// Manages multiple conversations
pub struct ConversationManager {
    /// Active conversations by user ID
    conversations: std::collections::HashMap<String, Conversation>,
    /// Default model for new conversations
    default_model: String,
    /// Default system prompt
    default_system_prompt: Option<String>,
}

impl ConversationManager {
    /// Create a new conversation manager
    pub fn new(default_model: impl Into<String>) -> Self {
        ConversationManager {
            conversations: std::collections::HashMap::new(),
            default_model: default_model.into(),
            default_system_prompt: None,
        }
    }

    /// Set the default system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.default_system_prompt = Some(prompt.into());
        self
    }

    /// Get or create a conversation for a user
    pub fn get_or_create(&mut self, user_id: &str) -> &mut Conversation {
        if !self.conversations.contains_key(user_id) {
            let mut conv = Conversation::new(user_id, &self.default_model);
            if let Some(ref prompt) = self.default_system_prompt {
                conv = conv.with_system_prompt(prompt);
            }
            self.conversations.insert(user_id.to_string(), conv);
        }
        self.conversations.get_mut(user_id).unwrap()
    }

    /// Get a conversation by user ID
    pub fn get(&self, user_id: &str) -> Option<&Conversation> {
        self.conversations.get(user_id)
    }

    /// Get a mutable conversation by user ID
    pub fn get_mut(&mut self, user_id: &str) -> Option<&mut Conversation> {
        self.conversations.get_mut(user_id)
    }

    /// Remove a conversation
    pub fn remove(&mut self, user_id: &str) -> Option<Conversation> {
        self.conversations.remove(user_id)
    }

    /// Clear a user's conversation (but keep the entry)
    pub fn clear_conversation(&mut self, user_id: &str) {
        if let Some(conv) = self.conversations.get_mut(user_id) {
            conv.clear();
        }
    }

    /// Get all active conversation IDs
    pub fn active_users(&self) -> Vec<&str> {
        self.conversations.keys().map(|s| s.as_str()).collect()
    }

    /// Count active conversations
    pub fn conversation_count(&self) -> usize {
        self.conversations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_creation() {
        let conv = Conversation::new("user123", "gpt-4");
        assert_eq!(conv.user_id, "user123");
        assert_eq!(conv.model, "gpt-4");
        assert!(conv.is_empty());
    }

    #[test]
    fn test_add_messages() {
        let mut conv = Conversation::new("user123", "gpt-4");
        conv.add_user_message("Hello");
        conv.add_assistant_message("Hi there!");

        assert_eq!(conv.message_count(), 2);
        assert_eq!(conv.messages[0].role, Role::User);
        assert_eq!(conv.messages[1].role, Role::Assistant);
    }

    #[test]
    fn test_api_messages_with_system() {
        let conv = Conversation::new("user123", "gpt-4")
            .with_system_prompt("You are a helpful assistant.");

        let api_messages = conv.get_api_messages();
        assert_eq!(api_messages.len(), 1);
        assert_eq!(api_messages[0].role, Role::System);
    }

    #[test]
    fn test_conversation_manager() {
        let mut manager = ConversationManager::new("gpt-4")
            .with_system_prompt("Test system prompt");

        let conv = manager.get_or_create("user1");
        conv.add_user_message("Hello");

        assert!(manager.get("user1").is_some());
        assert!(manager.get("user2").is_none());
        assert_eq!(manager.conversation_count(), 1);
    }
}
