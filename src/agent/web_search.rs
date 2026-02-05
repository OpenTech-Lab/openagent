//! Web search tools for the agent
//!
//! Provides web search capabilities:
//! - DuckDuckGo (default, no API key required)
//! - Brave Search (requires API key)
//! - Perplexity (requires API key)
//!
//! Based on openclaw's web-search.ts implementation.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use crate::Result;

use super::tools::{Tool, ToolResult};

/// Default timeout for web search requests
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum number of search results
const DEFAULT_RESULT_COUNT: u8 = 10;

// ============================================================================
// DuckDuckGo Search Tool (No API Key Required)
// ============================================================================

/// DuckDuckGo Instant Answer API response
#[derive(Debug, Deserialize)]
struct DuckDuckGoResponse {
    #[serde(rename = "AbstractText")]
    abstract_text: Option<String>,
    #[serde(rename = "AbstractURL")]
    abstract_url: Option<String>,
    #[serde(rename = "AbstractSource")]
    abstract_source: Option<String>,
    #[serde(rename = "Heading")]
    heading: Option<String>,
    #[serde(rename = "RelatedTopics")]
    related_topics: Option<Vec<DuckDuckGoTopic>>,
    #[serde(rename = "Results")]
    results: Option<Vec<DuckDuckGoResult>>,
}

#[derive(Debug, Deserialize)]
struct DuckDuckGoTopic {
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "FirstURL")]
    first_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DuckDuckGoResult {
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "FirstURL")]
    first_url: Option<String>,
}

/// DuckDuckGo search tool - works without API key
pub struct DuckDuckGoSearchTool {
    client: Client,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl Default for DuckDuckGoSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DuckDuckGoSearchTool {
    /// Create a new DuckDuckGo search tool
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .user_agent("OpenAgent/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }

    /// Create with custom timeout
    pub fn with_timeout(timeout_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent("OpenAgent/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            timeout_secs,
        }
    }

    /// Perform a search using DuckDuckGo Instant Answer API
    async fn search(&self, query: &str, count: u8) -> Result<Vec<SearchResult>> {
        // DuckDuckGo Instant Answer API (free, no API key)
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| crate::Error::Provider(format!("DuckDuckGo request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(crate::Error::Provider(format!(
                "DuckDuckGo search failed with status {}",
                status
            )));
        }

        let ddg_response: DuckDuckGoResponse = response
            .json::<DuckDuckGoResponse>()
            .await
            .map_err(|e| crate::Error::Provider(format!("Failed to parse DuckDuckGo response: {}", e)))?;

        let mut results = Vec::new();
        let max_results = count as usize;

        // Add abstract/main result if available
        if let (Some(text), Some(url), Some(source)) = (
            &ddg_response.abstract_text,
            &ddg_response.abstract_url,
            &ddg_response.abstract_source,
        ) {
            if !text.is_empty() {
                results.push(SearchResult {
                    title: ddg_response.heading.clone().unwrap_or_else(|| source.clone()),
                    url: url.clone(),
                    snippet: text.clone(),
                });
            }
        }

        // Add direct results
        if let Some(direct_results) = ddg_response.results {
            for r in direct_results.into_iter().take(max_results - results.len()) {
                if let (Some(text), Some(url)) = (r.text, r.first_url) {
                    results.push(SearchResult {
                        title: text.chars().take(100).collect(),
                        url,
                        snippet: text,
                    });
                }
            }
        }

        // Add related topics as results
        if let Some(topics) = ddg_response.related_topics {
            for topic in topics.into_iter().take(max_results - results.len()) {
                if let (Some(text), Some(url)) = (topic.text, topic.first_url) {
                    // Extract title from text (usually format: "Title - Description")
                    let title = text.split(" - ").next().unwrap_or(&text).to_string();
                    results.push(SearchResult {
                        title,
                        url,
                        snippet: text,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Perform HTML scraping search for more comprehensive results
    async fn search_html(&self, query: &str, count: u8) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| crate::Error::Provider(format!("DuckDuckGo HTML request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(crate::Error::Provider(format!(
                "DuckDuckGo HTML search failed with status {}",
                response.status()
            )));
        }

        let html = response
            .text()
            .await
            .map_err(|e| crate::Error::Provider(format!("Failed to read DuckDuckGo response: {}", e)))?;

        // Simple HTML parsing for search results
        let mut results = Vec::new();
        let max_results = count as usize;

        // Parse result blocks - looking for class="result__a" links and class="result__snippet" text
        for (i, chunk) in html.split("class=\"result__a\"").skip(1).enumerate() {
            if i >= max_results {
                break;
            }

            // Extract URL
            let url = chunk
                .split("href=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(|s| {
                    // DuckDuckGo wraps URLs, extract the actual URL
                    if s.contains("uddg=") {
                        s.split("uddg=")
                            .nth(1)
                            .and_then(|u| urlencoding::decode(u).ok())
                            .unwrap_or_else(|| s.to_string())
                    } else {
                        s.to_string()
                    }
                });

            // Extract title
            let title = chunk
                .split('>')
                .nth(1)
                .and_then(|s| s.split('<').next())
                .map(|s| html_decode(s));

            // Extract snippet
            let snippet = chunk
                .split("class=\"result__snippet\"")
                .nth(1)
                .and_then(|s| s.split('>').nth(1))
                .and_then(|s| s.split('<').next())
                .map(|s| html_decode(s));

            if let (Some(url), Some(title)) = (url, title) {
                if !url.is_empty() && !title.is_empty() {
                    results.push(SearchResult {
                        title,
                        url,
                        snippet: snippet.unwrap_or_default(),
                    });
                }
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Tool for DuckDuckGoSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using DuckDuckGo. Returns relevant web pages with titles, URLs, and snippets. No API key required."
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
                    "description": "Number of results to return (1-10, default: 5)"
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
            .map(|c| c.min(10) as u8)
            .unwrap_or(5);

        // Try instant answer API first, fallback to HTML scraping
        let results = match self.search(query, count).await {
            Ok(r) if !r.is_empty() => r,
            _ => {
                // Fallback to HTML scraping for more results
                self.search_html(query, count).await.unwrap_or_default()
            }
        };

        if results.is_empty() {
            Ok(ToolResult::success(format!(
                "No direct results found for '{}'. Try rephrasing your query.",
                query
            )))
        } else {
            let formatted = format_search_results(&results);
            Ok(ToolResult::success(formatted))
        }
    }
}

/// Simple HTML entity decoder
fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

// ============================================================================
// Brave Search Tool
// ============================================================================

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
            count.min(20) // Brave max is 20
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

// ============================================================================
// Perplexity Search Tool
// ============================================================================

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
        // Check if using OpenRouter for Perplexity
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
                // Direct Perplexity uses different model names
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

        // Extract the assistant's response
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

// ============================================================================
// Common Types and Utilities
// ============================================================================

/// A search result from any provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Title of the page
    pub title: String,
    /// URL of the page
    pub url: String,
    /// Snippet or description
    pub snippet: String,
}

/// Format search results for display
fn format_search_results(results: &[SearchResult]) -> String {
    let mut output = String::new();
    
    for (i, result) in results.iter().enumerate() {
        output.push_str(&format!(
            "{}. **{}**\n   URL: {}\n   {}\n\n",
            i + 1,
            result.title,
            result.url,
            result.snippet
        ));
    }
    
    output
}

/// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }

    pub fn decode(s: &str) -> Result<String, ()> {
        url::form_urlencoded::parse(s.as_bytes())
            .next()
            .map(|(k, _)| k.to_string())
            .ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duckduckgo_tool() {
        let tool = DuckDuckGoSearchTool::new();
        assert_eq!(tool.name(), "web_search");
    }

    #[test]
    fn test_brave_config_from_env() {
        // This test requires BRAVE_API_KEY to be set
        // Just test that it doesn't panic
        let _ = BraveSearchConfig::from_env();
    }

    #[test]
    fn test_perplexity_config_from_env() {
        // This test requires PERPLEXITY_API_KEY or OPENROUTER_API_KEY to be set
        // Just test that it doesn't panic
        let _ = PerplexityConfig::from_env();
    }

    #[test]
    fn test_format_search_results() {
        let results = vec![
            SearchResult {
                title: "Test Title".to_string(),
                url: "https://example.com".to_string(),
                snippet: "Test snippet".to_string(),
            },
        ];
        
        let formatted = format_search_results(&results);
        assert!(formatted.contains("Test Title"));
        assert!(formatted.contains("https://example.com"));
        assert!(formatted.contains("Test snippet"));
    }

    #[test]
    fn test_html_decode() {
        assert_eq!(html_decode("Hello &amp; World"), "Hello & World");
        assert_eq!(html_decode("&lt;tag&gt;"), "<tag>");
    }
}
