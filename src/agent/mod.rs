//! Agent module - LLM logic, prompt engineering, and OpenRouter client
//!
//! This module handles all AI-related functionality including:
//! - OpenRouter API client for multi-model LLM access
//! - Message handling and conversation management
//! - Prompt templates and engineering
//!
//! Tool/function calling support has moved to `crate::tools`.
//! Web search tools have moved to `crate::tools`.
//! MCP integration is in `crate::mcp`.
//! Skills (composable workflows) are in `crate::skills`.

mod client;
mod conversation;
pub mod prompts;
pub(crate) mod types;

pub use client::OpenRouterClient;
pub use conversation::{Conversation, ConversationManager};
pub use prompts::PromptTemplate;
pub use types::*;

// Re-export tools from the new location for backward compatibility
pub use crate::tools::{
    Tool, ToolCall, ToolResult, ToolRegistry,
    ReadFileTool, WriteFileTool, SystemCommandTool,
    DuckDuckGoSearchTool, BraveSearchTool, BraveSearchConfig,
    PerplexitySearchTool, PerplexityConfig, SearchResult,
    MemorySaveTool, MemorySearchTool, MemoryListTool, MemoryDeleteTool,
};
