//! Core module - Fundamental traits and types for OpenAgent
//!
//! This module defines the core abstractions that enable loose coupling:
//! - Provider traits for LLM backends
//! - Channel traits for messaging platforms
//! - Storage traits for persistence backends
//! - Executor traits for code execution
//!
//! Following the modular architecture pattern from openclaw, all integrations
//! implement these traits to ensure consistent behavior and easy extensibility.

pub mod channel;
pub mod executor;
pub mod provider;
pub mod storage;
pub mod types;

// Re-export core traits for convenient access
pub use channel::{Channel, ChannelCapabilities, ChannelMessage, ChannelPlugin, ChannelReply};
pub use executor::{CodeExecutor, ExecutionRequest, ExecutionResult, Language};
pub use provider::{GenerationOptions, LlmProvider, LlmResponse, StreamingChunk};
pub use storage::{MemoryBackend, SearchBackend, StorageBackend};
pub use types::*;
