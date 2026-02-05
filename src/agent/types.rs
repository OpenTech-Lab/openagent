//! Type definitions for the agent module

use serde::{Deserialize, Serialize};

/// Role of a message in a conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message providing context and instructions
    System,
    /// User message
    User,
    /// Assistant (AI) response
    Assistant,
    /// Tool/function result
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: Role,
    /// Content of the message
    pub content: String,
    /// Optional name (for tool messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional tool call ID (for tool messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional tool calls made by assistant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AssistantToolCall>>,
}

impl Message {
    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Message {
            role: Role::System,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Message {
            role: Role::User,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Message {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    /// Create a new tool result message
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Message {
            role: Role::Tool,
            content: content.into(),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

/// Tool call made by the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Type of tool call (usually "function")
    #[serde(rename = "type")]
    pub call_type: String,
    /// Function details
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function to call
    pub name: String,
    /// Arguments as JSON string
    pub arguments: String,
}

/// Request to the OpenRouter API
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    /// Model to use
    pub model: String,
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Whether to stream responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Available tools/functions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// Tool choice strategy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
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

/// Function definition for tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Name of the function
    pub name: String,
    /// Description of what the function does
    pub description: String,
    /// JSON Schema for function parameters
    pub parameters: serde_json::Value,
}

/// Tool choice strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Let the model decide
    Auto(String),
    /// Never use tools
    None(String),
    /// Force a specific tool
    Specific {
        #[serde(rename = "type")]
        tool_type: String,
        function: FunctionName,
    },
}

/// Function name for specific tool choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionName {
    /// Name of the function to call
    pub name: String,
}

/// Response from the OpenRouter API
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    /// Unique ID for this completion
    pub id: String,
    /// Object type
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model used
    pub model: String,
    /// Completion choices
    pub choices: Vec<Choice>,
    /// Usage statistics
    pub usage: Option<Usage>,
}

/// A completion choice
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    /// Index of this choice
    pub index: u32,
    /// The generated message
    pub message: Message,
    /// Reason for stopping
    pub finish_reason: Option<String>,
}

/// Token usage statistics
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    /// Tokens in the prompt
    pub prompt_tokens: u32,
    /// Tokens in the completion
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

/// Streaming response chunk
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionChunk {
    /// Unique ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model used
    pub model: String,
    /// Delta choices
    pub choices: Vec<ChunkChoice>,
}

/// A streaming choice delta
#[derive(Debug, Clone, Deserialize)]
pub struct ChunkChoice {
    /// Index of this choice
    pub index: u32,
    /// The delta content
    pub delta: MessageDelta,
    /// Reason for stopping
    pub finish_reason: Option<String>,
}

/// Delta content in streaming response
#[derive(Debug, Clone, Deserialize)]
pub struct MessageDelta {
    /// Role (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Content delta
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls delta
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Tool call delta in streaming
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallDelta {
    /// Index of this tool call
    pub index: u32,
    /// Tool call ID (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Type (only in first chunk)
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    /// Function delta
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionDelta>,
}

/// Function delta in streaming
#[derive(Debug, Clone, Deserialize)]
pub struct FunctionDelta {
    /// Function name (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Arguments delta
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// Generation options for chat completions
#[derive(Debug, Clone, Default)]
pub struct GenerationOptions {
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Sampling temperature (0.0 - 2.0)
    pub temperature: Option<f32>,
    /// Top-p sampling (0.0 - 1.0)
    pub top_p: Option<f32>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
    /// Whether to stream the response
    pub stream: bool,
}

impl GenerationOptions {
    /// Create options for precise, deterministic output
    pub fn precise() -> Self {
        GenerationOptions {
            temperature: Some(0.0),
            ..Default::default()
        }
    }

    /// Create options for creative output
    pub fn creative() -> Self {
        GenerationOptions {
            temperature: Some(0.8),
            top_p: Some(0.95),
            ..Default::default()
        }
    }

    /// Create options for balanced output
    pub fn balanced() -> Self {
        GenerationOptions {
            temperature: Some(0.5),
            ..Default::default()
        }
    }
}
