//! `FastEmbed` integration for text embedding.

use std::sync::Arc;

use anyhow::Result;
use derive_builder::Builder;
use fastembed::{SparseTextEmbedding, TextEmbedding};

pub use swiftide_core::EmbeddingModel as _;
pub use swiftide_core::SparseEmbeddingModel as _;

mod embedding_model;
mod rerank;
mod sparse_embedding_model;

pub use rerank::Rerank;

pub enum EmbeddingModelType {
    Dense(TextEmbedding),
    Sparse(SparseTextEmbedding),
}

impl From<TextEmbedding> for EmbeddingModelType {
    fn from(val: TextEmbedding) -> Self {
        EmbeddingModelType::Dense(val)
    }
}

impl From<SparseTextEmbedding> for EmbeddingModelType {
    fn from(val: SparseTextEmbedding) -> Self {
        EmbeddingModelType::Sparse(val)
    }
}

/// Default batch size for embedding
///
/// Matches the default batch size in [`fastembed`](https://docs.rs/fastembed)
const DEFAULT_BATCH_SIZE: usize = 256;

/// A wrapper around the `FastEmbed` library for text embedding.
///
/// Supports a variety of fast text embedding models. The default is the `Flag Embedding` model
/// with a dimension size of 384.
///
/// A default can also be used for sparse embeddings, which by default uses Splade. Sparse
/// embeddings are useful for more exact search in combination with dense vectors.
///
/// `Into` is implemented for all available models from fastembed-rs.
///
/// See the [FastEmbed documentation](https://docs.rs/fastembed) for more information on usage.
///
/// `FastEmbed` can be customized by setting the embedding model via the builder. The batch size can
/// also be set and is recommended. Batch size should match the batch size in the indexing
/// pipeline.
///
/// Note that the embedding vector dimensions need to match the dimensions of the vector database
/// collection
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
        default = "Arc::new(TextEmbedding::try_new(Default::default())?.into())"
    )]
    embedding_model: Arc<EmbeddingModelType>,
    #[builder(default = "Some(DEFAULT_BATCH_SIZE)")]
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

    /// Tries to build a default `FastEmbed` for sparse embeddings using Splade
    ///
    /// # Errors
    ///
    /// Errors if the build fails
    pub fn try_default_sparse() -> Result<Self> {
        Self::builder()
            .embedding_model(SparseTextEmbedding::try_new(
                fastembed::SparseInitOptions::default(),
            )?)
            .build()
    }

    pub fn builder() -> FastEmbedBuilder {
        FastEmbedBuilder::default()
    }
}

impl FastEmbedBuilder {
    #[must_use]
    pub fn embedding_model(mut self, fastembed: impl Into<EmbeddingModelType>) -> Self {
        self.embedding_model = Some(Arc::new(fastembed.into()));

        self
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

    #[tokio::test]
    async fn test_sparse_fastembed() {
        let fastembed = FastEmbed::try_default_sparse().unwrap();
        let embeddings = fastembed
            .sparse_embed(vec!["hello".to_string()])
            .await
            .unwrap();

        // Model can vary in size, assert it's small and not the full dictionary (30k+)
        assert!(embeddings[0].values.len() > 1);
        assert!(embeddings[0].values.len() < 100);
        assert_eq!(embeddings[0].indices.len(), embeddings[0].values.len());
    }
}
