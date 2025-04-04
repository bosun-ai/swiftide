//! Generic embedding transformer
use std::{collections::VecDeque, sync::Arc};

use anyhow::bail;
use async_trait::async_trait;
use swiftide_core::{
    indexing::{IndexingStream, Node},
    BatchableTransformer, EmbeddingModel, WithBatchIndexingDefaults, WithIndexingDefaults,
};

/// A transformer that can generate embeddings for an `Node`
///
/// This file defines the `Embed` struct and its implementation of the `BatchableTransformer` trait.
#[derive(Clone)]
pub struct Embed {
    model: Arc<dyn EmbeddingModel>,
    concurrency: Option<usize>,
    batch_size: Option<usize>,
}

impl std::fmt::Debug for Embed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embed")
            .field("concurrency", &self.concurrency)
            .field("batch_size", &self.batch_size)
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
            model: Arc::new(model),
            concurrency: None,
            batch_size: None,
        }
    }

    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    /// Sets the batch size for the transformer.
    /// If the batch size is not set, the transformer will use the default batch size set by the
    /// pipeline # Parameters
    ///
    /// * `batch_size` - The batch size to use for the transformer.
    ///
    /// # Returns
    ///
    /// A new instance of `Embed`.
    #[must_use]
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }
}

impl WithBatchIndexingDefaults for Embed {}
impl WithIndexingDefaults for Embed {}

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
    async fn batch_transform(&self, mut nodes: Vec<Node>) -> IndexingStream {
        // TODO: We should drop chunks that go over the token limit of the EmbedModel

        // EmbeddedFields grouped by node stored in order of processed nodes.
        let mut embeddings_keys_groups = VecDeque::with_capacity(nodes.len());
        // Embeddable data of every node stored in order of processed nodes.
        let embeddables_data = nodes
            .iter_mut()
            .fold(Vec::new(), |mut embeddables_data, node| {
                let embeddables = node.as_embeddables();
                let mut embeddables_keys = Vec::with_capacity(embeddables.len());
                for (embeddable_key, embeddable_data) in embeddables {
                    embeddables_keys.push(embeddable_key);
                    embeddables_data.push(embeddable_data);
                }
                embeddings_keys_groups.push_back(embeddables_keys);
                embeddables_data
            });

        // Embeddings vectors of every node stored in order of processed nodes.
        let mut embeddings = match self.model.embed(embeddables_data).await {
            Ok(embeddngs) => VecDeque::from(embeddngs),
            Err(err) => return IndexingStream::iter(vec![Err(err.into())]),
        };

        // Iterator of nodes with embeddings vectors map.
        let nodes_iter = nodes.into_iter().map(move |mut node| {
            let Some(embedding_keys) = embeddings_keys_groups.pop_front() else {
                bail!("Missing embedding data");
            };
            node.vectors = embedding_keys
                .into_iter()
                .map(|embedded_field| {
                    embeddings
                        .pop_front()
                        .map(|embedding| (embedded_field, embedding))
                })
                .collect();
            Ok(node)
        });

        IndexingStream::iter(nodes_iter)
    }

    fn concurrency(&self) -> Option<usize> {
        self.concurrency
    }

    fn batch_size(&self) -> Option<usize> {
        self.batch_size
    }
}

#[cfg(test)]
mod tests {
    use swiftide_core::indexing::{EmbedMode, EmbeddedField, Metadata, Node};
    use swiftide_core::{BatchableTransformer, MockEmbeddingModel};

    use super::Embed;

    use futures_util::StreamExt;
    use mockall::predicate::*;
    use test_case::test_case;

    use swiftide_core::chat_completion::errors::LanguageModelError;

    #[derive(Clone)]
    struct TestData<'a> {
        pub embed_mode: EmbedMode,
        pub chunk: &'a str,
        pub metadata: Metadata,
        pub expected_embedables: Vec<&'a str>,
        pub expected_vectors: Vec<(EmbeddedField, Vec<f32>)>,
    }

    #[test_case(vec![
        TestData {
            embed_mode: EmbedMode::SingleWithMetadata,
            chunk: "chunk_1",
            metadata: Metadata::from([("meta_1", "prompt_1")]),
            expected_embedables: vec!["meta_1: prompt_1\nchunk_1"],
            expected_vectors: vec![(EmbeddedField::Combined, vec![1f32])]
        },
        TestData {
            embed_mode: EmbedMode::SingleWithMetadata,
            chunk: "chunk_2",
            metadata: Metadata::from([("meta_2", "prompt_2")]),
            expected_embedables: vec!["meta_2: prompt_2\nchunk_2"],
            expected_vectors: vec![(EmbeddedField::Combined, vec![2f32])]
        }
    ]; "Multiple nodes EmbedMode::SingleWithMetadata with metadata.")]
    #[test_case(vec![
        TestData {
            embed_mode: EmbedMode::PerField,
            chunk: "chunk_1",
            metadata: Metadata::from([("meta_1", "prompt 1")]),
            expected_embedables: vec!["chunk_1", "prompt 1"],
            expected_vectors: vec![
                (EmbeddedField::Chunk, vec![10f32]),
                (EmbeddedField::Metadata("meta_1".into()), vec![11f32])
            ]
        },
        TestData {
            embed_mode: EmbedMode::PerField,
            chunk: "chunk_2",
            metadata: Metadata::from([("meta_2", "prompt 2")]),
            expected_embedables: vec!["chunk_2", "prompt 2"],
            expected_vectors: vec![
                (EmbeddedField::Chunk, vec![20f32]),
                (EmbeddedField::Metadata("meta_2".into()), vec![21f32])
            ]
        }
    ]; "Multiple nodes EmbedMode::PerField with metadata.")]
    #[test_case(vec![
        TestData {
            embed_mode: EmbedMode::Both,
            chunk: "chunk_1",
            metadata: Metadata::from([("meta_1", "prompt 1")]),
            expected_embedables: vec!["meta_1: prompt 1\nchunk_1", "chunk_1", "prompt 1"],
            expected_vectors: vec![
                (EmbeddedField::Combined, vec![10f32]),
                (EmbeddedField::Chunk, vec![11f32]),
                (EmbeddedField::Metadata("meta_1".into()), vec![12f32])
            ]
        },
        TestData {
            embed_mode: EmbedMode::Both,
            chunk: "chunk_2",
            metadata: Metadata::from([("meta_2", "prompt 2")]),
            expected_embedables: vec!["meta_2: prompt 2\nchunk_2", "chunk_2", "prompt 2"],
            expected_vectors: vec![
                (EmbeddedField::Combined, vec![20f32]),
                (EmbeddedField::Chunk, vec![21f32]),
                (EmbeddedField::Metadata("meta_2".into()), vec![22f32])
            ]
        }
    ]; "Multiple nodes EmbedMode::Both with metadata.")]
    #[test_case(vec![
        TestData {
            embed_mode: EmbedMode::Both,
            chunk: "chunk_1",
            metadata: Metadata::from([("meta_10", "prompt 10"), ("meta_11", "prompt 11"), ("meta_12", "prompt 12")]),
            expected_embedables: vec!["meta_10: prompt 10\nmeta_11: prompt 11\nmeta_12: prompt 12\nchunk_1", "chunk_1", "prompt 10", "prompt 11", "prompt 12"],
            expected_vectors: vec![
                (EmbeddedField::Combined, vec![10f32]),
                (EmbeddedField::Chunk, vec![11f32]),
                (EmbeddedField::Metadata("meta_10".into()), vec![12f32]),
                (EmbeddedField::Metadata("meta_11".into()), vec![13f32]),
                (EmbeddedField::Metadata("meta_12".into()), vec![14f32]),
            ]
        },
        TestData {
            embed_mode: EmbedMode::Both,
            chunk: "chunk_2",
            metadata: Metadata::from([("meta_20", "prompt 20"), ("meta_21", "prompt 21"), ("meta_22", "prompt 22")]),
            expected_embedables: vec!["meta_20: prompt 20\nmeta_21: prompt 21\nmeta_22: prompt 22\nchunk_2", "chunk_2", "prompt 20", "prompt 21", "prompt 22"],
            expected_vectors: vec![
                (EmbeddedField::Combined, vec![20f32]),
                (EmbeddedField::Chunk, vec![21f32]),
                (EmbeddedField::Metadata("meta_20".into()), vec![22f32]),
                (EmbeddedField::Metadata("meta_21".into()), vec![23f32]),
                (EmbeddedField::Metadata("meta_22".into()), vec![24f32])
            ]
        }
    ]; "Multiple nodes EmbedMode::Both with multiple metadata.")]
    #[test_case(vec![]; "No ingestion nodes")]
    #[tokio::test]
    async fn batch_transform(test_data: Vec<TestData<'_>>) {
        let test_nodes: Vec<Node> = test_data
            .iter()
            .map(|data| {
                Node::builder()
                    .chunk(data.chunk)
                    .metadata(data.metadata.clone())
                    .embed_mode(data.embed_mode)
                    .build()
                    .unwrap()
            })
            .collect();

        let expected_nodes: Vec<Node> = test_nodes
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
            .flat_map(|d| &d.expected_embedables)
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        let expected_vectors_batch: Vec<Vec<f32>> = test_data
            .clone()
            .iter()
            .flat_map(|d| d.expected_vectors.iter().map(|(_, v)| v).cloned())
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

    #[tokio::test]
    async fn test_returns_error_properly_if_embed_fails() {
        let test_nodes = vec![Node::new("chunk")];
        let mut model_mock = MockEmbeddingModel::new();
        model_mock
            .expect_embed()
            .times(1)
            .returning(|_| Err(LanguageModelError::PermanentError("error".into())));
        let embed = Embed::new(model_mock);
        let mut stream = embed.batch_transform(test_nodes).await;
        let error = stream
            .next()
            .await
            .expect("IngestionStream has same length as expected_nodes")
            .expect_err("Is Err");

        assert_eq!(error.to_string(), "Permanent error: error");
    }
}
