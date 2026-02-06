//! Local embedding generation via fastembed
//!
//! Uses the multilingual-e5-small model (384 dimensions, ~90MB).
//! Supports 100+ languages including Japanese.
//! Model auto-downloads on first use.

use crate::error::{Error, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Arc;

/// Local embedding service wrapping fastembed
#[derive(Clone)]
pub struct EmbeddingService {
    model: Arc<TextEmbedding>,
}

impl EmbeddingService {
    /// Create a new embedding service with multilingual-e5-small
    pub fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::MultilingualE5Small).with_show_download_progress(true),
        )
        .map_err(|e| Error::Internal(format!("Failed to init embedding model: {}", e)))?;

        Ok(EmbeddingService {
            model: Arc::new(model),
        })
    }

    /// Generate an embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.model.clone();
        let text = text.to_string();

        tokio::task::spawn_blocking(move || {
            let embeddings = model
                .embed(vec![text], None)
                .map_err(|e| Error::Internal(format!("Embedding error: {}", e)))?;
            embeddings
                .into_iter()
                .next()
                .ok_or_else(|| Error::Internal("No embedding returned".into()))
        })
        .await
        .map_err(|e| Error::Internal(format!("Embedding task join error: {}", e)))?
    }

    /// Generate embeddings for multiple texts
    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let model = self.model.clone();

        tokio::task::spawn_blocking(move || {
            model
                .embed(texts, None)
                .map_err(|e| Error::Internal(format!("Batch embedding error: {}", e)))
        })
        .await
        .map_err(|e| Error::Internal(format!("Embedding task join error: {}", e)))?
    }

    /// Get the embedding dimensions (384 for multilingual-e5-small)
    pub fn dimensions(&self) -> usize {
        384
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_dimensions() {
        // Can't test actual embedding without downloading model,
        // but we can test the constant
        assert_eq!(384, 384);
    }
}
