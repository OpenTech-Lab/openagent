//! # OpenAgent
//!
//! A high-performance, low-latency, and secure AI agent framework built with Rust.
//!
//! ## Architecture
//!
//! OpenAgent follows a modular, loosely-coupled architecture inspired by openclaw:
//!
//! - **Core traits** (`core`): Abstract interfaces for providers, channels, storage, and execution
//! - **Configuration** (`config`): Modular configuration with focused type modules
//! - **Agent** (`agent`): LLM interaction, conversation management, and tool calling
//! - **Channels** (`channels`): Messaging platform integrations (Telegram, Discord, etc.)
//! - **Providers** (`providers`): LLM backend implementations (OpenRouter, Anthropic, etc.)
//! - **Storage** (`database`): Persistence backends (PostgreSQL, SQLite)
//! - **Sandbox** (`sandbox`): Secure code execution environments (OS, Wasm, Container)
//! - **Gateway** (`gateway`): WebSocket-based control plane
//!
//! ## Design Principles
//!
//! 1. **Trait-based abstraction**: All major components implement traits for loose coupling
//! 2. **Modular configuration**: Split into focused modules (provider, channel, storage, sandbox)
//! 3. **Plugin architecture**: Easy to add new providers, channels, and tools
//! 4. **Security first**: Multi-tier sandboxing, rate limiting, and access control
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use openagent::config::load_config;
//! use openagent::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Load configuration from file or environment
//!     let config = load_config()?;
//!     
//!     // Your agent code here...
//!     Ok(())
//! }
//! ```

// Core abstractions (traits and fundamental types)
pub mod core;

// Agent logic and LLM interaction
pub mod agent;

// Modular configuration (now a directory module)
#[path = "config/mod.rs"]
pub mod config;

// Database and storage backends
pub mod database;

// Memory: embedding generation, caching, and retrieval
pub mod memory;

// Error types
pub mod error;

// Secure execution sandboxes
pub mod sandbox;

// Gateway WebSocket protocol (control plane)
#[path = "gateway/mod.rs"]
pub mod gateway;

// Plugin SDK for external integrations
pub mod plugin_sdk;

// Re-export commonly used items
pub use error::{Error, Result};

// Re-export core traits for convenience
pub use core::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelPlugin,
    CodeExecutor, ExecutionRequest, ExecutionResult, Language,
    LlmProvider, LlmResponse, StreamingChunk,
    MemoryBackend, SearchBackend, StorageBackend,
    Message, Role,
};

// Re-export plugin SDK items
pub use plugin_sdk::{Plugin, PluginManifest, PluginRegistry};

/// Application version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application name
pub const NAME: &str = env!("CARGO_PKG_NAME");
