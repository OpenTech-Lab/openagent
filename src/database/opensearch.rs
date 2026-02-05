//! OpenSearch client for full-text search

use crate::config::OpenSearchConfig;
use crate::error::{Error, Result};
use opensearch::{
    auth::Credentials,
    cert::CertificateValidation,
    http::transport::{SingleNodeConnectionPool, TransportBuilder},
    OpenSearch,
};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info};
use url::Url;

/// OpenSearch client wrapper
pub struct OpenSearchClient {
    client: OpenSearch,
    index_prefix: String,
}

/// A document to be indexed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDocument {
    /// Document ID
    pub id: String,
    /// User ID
    pub user_id: String,
    /// Conversation ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Document content
    pub content: String,
    /// Document type (message, memory, etc.)
    pub doc_type: String,
    /// Role (for messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Timestamp
    pub timestamp: String,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Document
    pub document: SearchDocument,
    /// Relevance score
    pub score: f32,
    /// Highlighted content
    pub highlights: Vec<String>,
}

impl OpenSearchClient {
    /// Create a new OpenSearch client
    pub async fn new(config: &OpenSearchConfig) -> Result<Self> {
        let url = Url::parse(&config.url)
            .map_err(|e| Error::Config(format!("Invalid OpenSearch URL: {}", e)))?;

        let conn_pool = SingleNodeConnectionPool::new(url);

        let mut transport_builder = TransportBuilder::new(conn_pool);

        // Add authentication if provided
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            transport_builder = transport_builder.auth(Credentials::Basic(
                username.clone(),
                password.expose_secret().to_string(),
            ));
        }

        // Disable certificate validation for self-signed certs (dev only)
        transport_builder = transport_builder.cert_validation(CertificateValidation::None);

        let transport = transport_builder
            .build()
            .map_err(|e| Error::OpenSearch(format!("Failed to build transport: {}", e)))?;

        let client = OpenSearch::new(transport);

        let opensearch_client = OpenSearchClient {
            client,
            index_prefix: config.index_prefix.clone(),
        };

        // Verify connection
        opensearch_client.health_check().await?;

        info!("OpenSearch client initialized successfully");
        Ok(opensearch_client)
    }

    /// Check cluster health
    pub async fn health_check(&self) -> Result<()> {
        let response = self
            .client
            .cluster()
            .health(opensearch::cluster::ClusterHealthParts::None)
            .send()
            .await
            .map_err(|e| Error::OpenSearch(format!("Health check failed: {}", e)))?;

        if !response.status_code().is_success() {
            return Err(Error::OpenSearch(format!(
                "Cluster unhealthy: {}",
                response.status_code()
            )));
        }

        Ok(())
    }

    /// Get index name for a given type
    fn index_name(&self, doc_type: &str) -> String {
        format!("{}-{}", self.index_prefix, doc_type)
    }

    /// Initialize indexes with mappings
    pub async fn init_indexes(&self) -> Result<()> {
        // Messages index
        self.create_index_if_not_exists(
            "messages",
            json!({
                "mappings": {
                    "properties": {
                        "id": { "type": "keyword" },
                        "user_id": { "type": "keyword" },
                        "conversation_id": { "type": "keyword" },
                        "content": {
                            "type": "text",
                            "analyzer": "standard"
                        },
                        "doc_type": { "type": "keyword" },
                        "role": { "type": "keyword" },
                        "timestamp": { "type": "date" },
                        "tags": { "type": "keyword" }
                    }
                }
            }),
        )
        .await?;

        // Memories index
        self.create_index_if_not_exists(
            "memories",
            json!({
                "mappings": {
                    "properties": {
                        "id": { "type": "keyword" },
                        "user_id": { "type": "keyword" },
                        "content": {
                            "type": "text",
                            "analyzer": "standard"
                        },
                        "doc_type": { "type": "keyword" },
                        "timestamp": { "type": "date" },
                        "tags": { "type": "keyword" }
                    }
                }
            }),
        )
        .await?;

        info!("OpenSearch indexes initialized");
        Ok(())
    }

    /// Create an index if it doesn't exist
    async fn create_index_if_not_exists(&self, doc_type: &str, mapping: Value) -> Result<()> {
        let index = self.index_name(doc_type);

        let exists = self
            .client
            .indices()
            .exists(opensearch::indices::IndicesExistsParts::Index(&[&index]))
            .send()
            .await
            .map_err(|e| Error::OpenSearch(format!("Failed to check index: {}", e)))?;

        if exists.status_code().as_u16() == 404 {
            self.client
                .indices()
                .create(opensearch::indices::IndicesCreateParts::Index(&index))
                .body(mapping)
                .send()
                .await
                .map_err(|e| Error::OpenSearch(format!("Failed to create index: {}", e)))?;

            info!("Created index: {}", index);
        }

        Ok(())
    }

    /// Index a document
    pub async fn index_document(&self, doc: &SearchDocument) -> Result<()> {
        let index = self.index_name(&doc.doc_type);

        self.client
            .index(opensearch::IndexParts::IndexId(&index, &doc.id))
            .body(doc)
            .send()
            .await
            .map_err(|e| Error::OpenSearch(format!("Failed to index document: {}", e)))?;

        debug!("Indexed document: {} in {}", doc.id, index);
        Ok(())
    }

    /// Delete a document
    pub async fn delete_document(&self, doc_type: &str, id: &str) -> Result<()> {
        let index = self.index_name(doc_type);

        self.client
            .delete(opensearch::DeleteParts::IndexId(&index, id))
            .send()
            .await
            .map_err(|e| Error::OpenSearch(format!("Failed to delete document: {}", e)))?;

        Ok(())
    }

    /// Search for documents
    pub async fn search(
        &self,
        doc_type: &str,
        query: &str,
        user_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let index = self.index_name(doc_type);

        let mut must_clauses = vec![json!({
            "multi_match": {
                "query": query,
                "fields": ["content^2", "tags"],
                "type": "best_fields",
                "fuzziness": "AUTO"
            }
        })];

        if let Some(user_id) = user_id {
            must_clauses.push(json!({
                "term": { "user_id": user_id }
            }));
        }

        let search_body = json!({
            "query": {
                "bool": {
                    "must": must_clauses
                }
            },
            "size": limit,
            "highlight": {
                "fields": {
                    "content": {}
                }
            }
        });

        let response = self
            .client
            .search(opensearch::SearchParts::Index(&[&index]))
            .body(search_body)
            .send()
            .await
            .map_err(|e| Error::OpenSearch(format!("Search failed: {}", e)))?;

        let body: Value = response
            .json()
            .await
            .map_err(|e| Error::OpenSearch(format!("Failed to parse response: {}", e)))?;

        let hits = body["hits"]["hits"]
            .as_array()
            .ok_or_else(|| Error::OpenSearch("Invalid response format".to_string()))?;

        let results = hits
            .iter()
            .filter_map(|hit| {
                let doc: SearchDocument = serde_json::from_value(hit["_source"].clone()).ok()?;
                let score = hit["_score"].as_f64().unwrap_or(0.0) as f32;
                let highlights = hit["highlight"]["content"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                Some(SearchResult {
                    document: doc,
                    score,
                    highlights,
                })
            })
            .collect();

        Ok(results)
    }

    /// Search messages for a specific user
    pub async fn search_messages(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        self.search("messages", query, Some(user_id), limit).await
    }

    /// Search memories for a specific user
    pub async fn search_memories(
        &self,
        user_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        self.search("memories", query, Some(user_id), limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_document_serialization() {
        let doc = SearchDocument {
            id: "test-id".to_string(),
            user_id: "user123".to_string(),
            conversation_id: Some("conv456".to_string()),
            content: "Hello, world!".to_string(),
            doc_type: "message".to_string(),
            role: Some("user".to_string()),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            tags: vec!["test".to_string()],
        };

        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.contains("Hello, world!"));
    }
}
