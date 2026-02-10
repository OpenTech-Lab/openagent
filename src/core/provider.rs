//! LLM Provider trait - Abstract interface for LLM backends
//!
//! This module defines the `LlmProvider` trait that allows OpenAgent to work with
//! any LLM backend (OpenRouter, Anthropic, OpenAI, local models, etc.)
//!
//! The trait-based approach enables:
//! - Easy addition of new providers without modifying core code
//! - Testing with mock providers
//! - Runtime provider switching based on configuration

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use futures::Stream;

use crate::error::Result;

// Re-export Message type from agent module for backward compatibility
pub use crate::agent::types::Message;

/// Metadata about a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    /// Unique provider identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Provider description
    pub description: String,
    /// Base URL for the API
    pub base_url: String,
    /// Whether the provider supports streaming
    pub supports_streaming: bool,
    /// Whether the provider supports tool calling
    pub supports_tools: bool,
    /// Whether the provider supports vision
    pub supports_vision: bool,
}

/// Options for LLM generation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerationOptions {
    /// Model to use (provider-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature for sampling (0.0-2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Enable streaming response
    #[serde(default)]
    pub stream: bool,
    /// Tool definitions for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Type of tool (usually "function")
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition
    pub function: FunctionDefinition,
}

/// Function definition for tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

/// Response from an LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// Unique response ID
    pub id: String,
    /// Model used for generation
    pub model: String,
    /// Generated content
    pub content: String,
    /// Finish reason (stop, length, tool_calls, etc.)
    pub finish_reason: Option<String>,
    /// Tool calls made by the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Token usage statistics
    pub usage: Option<UsageStats>,
}

/// Tool call requested by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Type (usually "function")
    #[serde(rename = "type")]
    pub call_type: String,
    /// Function details
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Arguments as JSON string
    pub arguments: String,
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageStats {
    /// Number of prompt tokens
    pub prompt_tokens: u32,
    /// Number of completion tokens
    pub completion_tokens: u32,
    /// Total tokens
    pub total_tokens: u32,
}

/// A streaming chunk from the provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingChunk {
    /// Chunk ID
    pub id: String,
    /// Delta content
    pub delta: String,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Finish reason (if final)
    pub finish_reason: Option<String>,
}

/// Stream of LLM response chunks
pub type LlmStream = Pin<Box<dyn Stream<Item = Result<StreamingChunk>> + Send>>;

/// Abstract interface for LLM providers
///
/// Implement this trait to add support for new LLM backends.
/// The provider handles authentication, request formatting, and response parsing.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get provider metadata
    fn meta(&self) -> &ProviderMeta;

    /// Get the provider ID
    fn id(&self) -> &str {
        &self.meta().id
    }

    /// Get the default model for this provider
    fn default_model(&self) -> &str;

    /// List available models
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Generate a response (non-streaming)
    async fn generate(
        &self,
        messages: &[Message],
        options: &GenerationOptions,
    ) -> Result<LlmResponse>;

    /// Generate a streaming response
    async fn generate_stream(
        &self,
        messages: &[Message],
        options: &GenerationOptions,
    ) -> Result<LlmStream>;

    /// Check if the provider is healthy
    async fn health_check(&self) -> Result<bool> {
        // Default implementation - try to list models
        self.list_models().await.map(|_| true)
    }
}

/// Information about an available model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Model description
    pub description: Option<String>,
    /// Context window size
    pub context_length: Option<u32>,
    /// Pricing per million tokens (input)
    pub input_price: Option<f64>,
    /// Pricing per million tokens (output)
    pub output_price: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_options_default() {
        let opts = GenerationOptions::default();
        assert!(opts.model.is_none());
        assert!(!opts.stream);
    }
}
