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

pub mod agentic_loop;
mod client;
mod conversation;
pub mod loop_guard;
pub mod prompts;
pub(crate) mod types;

pub mod rig_client;
pub mod tool_bridge;

pub use agentic_loop::{
    run_agentic_loop, AgentLoopInput, AgentLoopOutput, LoopCallback, LoopConfig, LoopOutcome,
    LoopTrace, NoOpCallback,
};
pub use client::OpenRouterClient;
pub use conversation::{Conversation, ConversationManager};
pub use loop_guard::LoopGuard;
pub use prompts::PromptTemplate;
pub use types::*;

pub use rig_client::RigLlmClient;
pub use tool_bridge::ToolRegistryRigExt;

// Re-export tools from the new location for backward compatibility
pub use crate::tools::{
    Tool, ToolCall, ToolResult, ToolRegistry,
    ReadFileTool, WriteFileTool, SystemCommandTool,
    DuckDuckGoSearchTool, BraveSearchTool, BraveSearchConfig,
    PerplexitySearchTool, PerplexityConfig, SearchResult,
    MemorySaveTool, MemorySearchTool, MemoryListTool, MemoryDeleteTool,
    TaskCreateTool, TaskListTool, TaskUpdateTool,
};
