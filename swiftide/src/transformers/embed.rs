//! Generic embedding transformer
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use crate::{
    ingestion::{EmbeddableType, IngestionNode, IngestionStream},
    BatchableTransformer, EmbeddingModel,
};
use anyhow::bail;
use async_trait::async_trait;

/// A transformer that can generate embeddings for an `IngestionNode`
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
    /// Transforms a batch of `IngestionNode` objects by generating embeddings for them.
    ///
    /// # Parameters
    ///
    /// * `nodes` - A vector of `IngestionNode` objects to be transformed.
    ///
    /// # Returns
    ///
    /// An `IngestionStream` containing the transformed `IngestionNode` objects with their embeddings.
    ///
    /// # Errors
    ///
    /// If the embedding process fails, the function returns a stream with the error.
    #[tracing::instrument(skip_all, name = "transformers.embed")]
    async fn batch_transform(&self, mut nodes: Vec<IngestionNode>) -> IngestionStream {
        // TODO: We should drop chunks that go over the token limit of the EmbedModel
        let mut embeddings_keys_groups = VecDeque::with_capacity(nodes.len());
        let embeddables_data =
            nodes
                .iter_mut()
                .fold(Vec::new(), |mut embeddables_data, mut node| {
                    let embeddables = node.embeddables();
                    let mut embeddables_keys = Vec::with_capacity(embeddables.len());
                    for (embeddable_key, embeddable_data) in embeddables.into_iter() {
                        embeddables_keys.push(embeddable_key);
                        embeddables_data.push(embeddable_data);
                    }
                    embeddings_keys_groups.push_back(embeddables_keys);
                    embeddables_data
                });

        let mut embeddings = match self.embed_model.embed(embeddables_data).await {
            Ok(embeddngs) => VecDeque::from(embeddngs),
            Err(err) => return IngestionStream::iter(Err(err)),
        };

        IngestionStream::iter(nodes.into_iter().map(move |mut node| {
            let Some(embedding_keys) = embeddings_keys_groups.pop_front() else {
                bail!("Missing embedding data");
            };
            let remaining_embeddings = embeddings.split_off(embedding_keys.len());
            let embedding_values = embeddings.clone();
            embeddings = remaining_embeddings;
            // TODO: handle different lengths
            let vectors: HashMap<EmbeddableType, Vec<f32>> =
                embedding_keys.into_iter().zip(embedding_values).collect();
            node.vectors = Some(vectors);
            Ok(node)
        }))
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }
}
