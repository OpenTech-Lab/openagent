//! OpenRouter API client

use crate::config::OpenRouterConfig;
use crate::error::{Error, Result};
use crate::agent::types::*;
use reqwest::{Client, header};
use secrecy::ExposeSecret;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// OpenRouter API client
#[derive(Clone)]
pub struct OpenRouterClient {
    /// HTTP client
    client: Client,
    /// Configuration
    config: OpenRouterConfig,
    /// Rate limit state
    rate_limit: Arc<RwLock<RateLimitState>>,
}

/// Rate limit tracking
#[derive(Debug, Default)]
struct RateLimitState {
    /// Remaining requests
    remaining: Option<u32>,
    /// Reset timestamp
    reset_at: Option<u64>,
}

impl OpenRouterClient {
    /// Create a new OpenRouter client
    pub fn new(config: OpenRouterConfig) -> Result<Self> {
        let mut headers = header::HeaderMap::new();

        // Add authorization header
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!(
                "Bearer {}",
                config.api_key.expose_secret()
            ))
            .map_err(|e| Error::Config(format!("Invalid API key format: {}", e)))?,
        );

        // Add OpenRouter-specific headers
        if let Some(ref site_url) = config.site_url {
            if let Ok(value) = header::HeaderValue::from_str(site_url) {
                headers.insert("HTTP-Referer", value);
            }
        }
        if let Some(ref site_name) = config.site_name {
            if let Ok(value) = header::HeaderValue::from_str(site_name) {
                headers.insert("X-Title", value);
            }
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        Ok(OpenRouterClient {
            client,
            config,
            rate_limit: Arc::new(RwLock::new(RateLimitState::default())),
        })
    }

    /// Get the default model
    pub fn default_model(&self) -> &str {
        &self.config.default_model
    }

    /// Create a chat completion
    pub async fn chat(
        &self,
        messages: Vec<Message>,
        options: GenerationOptions,
    ) -> Result<ChatCompletionResponse> {
        self.chat_with_model(&self.config.default_model.clone(), messages, options)
            .await
    }

    /// Create a chat completion with a specific model
    pub async fn chat_with_model(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: GenerationOptions,
    ) -> Result<ChatCompletionResponse> {
        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            top_p: options.top_p,
            stop: options.stop,
            stream: Some(false),
            tools: None,
            tool_choice: None,
        };

        self.send_request(request).await
    }

    /// Create a chat completion with tools/functions
    pub async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolDefinition>,
        options: GenerationOptions,
    ) -> Result<ChatCompletionResponse> {
        let request = ChatCompletionRequest {
            model: self.config.default_model.clone(),
            messages,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            top_p: options.top_p,
            stop: options.stop,
            stream: Some(false),
            tools: Some(tools),
            tool_choice: Some(ToolChoice::Auto("auto".to_string())),
        };

        self.send_request(request).await
    }

    /// Send a request to the OpenRouter API
    async fn send_request(&self, request: ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        let url = format!("{}/chat/completions", self.config.base_url);

        debug!("Sending request to OpenRouter: model={}", request.model);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await?;

        // Update rate limit state from headers
        self.update_rate_limit(&response).await;

        let status = response.status();

        if status.is_success() {
            let body = response.json::<ChatCompletionResponse>().await?;

            if let Some(ref usage) = body.usage {
                info!(
                    "OpenRouter response: model={}, tokens={}",
                    body.model, usage.total_tokens
                );
            }

            Ok(body)
        } else {
            let error_text = response.text().await.unwrap_or_default();

            if status.as_u16() == 429 {
                warn!("Rate limit exceeded: {}", error_text);
                Err(Error::RateLimit(error_text))
            } else if status.as_u16() == 401 {
                Err(Error::Unauthorized("Invalid API key".to_string()))
            } else {
                Err(Error::OpenRouter(format!(
                    "API error ({}): {}",
                    status, error_text
                )))
            }
        }
    }

    /// Update rate limit state from response headers
    async fn update_rate_limit(&self, response: &reqwest::Response) {
        let mut state = self.rate_limit.write().await;

        if let Some(remaining) = response
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
        {
            state.remaining = Some(remaining);
        }

        if let Some(reset) = response
            .headers()
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
        {
            state.reset_at = Some(reset);
        }
    }

    /// Check if we should wait before making another request
    pub async fn should_wait(&self) -> Option<std::time::Duration> {
        let state = self.rate_limit.read().await;

        if let (Some(remaining), Some(reset_at)) = (state.remaining, state.reset_at) {
            if remaining == 0 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                if reset_at > now {
                    return Some(std::time::Duration::from_secs(reset_at - now));
                }
            }
        }

        None
    }

    /// List available models from OpenRouter
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.config.base_url);

        let response = self.client.get(&url).send().await?;

        if response.status().is_success() {
            let body: ModelsResponse = response.json().await?;
            Ok(body.data)
        } else {
            let error = response.text().await.unwrap_or_default();
            Err(Error::OpenRouter(format!("Failed to list models: {}", error)))
        }
    }
}

/// Response from /models endpoint
#[derive(Debug, serde::Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

/// Information about an available model
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelInfo {
    /// Model ID (e.g., "anthropic/claude-3.5-sonnet")
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Context length
    pub context_length: u32,
    /// Pricing info
    pub pricing: ModelPricing,
}

/// Model pricing information
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelPricing {
    /// Price per prompt token
    pub prompt: String,
    /// Price per completion token
    pub completion: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn test_config() -> OpenRouterConfig {
        OpenRouterConfig {
            api_key: SecretString::from("test-key"),
            default_model: "anthropic/claude-3.5-sonnet".to_string(),
            site_url: None,
            site_name: None,
            base_url: "https://openrouter.ai/api/v1".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }

    #[test]
    fn test_client_creation() {
        let config = test_config();
        let client = OpenRouterClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_generation_options() {
        let precise = GenerationOptions::precise();
        assert_eq!(precise.temperature, Some(0.0));

        let creative = GenerationOptions::creative();
        assert_eq!(creative.temperature, Some(0.8));
    }
}
