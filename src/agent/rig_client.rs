//! Rig OpenRouter client wrapper
//!
//! This module provides a wrapper around rig-core's OpenRouter provider
//! implementing our LlmProvider trait for seamless integration.

use crate::config::OpenRouterConfig;
use crate::core::provider::{LlmProvider, ProviderMeta, GenerationOptions, LlmResponse, ModelInfo, LlmStream, UsageStats};
use crate::error::{Error, Result};
use rig::completion::{Prompt, CompletionModel};
use rig::providers::openrouter;
use rig::OneOrMany;
use secrecy::{ExposeSecret, SecretString};

/// Rig-based OpenRouter client wrapper
#[derive(Clone)]
pub struct RigLlmClient {
    /// Rig OpenRouter client
    client: openrouter::Client,
    /// Configuration
    config: OpenRouterConfig,
}

impl RigLlmClient {
    /// Create a new RigLlmClient from OpenRouter config
    pub fn new(config: OpenRouterConfig) -> Result<Self> {
        // Create rig's OpenRouter client with API key
        let client = openrouter::Client::new(config.api_key.expose_secret())
            .map_err(|e| Error::Config(format!("Failed to create OpenRouter client: {}", e)))?;

        Ok(Self { client, config })
    }

    /// Create from environment variable OPENROUTER_API_KEY
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| Error::Config("OPENROUTER_API_KEY not set".to_string()))?;

        let api_key = SecretString::from(api_key);
        let config = OpenRouterConfig {
            api_key,
            default_model: "anthropic/claude-3-haiku".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            site_url: None,
            site_name: None,
            timeout_secs: 30,
            max_retries: 3,
        };

        Self::new(config)
    }

    /// Get the underlying rig client
    pub fn client(&self) -> &openrouter::Client {
        &self.client
    }

    /// Create a completion model for the given model ID
    pub fn completion_model(&self, model: &str) -> openrouter::CompletionModel {
        self.client.completion_model(model)
    }

    /// Create an agent builder for this client with the given model
    pub fn agent(&self, model: &str) -> rig::agent::AgentBuilder<openrouter::CompletionModel> {
        self.client.agent(model)
    }

    /// Perform a simple prompt completion
    pub async fn prompt(&self, model: &str, prompt: &str) -> Result<String> {
        let completion_model = self.completion_model(model);

        let response = completion_model
            .prompt(prompt)
            .await
            .map_err(|e| Error::Provider(format!("OpenRouter completion failed: {}", e)))?;

        Ok(response)
    }

    /// Legacy method: perform a raw completion call (backwards compatibility)
    /// This is for compatibility with the old OpenRouterClient API
    pub async fn complete(&self, model: &str, prompt: String) -> Result<String> {
        self.prompt(model, &prompt).await
    }
}

#[async_trait::async_trait]
impl LlmProvider for RigLlmClient {
    fn meta(&self) -> &ProviderMeta {
        // Static metadata for OpenRouter
        static META: std::sync::OnceLock<ProviderMeta> = std::sync::OnceLock::new();
        META.get_or_init(|| ProviderMeta {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            description: "Unified API for multiple LLM providers via rig-core".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
        })
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Return a curated list of popular models available via OpenRouter
        Ok(vec![
            ModelInfo {
                id: "anthropic/claude-3-haiku".to_string(),
                name: "Claude 3 Haiku".to_string(),
                description: Some("Fast and efficient model by Anthropic".to_string()),
                context_length: Some(200000),
                input_price: Some(0.25),
                output_price: Some(1.25),
            },
            ModelInfo {
                id: "anthropic/claude-3-sonnet".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                description: Some("Balanced model by Anthropic".to_string()),
                context_length: Some(200000),
                input_price: Some(3.0),
                output_price: Some(15.0),
            },
            ModelInfo {
                id: "anthropic/claude-3.7-sonnet".to_string(),
                name: "Claude 3.7 Sonnet".to_string(),
                description: Some("Latest Sonnet model by Anthropic".to_string()),
                context_length: Some(200000),
                input_price: Some(3.0),
                output_price: Some(15.0),
            },
            ModelInfo {
                id: "openai/gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                description: Some("Fast and affordable model by OpenAI".to_string()),
                context_length: Some(128000),
                input_price: Some(0.15),
                output_price: Some(0.6),
            },
            ModelInfo {
                id: "google/gemini-2.0-flash-001".to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                description: Some("Fast multimodal model by Google".to_string()),
                context_length: Some(1000000),
                input_price: Some(0.0),
                output_price: Some(0.0),
            },
            ModelInfo {
                id: "qwen/qwq-32b".to_string(),
                name: "QwQ 32B".to_string(),
                description: Some("Reasoning model by Alibaba".to_string()),
                context_length: Some(32000),
                input_price: Some(0.0),
                output_price: Some(0.0),
            },
        ])
    }

    async fn generate(
        &self,
        messages: &[crate::agent::types::Message],
        options: &GenerationOptions,
    ) -> Result<LlmResponse> {
        let model = options.model.as_deref().unwrap_or(&self.config.default_model);

        // Convert our messages to OpenRouter's message format
        let rig_messages: Vec<openrouter::completion::Message> = messages
            .iter()
            .map(|msg| {
                match msg.role {
                    crate::agent::types::Role::System => openrouter::completion::Message::system(&msg.content),
                    crate::agent::types::Role::User => openrouter::completion::Message::User {
                        content: OneOrMany::one(msg.content.clone().into()),
                        name: None,
                    },
                    crate::agent::types::Role::Assistant => openrouter::completion::Message::Assistant {
                        content: vec![rig::providers::openai::AssistantContent::Text {
                            text: msg.content.clone(),
                        }],
                        refusal: None,
                        audio: None,
                        name: None,
                        tool_calls: vec![],
                        reasoning: None,
                        reasoning_details: vec![],
                    },
                }
            })
            .collect();

        // Create completion request using rig
        let mut request = self.completion_model(model)
            .completion_request(rig_messages);

        // Apply generation options
        if let Some(max_tokens) = options.max_tokens {
            request = request.max_tokens(max_tokens as usize);
        }
        if let Some(temperature) = options.temperature {
            request = request.temperature(temperature as f64);
        }
        if let Some(top_p) = options.top_p {
            request = request.top_p(top_p as f64);
        }

        // Execute the completion
        let response = request
            .send()
            .await
            .map_err(|e| Error::Provider(format!("OpenRouter completion failed: {}", e)))?;

        // Extract the content from the response
        let content = response.choice.to_content();

        Ok(LlmResponse {
            id: format!("openrouter-{}", uuid::Uuid::new_v4()),
            model: model.to_string(),
            content,
            finish_reason: Some("stop".to_string()),
            tool_calls: None, // TODO: Implement tool calls conversion
            usage: Some(UsageStats {
                prompt_tokens: response.usage.input_tokens as u32,
                completion_tokens: response.usage.output_tokens as u32,
                total_tokens: response.usage.total_tokens as u32,
            }),
        })
    }

    async fn generate_stream(
        &self,
        messages: &[crate::agent::types::Message],
        options: &GenerationOptions,
    ) -> Result<LlmStream> {
        let model = options.model.as_deref().unwrap_or(&self.config.default_model);

        // Convert our messages to OpenRouter's message format
        let rig_messages: Vec<openrouter::completion::Message> = messages
            .iter()
            .map(|msg| {
                match msg.role {
                    crate::agent::types::Role::System => openrouter::completion::Message::system(&msg.content),
                    crate::agent::types::Role::User => openrouter::completion::Message::User {
                        content: OneOrMany::one(msg.content.clone().into()),
                        name: None,
                    },
                    crate::agent::types::Role::Assistant => openrouter::completion::Message::Assistant {
                        content: vec![rig::providers::openai::AssistantContent::Text {
                            text: msg.content.clone(),
                        }],
                        refusal: None,
                        audio: None,
                        name: None,
                        tool_calls: vec![],
                        reasoning: None,
                        reasoning_details: vec![],
                    },
                }
            })
            .collect();

        // Create streaming completion request
        let mut request = self.completion_model(model)
            .completion_request(rig_messages);

        // Apply generation options
        if let Some(max_tokens) = options.max_tokens {
            request = request.max_tokens(max_tokens as usize);
        }
        if let Some(temperature) = options.temperature {
            request = request.temperature(temperature as f64);
        }
        if let Some(top_p) = options.top_p {
            request = request.top_p(top_p as f64);
        }

        // Create the stream
        let stream = request
            .stream()
            .await
            .map_err(|e| Error::Provider(format!("OpenRouter streaming failed: {}", e)))?;

        // Convert rig's stream to our LlmStream type
        use futures::stream::StreamExt;

        let mapped_stream = stream.map(|result| {
            result.map_err(|e| Error::Provider(format!("Stream error: {}", e)))
        });

        Ok(Box::pin(mapped_stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = OpenRouterConfig {
            api_key: SecretString::from("test-key".to_string()),
            default_model: "anthropic/claude-3-haiku".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            site_url: None,
            site_name: None,
            timeout_secs: 30,
            max_retries: 3,
        };

        let client = RigLlmClient::new(config);
        assert!(client.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires actual API key
    async fn test_list_models() {
        let client = RigLlmClient::from_env().unwrap();
        let models = client.list_models().await.unwrap();
        assert!(!models.is_empty());
    }
}
