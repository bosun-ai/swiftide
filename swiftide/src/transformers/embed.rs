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

        // EmbeddableTypes grouped by node stored in order of processed nodes.
        let mut embeddings_keys_groups = VecDeque::with_capacity(nodes.len());
        // Embeddable data of every node stored in order of processed nodes.
        let embeddables_data = nodes
            .iter_mut()
            .fold(Vec::new(), |mut embeddables_data, node| {
                let embeddables = node.as_embeddables();
                let mut embeddables_keys = Vec::with_capacity(embeddables.len());
                for (embeddable_key, embeddable_data) in embeddables.into_iter() {
                    embeddables_keys.push(embeddable_key);
                    embeddables_data.push(embeddable_data);
                }
                embeddings_keys_groups.push_back(embeddables_keys);
                embeddables_data
            });

        // Embeddings vectors of every node stored in order of processed nodes.
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

#[cfg(test)]
mod tests {
    use crate::ingestion::{EmbedMode, EmbeddableType, IngestionNode};
    use crate::{BatchableTransformer, MockEmbeddingModel};

    use super::Embed;

    use std::collections::HashMap;

    use futures_util::StreamExt;
    use mockall::predicate::*;
    use test_case::test_case;

    #[derive(Clone)]
    struct TestData<'a> {
        pub embed_mode: EmbedMode,
        pub chunk: &'a str,
        pub metadata: HashMap<&'a str, &'a str>,
        pub expected_embedables: Vec<&'a str>,
        pub expected_vectors: Vec<(EmbeddableType, Vec<f32>)>,
    }

    #[test_case(vec![
        TestData {
            embed_mode: EmbedMode::SingleWithMetadata,
            chunk: "chunk_1",
            metadata: HashMap::from([("meta_1", "prompt_1")]),
            expected_embedables: vec!["meta_1: prompt_1\nchunk_1"],
            expected_vectors: vec![(EmbeddableType::Combined, vec![1f32])]
        },
        TestData {
            embed_mode: EmbedMode::SingleWithMetadata,
            chunk: "chunk_2",
            metadata: HashMap::from([("meta_2", "prompt_2")]),
            expected_embedables: vec!["meta_2: prompt_2\nchunk_2"],
            expected_vectors: vec![(EmbeddableType::Combined, vec![2f32])]
        }
    ]; "Multiple nodes EmbedMode::SingleWithMetadata with metadata.")]
    #[test_case(vec![
        TestData {
            embed_mode: EmbedMode::PerField,
            chunk: "chunk_1",
            metadata: HashMap::from([("meta_1", "prompt_1")]),
            expected_embedables: vec!["chunk_1", "prompt_1"],
            expected_vectors: vec![
                (EmbeddableType::Chunk, vec![10f32]),
                (EmbeddableType::Metadata("meta_1".into()), vec![11f32])
            ]
        },
        TestData {
            embed_mode: EmbedMode::PerField,
            chunk: "chunk_2",
            metadata: HashMap::from([("meta_2", "prompt_2")]),
            expected_embedables: vec!["chunk_2", "prompt_2"],
            expected_vectors: vec![
                (EmbeddableType::Chunk, vec![20f32]),
                (EmbeddableType::Metadata("meta_2".into()), vec![21f32])
            ]
        }
    ]; "Multiple nodes EmbedMode::PerField with metadata. Metadata name skipped from Embeddable.")]
    #[test_case(vec![
        TestData {
            embed_mode: EmbedMode::Both,
            chunk: "chunk_1",
            metadata: HashMap::from([("meta_1", "prompt_1")]),
            expected_embedables: vec!["meta_1: prompt_1\nchunk_1", "chunk_1", "prompt_1"],
            expected_vectors: vec![
                (EmbeddableType::Combined, vec![10f32]),
                (EmbeddableType::Chunk, vec![11f32]),
                (EmbeddableType::Metadata("meta_1".into()), vec![12f32])
            ]
        },
        TestData {
            embed_mode: EmbedMode::Both,
            chunk: "chunk_2",
            metadata: HashMap::from([("meta_2", "prompt_2")]),
            expected_embedables: vec!["meta_2: prompt_2\nchunk_2", "chunk_2", "prompt_2"],
            expected_vectors: vec![
                (EmbeddableType::Combined, vec![20f32]),
                (EmbeddableType::Chunk, vec![21f32]),
                (EmbeddableType::Metadata("meta_2".into()), vec![22f32])
            ]
        }
    ]; "Multiple nodes EmbedMode::Both with metadata. Metadata name skipped from Embeddable.")]
    #[test_case(vec![]; "No nodes")]
    #[tokio::test]
    async fn batch_transform<'a>(test_data: Vec<TestData<'a>>) {
        let test_nodes: Vec<IngestionNode> = test_data
            .iter()
            .map(|data| IngestionNode {
                chunk: data.chunk.into(),
                metadata: data
                    .metadata
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                embed_mode: data.embed_mode,
                ..Default::default()
            })
            .collect();

        let expected_nodes: Vec<IngestionNode> = test_nodes
            .clone()
            .into_iter()
            .zip(test_data.iter())
            .map(|(mut expected_node, test_data)| {
                expected_node.vectors = Some(test_data.expected_vectors.iter().cloned().collect());
                expected_node
            })
            .collect();

        let expected_embeddables_batch = test_data
            .clone()
            .iter()
            .map(|d| &d.expected_embedables)
            .flatten()
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        let expected_vectors_batch: Vec<Vec<f32>> = test_data
            .clone()
            .iter()
            .map(|d| d.expected_vectors.iter().map(|(_, v)| v).cloned())
            .flatten()
            .collect();

        let mut model_mock = MockEmbeddingModel::new();
        model_mock
            .expect_embed()
            .withf(move |embeddables| expected_embeddables_batch.eq(embeddables))
            .times(1)
            .returning_st(move |_| Ok(expected_vectors_batch.clone()));

        let embed = Embed::new(model_mock);

        let mut stream = embed.batch_transform(test_nodes).await;

        for expected_node in expected_nodes {
            let ingested_node = stream
                .next()
                .await
                .expect("IngestionStream has same length as expected_nodes")
                .expect("Is OK");
            debug_assert_eq!(ingested_node, expected_node);
        }
    }
}
