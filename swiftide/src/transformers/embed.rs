//! Generic embedding transformer
use std::sync::Arc;

use crate::{
    indexing::{IndexingStream, Node},
    BatchableTransformer, EmbeddingModel,
};
use async_trait::async_trait;
use itertools::Itertools as _;

/// A transformer that can generate embeddings for an `Node`
///
/// This file defines the `Embed` struct and its implementation of the `BatchableTransformer` trait.
pub struct Embed {
    embed_model: Arc<dyn EmbeddingModel>,
    concurrency: Option<usize>,
}

impl std::fmt::Debug for Embed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embed")
            .field("concurrency", &self.concurrency)
            .finish()
    }
}

impl Embed {
    /// Creates a new instance of the `Embed` transformer.
    ///
    /// # Parameters
    ///
    /// * `model` - An embedding model that implements the `EmbeddingModel` trait.
    ///
    /// # Returns
    ///
    /// A new instance of `Embed`.
    pub fn new(model: impl EmbeddingModel + 'static) -> Self {
        Self {
            embed_model: Arc::new(model),
            concurrency: None,
        }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }
}

#[async_trait]
impl BatchableTransformer for Embed {
    /// Transforms a batch of `Node` objects by generating embeddings for them.
    ///
    /// # Parameters
    ///
    /// * `nodes` - A vector of `Node` objects to be transformed.
    ///
    /// # Returns
    ///
    /// An `IndexingStream` containing the transformed `Node` objects with their embeddings.
    ///
    /// # Errors
    ///
    /// If the embedding process fails, the function returns a stream with the error.
    #[tracing::instrument(skip_all, name = "transformers.embed")]
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream {
        // TODO: We should drop chunks that go over the token limit of the EmbedModel
        let chunks_to_embed: Vec<String> = nodes.iter().map(|n| n.as_embeddable()).collect();

        self.embed_model
            .embed(chunks_to_embed)
            .await
            .map(|embeddings| {
                nodes
                    .into_iter()
                    // Will panic if the number of embeddings doesn't match the number of nodes
                    .zip_eq(embeddings)
                    .map(|(mut n, v)| {
                        n.vector = Some(v);
                        n
                    })
                    .collect::<Vec<_>>()
            })
            .into()
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}
