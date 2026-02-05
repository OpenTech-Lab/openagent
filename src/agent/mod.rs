//! Agent module - LLM logic, prompt engineering, and OpenRouter client
//!
//! This module handles all AI-related functionality including:
//! - OpenRouter API client for multi-model LLM access
//! - Message handling and conversation management
//! - Prompt templates and engineering
//! - Tool/function calling support

mod client;
mod conversation;
pub mod prompts;
mod tools;
mod types;

pub use client::OpenRouterClient;
pub use conversation::{Conversation, ConversationManager};
pub use prompts::PromptTemplate;
pub use tools::{Tool, ToolCall, ToolResult, ToolRegistry, ReadFileTool, WriteFileTool};
pub use types::*;
