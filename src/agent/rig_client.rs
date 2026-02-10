//! Rig OpenRouter client wrapper
//!
//! This module provides a wrapper for OpenRouter API calls using rig-core's types
//! but with direct HTTP requests since rig-core doesn't have OpenRouter provider.

use crate::config::OpenRouterConfig;
use crate::core::provider::{LlmProvider, ProviderMeta, GenerationOptions, LlmResponse, ModelInfo, LlmStream};
use crate::error::{Error, Result};
use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

/// OpenRouter API request for completions
#[derive(Serialize)]
struct OpenRouterCompletionRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    stop: Option<Vec<String>>,
    stream: Option<bool>,
}

/// OpenRouter message format
#[derive(Serialize, Deserialize, Clone)]
struct OpenRouterMessage {
    role: String,
    content: String,
}

/// OpenRouter API response
#[derive(Deserialize)]
struct OpenRouterCompletionResponse {
    choices: Vec<OpenRouterChoice>,
}

/// Choice in OpenRouter response
#[derive(Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessage,
}

/// Rig-based OpenRouter client wrapper
#[derive(Clone)]
pub struct RigLlmClient {
    /// HTTP client for OpenRouter API
    client: Client,
    /// Configuration
    config: OpenRouterConfig,
}

impl RigLlmClient {
    /// Create a new RigLlmClient from OpenRouter config
    pub fn new(config: OpenRouterConfig) -> Result<Self> {
        let client_builder = Client::builder();

        // Add OpenRouter-specific headers
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!(
                "Bearer {}",
                config.api_key.expose_secret()
            ))
            .map_err(|e| Error::Config(format!("Invalid API key format: {}", e)))?,
        );

        if let Some(ref site_url) = config.site_url {
            headers.insert("HTTP-Referer", reqwest::header::HeaderValue::from_str(site_url)
                .map_err(|e| Error::Config(format!("Invalid site URL format: {}", e)))?);
        }

        if let Some(ref site_name) = config.site_name {
            headers.insert("X-Title", reqwest::header::HeaderValue::from_str(site_name)
                .map_err(|e| Error::Config(format!("Invalid site name format: {}", e)))?);
        }

        let client = client_builder
            .default_headers(headers)
            .build()
            .map_err(|e| Error::Config(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    /// Perform a raw completion call
    pub async fn complete(
        &self,
        model: &str,
        prompt: String,
    ) -> Result<String> {
        let messages = vec![OpenRouterMessage {
            role: "user".to_string(),
            content: prompt,
        }];

        let request = OpenRouterCompletionRequest {
            model: model.to_string(),
            messages,
            max_tokens: None,
            temperature: Some(0.7),
            top_p: None,
            stop: None,
            stream: Some(false),
        };

        let response = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("OpenRouter request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Provider(format!("OpenRouter API error: {}", error_text)));
        }

        let completion_response: OpenRouterCompletionResponse = response
            .json()
            .await
            .map_err(|e| Error::Provider(format!("Failed to parse OpenRouter response: {}", e)))?;

        if let Some(choice) = completion_response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err(Error::Provider("No completion choices returned".to_string()))
        }
    }

    /// Create an agent builder for this client
    /// Note: This is a simplified version since rig-core doesn't have OpenRouter provider
    pub fn agent_builder(&self, _model: &str) -> Result<SimpleAgentBuilder> {
        // For now, return a simple builder that doesn't use rig's agent system
        // This will be expanded in Phase 2
        Ok(SimpleAgentBuilder {
            client: self.clone(),
            model: _model.to_string(),
        })
    }
}

/// Simple agent builder (placeholder until rig-core agent system is integrated)
pub struct SimpleAgentBuilder {
    client: RigLlmClient,
    model: String,
}

impl SimpleAgentBuilder {
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn client(&self) -> &RigLlmClient {
        &self.client
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
            description: "Unified API for multiple LLM providers".to_string(),
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
        // For now, return a basic list. In a real implementation, this would call the OpenRouter models API
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
                id: "openai/gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                description: Some("Fast and affordable model by OpenAI".to_string()),
                context_length: Some(128000),
                input_price: Some(0.15),
                output_price: Some(0.6),
            },
        ])
    }

    async fn generate(
        &self,
        messages: &[crate::agent::types::Message],
        options: &GenerationOptions,
    ) -> Result<LlmResponse> {
        // Convert core::types::Message to OpenRouter format
        let openrouter_messages: Vec<OpenRouterMessage> = messages
            .iter()
            .map(|msg| OpenRouterMessage {
                role: msg.role.to_string(),
                content: msg.content.clone(),
            })
            .collect();

        let model = options.model.as_deref().unwrap_or(&self.config.default_model);

        let request = OpenRouterCompletionRequest {
            model: model.to_string(),
            messages: openrouter_messages,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            top_p: options.top_p,
            stop: options.stop.clone(),
            stream: Some(false),
        };

        let response = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("OpenRouter request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Provider(format!("OpenRouter API error: {}", error_text)));
        }

        let completion_response: OpenRouterCompletionResponse = response
            .json()
            .await
            .map_err(|e| Error::Provider(format!("Failed to parse OpenRouter response: {}", e)))?;

        if let Some(choice) = completion_response.choices.first() {
            Ok(LlmResponse {
                id: "openrouter-response".to_string(), // OpenRouter doesn't provide IDs in this format
                model: model.to_string(),
                content: choice.message.content.clone(),
                finish_reason: Some("stop".to_string()), // Default finish reason
                tool_calls: None, // Not implemented yet
                usage: None, // Not implemented yet
            })
        } else {
            Err(Error::Provider("No completion choices returned".to_string()))
        }
    }

    async fn generate_stream(
        &self,
        _messages: &[crate::agent::types::Message],
        _options: &GenerationOptions,
    ) -> Result<LlmStream> {
        // Streaming not implemented yet - return an error for now
        Err(Error::Provider("Streaming not yet implemented for RigLlmClient".to_string()))
    }
}