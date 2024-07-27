//! `FastEmbed` integration for text embedding.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use fastembed::TextEmbedding;

use swiftide_core::{EmbeddingModel, Embeddings};

/// A wrapper around the `FastEmbed` library for text embedding.
///
/// Supports a variety of fast text embedding models. The default is the `Flag Embedding` model
/// with a dimension size of 384.
///
/// See the [FastEmbed documentation](https://docs.rs/fastembed) for more information on usage.
///
/// `FastEmbed` can be customized by setting the embedding model via the builder. The batch size can
/// also be set and is recommended. Batch size should match the batch size in the indexing
/// pipeline.
///
/// Node that the embedding vector dimensions need to match the dimensions of the vector database collection
///
/// Requires the `fastembed` feature to be enabled.
#[derive(Builder, Clone)]
#[builder(
    pattern = "owned",
    setter(strip_option),
    build_fn(error = "anyhow::Error")
)]
pub struct FastEmbed {
    #[builder(
        setter(custom),
        default = "TextEmbedding::try_new(Default::default())?.into()"
    )]
    embedding_model: Arc<TextEmbedding>,
    #[builder(default)]
    batch_size: Option<usize>,
}

impl std::fmt::Debug for FastEmbed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedBuilder")
            .field("batch_size", &self.batch_size)
            .finish()
    }
}

impl FastEmbed {
    /// Tries to build a default `FastEmbed` with `Flag Embedding`.
    ///
    /// # Errors
    ///
    /// Errors if the build fails
    pub fn try_default() -> Result<Self> {
        Self::builder().build()
    }

    pub fn builder() -> FastEmbedBuilder {
        FastEmbedBuilder::default()
    }
}

impl FastEmbedBuilder {
    #[must_use]
    pub fn embedding_model(mut self, fastembed: TextEmbedding) -> Self {
        self.embedding_model = Some(Arc::new(fastembed));

        self
    }
}

#[async_trait]
impl EmbeddingModel for FastEmbed {
    #[tracing::instrument(skip_all)]
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings> {
        self.embedding_model.embed(input, self.batch_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fastembed() {
        let fastembed = FastEmbed::try_default().unwrap();
        let embeddings = fastembed.embed(vec!["hello".to_string()]).await.unwrap();
        assert_eq!(embeddings.len(), 1);
    }
}
