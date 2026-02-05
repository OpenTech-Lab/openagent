//! # OpenAgent
//!
//! A high-performance, low-latency, and secure AI agent framework built with Rust.
//!
//! ## Features
//!
//! - **Ultra-Low Latency:** Engineered in Rust for near-zero runtime overhead
//! - **OpenRouter Integration:** Unified access to any LLM via a single API key
//! - **Hybrid Memory Engine:** PostgreSQL + pgvector and OpenSearch
//! - **Telegram Native:** First-class Telegram Bot API support
//! - **Multi-Tier Sandboxing:** OS, Wasm, or Container execution environments

pub mod agent;
pub mod config;
pub mod database;
pub mod error;
pub mod sandbox;

pub use config::Config;
pub use error::{Error, Result};

/// Application version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application name
pub const NAME: &str = env!("CARGO_PKG_NAME");
