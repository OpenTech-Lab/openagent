//! Brave Search tool
//!
//! Web search using the Brave Search API. Requires a Brave Search API key.

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

use super::duckduckgo_search::SearchResult;
use super::traits::{Tool, ToolResult};
use super::{format_search_results, urlencoding};
use crate::Result;

/// Default timeout for search requests
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default number of results
const DEFAULT_RESULT_COUNT: u8 = 10;

/// Brave Search API response structures
#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveSearchResult>,
}

#[derive(Debug, Deserialize)]
struct BraveSearchResult {
    title: String,
    url: String,
    description: String,
}

/// Brave Search tool configuration
#[derive(Debug, Clone)]
pub struct BraveSearchConfig {
    /// API key for Brave Search
    pub api_key: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Default number of results
    pub result_count: u8,
}

impl BraveSearchConfig {
    /// Create config from environment variables
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("BRAVE_API_KEY").ok()?;
        Some(Self {
            api_key,
            timeout_secs: std::env::var("BRAVE_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_TIMEOUT_SECS),
            result_count: std::env::var("BRAVE_RESULT_COUNT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_RESULT_COUNT),
        })
    }
}

/// Brave Search tool for web searching
pub struct BraveSearchTool {
    client: Client,
    config: BraveSearchConfig,
}

impl BraveSearchTool {
    /// Create a new Brave Search tool
    pub fn new(config: BraveSearchConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    /// Create from environment variables
    pub fn from_env() -> Option<Self> {
        BraveSearchConfig::from_env().map(Self::new)
    }

    /// Perform a web search
    async fn search(&self, query: &str, count: u8, country: Option<&str>) -> Result<Vec<SearchResult>> {
        let mut url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            urlencoding::encode(query),
            count.min(20)
        );

        if let Some(cc) = country {
            url.push_str(&format!("&country={}", cc));
        }

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .header("Accept-Encoding", "gzip")
            .header("X-Subscription-Token", &self.config.api_key)
            .send()
            .await
            .map_err(|e| crate::Error::Provider(format!("Brave search request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text: String = response.text().await.unwrap_or_default();
            return Err(crate::Error::Provider(format!(
                "Brave search failed with status {}: {}",
                status, text
            )));
        }

        let brave_response: BraveSearchResponse = response
            .json::<BraveSearchResponse>()
            .await
            .map_err(|e| crate::Error::Provider(format!("Failed to parse Brave response: {}", e)))?;

        let results = brave_response
            .web
            .map(|w| {
                w.results
                    .into_iter()
                    .map(|r| SearchResult {
                        title: r.title,
                        url: r.url,
                        snippet: r.description,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(results)
    }
}

#[async_trait]
impl Tool for BraveSearchTool {
    fn name(&self) -> &str {
        "brave_search"
    }

    fn description(&self) -> &str {
        "Search the web using Brave Search API. Returns relevant web pages with titles, URLs, and snippets."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of results to return (1-20, default: 10)"
                },
                "country": {
                    "type": "string",
                    "description": "Country code for localized results (e.g., 'us', 'jp', 'gb')"
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

        let count = args
            .get("count")
            .and_then(|v| v.as_u64())
            .map(|c| c as u8)
            .unwrap_or(self.config.result_count);

        let country = args.get("country").and_then(|v| v.as_str());

        match self.search(query, count, country).await {
            Ok(results) => {
                if results.is_empty() {
                    Ok(ToolResult::success("No results found for the query."))
                } else {
                    let formatted = format_search_results(&results);
                    Ok(ToolResult::success(formatted))
                }
            }
            Err(e) => Ok(ToolResult::failure(format!("Search failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brave_config_from_env() {
        // Just test that it doesn't panic
        let _ = BraveSearchConfig::from_env();
    }
}
