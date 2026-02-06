//! DuckDuckGo search tool
//!
//! Web search using DuckDuckGo APIs (no API key required).
//! Uses both the Instant Answer API and HTML scraping as fallback.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use super::traits::{Tool, ToolResult};
use super::{format_search_results, urlencoding};
use crate::Result;

/// Default timeout for web search requests
const DEFAULT_TIMEOUT_SECS: u64 = 30;

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
    async fn search(&self, query: &str, count: u8) -> crate::Result<Vec<SearchResult>> {
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
    async fn search_html(&self, query: &str, count: u8) -> crate::Result<Vec<SearchResult>> {
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

        let mut results = Vec::new();
        let max_results = count as usize;

        for (i, chunk) in html.split("class=\"result__a\"").skip(1).enumerate() {
            if i >= max_results {
                break;
            }

            let url = chunk
                .split("href=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(|s| {
                    if s.contains("uddg=") {
                        s.split("uddg=")
                            .nth(1)
                            .and_then(|u| urlencoding::decode(u).ok())
                            .unwrap_or_else(|| s.to_string())
                    } else {
                        s.to_string()
                    }
                });

            let title = chunk
                .split('>')
                .nth(1)
                .and_then(|s| s.split('<').next())
                .map(|s| html_decode(s));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duckduckgo_tool() {
        let tool = DuckDuckGoSearchTool::new();
        assert_eq!(tool.name(), "web_search");
    }

    #[test]
    fn test_html_decode() {
        assert_eq!(html_decode("Hello &amp; World"), "Hello & World");
        assert_eq!(html_decode("&lt;tag&gt;"), "<tag>");
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
}
