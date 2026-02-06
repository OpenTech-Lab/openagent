//! Perplexity search tool
//!
//! AI-powered search using Perplexity's chat completions API.
//! Can use either direct Perplexity API or OpenRouter as proxy.

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use super::traits::{Tool, ToolResult};
use crate::Result;

/// Default timeout for search requests
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Perplexity API configuration
#[derive(Debug, Clone)]
pub struct PerplexityConfig {
    /// API key for Perplexity
    pub api_key: String,
    /// Whether to use OpenRouter as proxy
    pub use_openrouter: bool,
    /// OpenRouter API key (if using OpenRouter)
    pub openrouter_api_key: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Model to use
    pub model: String,
}

impl PerplexityConfig {
    /// Create config from environment variables
    pub fn from_env() -> Option<Self> {
        let openrouter_key = std::env::var("OPENROUTER_API_KEY").ok();
        let perplexity_key = std::env::var("PERPLEXITY_API_KEY").ok();

        // Prefer direct Perplexity API, fallback to OpenRouter
        let (api_key, use_openrouter) = match (&perplexity_key, &openrouter_key) {
            (Some(pk), _) => (pk.clone(), false),
            (None, Some(ok)) => (ok.clone(), true),
            (None, None) => return None,
        };

        Some(Self {
            api_key,
            use_openrouter,
            openrouter_api_key: openrouter_key,
            timeout_secs: std::env::var("PERPLEXITY_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_TIMEOUT_SECS),
            model: std::env::var("PERPLEXITY_MODEL")
                .unwrap_or_else(|_| "perplexity/sonar-pro".to_string()),
        })
    }
}

/// Perplexity search tool using chat completions API
pub struct PerplexitySearchTool {
    client: Client,
    config: PerplexityConfig,
}

impl PerplexitySearchTool {
    /// Create a new Perplexity Search tool
    pub fn new(config: PerplexityConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    /// Create from environment variables
    pub fn from_env() -> Option<Self> {
        PerplexityConfig::from_env().map(Self::new)
    }

    /// Perform a search using Perplexity's chat API
    async fn search(&self, query: &str) -> Result<String> {
        let (base_url, auth_header, model) = if self.config.use_openrouter {
            (
                "https://openrouter.ai/api/v1/chat/completions",
                format!("Bearer {}", self.config.api_key),
                self.config.model.clone(),
            )
        } else {
            (
                "https://api.perplexity.ai/chat/completions",
                format!("Bearer {}", self.config.api_key),
                if self.config.model.starts_with("perplexity/") {
                    self.config.model.replace("perplexity/", "")
                } else {
                    self.config.model.clone()
                },
            )
        };

        let request_body = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a helpful search assistant. Provide concise, factual answers with sources when available. Focus on the most relevant and up-to-date information."
                },
                {
                    "role": "user",
                    "content": query
                }
            ],
            "temperature": 0.1,
            "max_tokens": 2048
        });

        let response = self
            .client
            .post(base_url)
            .header("Content-Type", "application/json")
            .header("Authorization", &auth_header)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| crate::Error::Provider(format!("Perplexity request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text: String = response.text().await.unwrap_or_default();
            return Err(crate::Error::Provider(format!(
                "Perplexity search failed with status {}: {}",
                status, text
            )));
        }

        let json: Value = response
            .json::<Value>()
            .await
            .map_err(|e| crate::Error::Provider(format!("Failed to parse Perplexity response: {}", e)))?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("No response received")
            .to_string();

        Ok(content)
    }
}

#[async_trait]
impl Tool for PerplexitySearchTool {
    fn name(&self) -> &str {
        "perplexity_search"
    }

    fn description(&self) -> &str {
        "Search the web using Perplexity AI. Provides AI-synthesized answers with real-time web information and sources."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query or question to answer"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::Error::InvalidInput("Missing 'query' parameter".to_string()))?;

        match self.search(query).await {
            Ok(response) => Ok(ToolResult::success(response)),
            Err(e) => Ok(ToolResult::failure(format!("Perplexity search failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perplexity_config_from_env() {
        // Just test that it doesn't panic
        let _ = PerplexityConfig::from_env();
    }
}
