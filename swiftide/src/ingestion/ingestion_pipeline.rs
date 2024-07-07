use crate::{BatchableTransformer, ChunkerTransformer, Loader, NodeCache, Persist, Transformer};
use anyhow::Result;
use futures_util::{StreamExt, TryStreamExt};
use tokio::sync::mpsc;
use tracing::Instrument;

use std::{sync::Arc, time::Duration};

use super::{IngestionNode, IngestionStream};

/// A pipeline for ingesting files, adding metadata, chunking, transforming, embedding, and then storing them.
///
/// The `IngestionPipeline` struct orchestrates the entire file ingestion process. It is designed to be flexible and
/// performant, allowing for various stages of data transformation and storage to be configured and executed asynchronously.
///
/// # Fields
///
/// * `stream` - The stream of `IngestionNode` items to be processed.
/// * `storage` - Optional storage backend where the processed nodes will be stored.
/// * `concurrency` - The level of concurrency for processing nodes.
pub struct IngestionPipeline {
    stream: IngestionStream,
    storage: Vec<Arc<dyn Persist>>,
    concurrency: usize,
}

impl Default for IngestionPipeline {
    /// Creates a default `IngestionPipeline` with an empty stream, no storage, and a concurrency level equal to the number of CPUs.
    fn default() -> Self {
        Self {
            stream: IngestionStream::empty(),
            storage: Vec::default(),
            concurrency: num_cpus::get(),
        }
    }
}

impl IngestionPipeline {
    /// Creates an `IngestionPipeline` from a given loader.
    ///
    /// # Arguments
    ///
    /// * `loader` - A loader that implements the `Loader` trait.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` initialized with the provided loader.
    pub fn from_loader(loader: impl Loader + 'static) -> Self {
        let stream = loader.into_stream();
        Self {
            stream,
            ..Default::default()
        }
    }

    /// Creates an `IngestionPipeline` from a given stream.
    ///
    /// # Arguments
    ///
    /// * `stream` - An `IngestionStream` containing the nodes to be processed.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` initialized with the provided stream.
    pub fn from_stream(stream: impl Into<IngestionStream>) -> Self {
        Self {
            stream: stream.into(),
            ..Default::default()
        }
    }

    /// Sets the concurrency level for the pipeline.
    ///
    /// # Arguments
    ///
    /// * `concurrency` - The desired level of concurrency.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` with the updated concurrency level.
    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    /// Filters out cached nodes using the provided cache.
    ///
    /// # Arguments
    ///
    /// * `cache` - A cache that implements the `NodeCache` trait.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` with the updated stream that filters out cached nodes.
    #[must_use]
    pub fn filter_cached(mut self, cache: impl NodeCache + 'static) -> Self {
        let cache = Arc::new(cache);
        self.stream = self
            .stream
            .try_filter_map(move |node| {
                let cache = Arc::clone(&cache);
                let span =
                    tracing::trace_span!("filter_cached", node_cache = ?cache, node = ?node );
                async move {
                    if cache.get(&node).await {
                        tracing::debug!("Node in cache, skipping");
                        Ok(None)
                    } else {
                        cache.set(&node).await;
                        tracing::debug!("Node not in cache, passing through");
                        Ok(Some(node))
                    }
                }
                .instrument(span)
            })
            .boxed()
            .into();
        self
    }

    /// Adds a transformer to the pipeline.
    ///
    /// # Arguments
    ///
    /// * `transformer` - A transformer that implements the `Transformer` trait.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` with the updated stream that applies the transformer to each node.
    #[must_use]
    pub fn then(mut self, transformer: impl Transformer + 'static) -> Self {
        let concurrency = transformer.concurrency().unwrap_or(self.concurrency);
        let transformer = Arc::new(transformer);
        self.stream = self
            .stream
            .map_ok(move |node| {
                let transformer = transformer.clone();
                let span = tracing::trace_span!("then", node = ?node );

                async move { transformer.transform_node(node).await }.instrument(span)
            })
            .try_buffer_unordered(concurrency)
            .boxed()
            .into();

        self
    }

    /// Adds a batch transformer to the pipeline.
    ///
    /// # Arguments
    ///
    /// * `batch_size` - The size of the batches to be processed.
    /// * `transformer` - A transformer that implements the `BatchableTransformer` trait.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` with the updated stream that applies the batch transformer to each batch of nodes.
    #[must_use]
    pub fn then_in_batch(
        mut self,
        batch_size: usize,
        transformer: impl BatchableTransformer + 'static,
    ) -> Self {
        let transformer = Arc::new(transformer);
        let concurrency = transformer.concurrency().unwrap_or(self.concurrency);
        self.stream = self
            .stream
            .try_chunks(batch_size)
            .map_ok(move |nodes| {
                let transformer = Arc::clone(&transformer);
                let span = tracing::trace_span!("then_in_batch",  nodes = ?nodes );

                async move { Ok(transformer.batch_transform(nodes).await) }.instrument(span)
            })
            .try_buffer_unordered(concurrency) // First get the streams from each future
            .try_flatten_unordered(concurrency) // Then flatten all the streams back into one
            .boxed()
            .into();
        self
    }

    /// Adds a chunker transformer to the pipeline.
    ///
    /// # Arguments
    ///
    /// * `chunker` - A transformer that implements the `ChunkerTransformer` trait.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` with the updated stream that applies the chunker transformer to each node.
    #[must_use]
    pub fn then_chunk(mut self, chunker: impl ChunkerTransformer + 'static) -> Self {
        let chunker = Arc::new(chunker);
        let concurrency = chunker.concurrency().unwrap_or(self.concurrency);
        self.stream = self
            .stream
            .map_ok(move |node| {
                let chunker = Arc::clone(&chunker);
                let span = tracing::trace_span!("then_chunk", chunker = ?chunker, node = ?node );

                async move { Ok(chunker.transform_node(node).await) }.instrument(span)
            })
            .try_buffer_unordered(concurrency)
            .try_flatten_unordered(concurrency)
            .boxed()
            .into();

        self
    }

    /// Persists ingestion nodes using the provided storage backend.
    ///
    /// # Arguments
    ///
    /// * `storage` - A storage backend that implements the `Storage` trait.
    ///
    /// # Returns
    ///
    /// An instance of `IngestionPipeline` with the configured storage backend.
    ///
    /// # Panics
    ///
    /// Panics if batch size turns out to be not set and batch storage is still invoked.
    /// Pipeline only invokes batch storing if the batch size is set, so should be alright.
    #[must_use]
    pub fn then_store_with(mut self, storage: impl Persist + 'static) -> Self {
        let storage = Arc::new(storage);
        self.storage.push(storage.clone());
        // add storage to the stream instead of doing it at the end
        if storage.batch_size().is_some() {
            self.stream = self
                .stream
                .try_chunks(storage.batch_size().unwrap())
                .map_ok(move |nodes| {
                    let storage = Arc::clone(&storage);
                    let span = tracing::trace_span!("then_store_with_batched", storage = ?storage, nodes = ?nodes );

                    async move { Ok(storage.batch_store(nodes).await) }.instrument(span)
                })
                .try_buffer_unordered(self.concurrency)
                .try_flatten_unordered(self.concurrency)
                .boxed().into();
        } else {
            self.stream = self
                .stream
                .map_ok(move |node| {
                    let storage = Arc::clone(&storage);
                    let span =
                        tracing::trace_span!("then_store_with", storage = ?storage, node = ?node );

                    async move { storage.store(node).await }.instrument(span)
                })
                .try_buffer_unordered(self.concurrency)
                .boxed()
                .into();
        }

        self
    }

    /// Splits the stream into two streams based on a predicate.
    ///
    /// Note that this is not lazy. It will start consuming the stream immediately
    /// and send each item to the left or right stream based on the predicate.
    ///
    /// The other streams have a buffer, but should be started as soon as possible.
    /// The channels of the resulting streams are bounded and the parent stream will panic
    /// if sending fails.
    ///
    /// They can either be run concurrently, alternated between or merged back together.
    ///
    /// # Panics
    ///
    /// Panics if the receiving pipelines buffers are full or unavailable.
    #[must_use]
    pub fn split_by<P>(self, predicate: P) -> (Self, Self)
    where
        P: Fn(&Result<IngestionNode>) -> bool + Send + Sync + 'static,
    {
        let predicate = Arc::new(predicate);

        let (left_tx, left_rx) = mpsc::channel(1000);
        let (right_tx, right_rx) = mpsc::channel(1000);

        let stream = self.stream;
        let span = tracing::trace_span!("split_by");
        tokio::spawn(async move {
            stream
                .for_each(move |item| {
                    let predicate = Arc::clone(&predicate);
                    let left_tx = left_tx.clone();
                    let right_tx = right_tx.clone();
                    async move {
                        if predicate(&item) {
                            tracing::debug!(?item, "Sending to left stream");
                            left_tx
                                .send(item)
                                .await
                                .expect("Failed to send to left stream");
                        } else {
                            tracing::debug!(?item, "Sending to right stream");
                            right_tx
                                .send(item)
                                .await
                                .expect("Failed to send to right stream");
                        }
                    }
                })
                .instrument(span)
                .await;
        });

        let left_pipeline = Self {
            stream: left_rx.into(),
            storage: self.storage.clone(),
            concurrency: self.concurrency,
        };

        let right_pipeline = Self {
            stream: right_rx.into(),
            storage: self.storage.clone(),
            concurrency: self.concurrency,
        };

        (left_pipeline, right_pipeline)
    }

    /// Merges two streams into one
    ///
    /// This is useful for merging two streams that have been split using the `split_by` method.
    ///
    /// The full stream can then be processed using the `run` method.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        let stream = tokio_stream::StreamExt::merge(self.stream, other.stream);

        Self {
            stream: stream.boxed().into(),
            ..self
        }
    }

    /// Throttles the stream of nodes, limiting the rate to 1 per duration.
    ///
    /// Useful for rate limiting the ingestion pipeline. Uses `tokio_stream::StreamExt::throttle` internally which has a granualarity of 1ms.
    #[must_use]
    pub fn throttle(mut self, duration: impl Into<Duration>) -> Self {
        self.stream = tokio_stream::StreamExt::throttle(self.stream, duration.into())
            .boxed()
            .into();
        self
    }

    // Silently filters out errors encountered by the pipeline.
    //
    // This method filters out errors encountered by the pipeline, preventing them from bubbling up and terminating the stream.
    // Note that errors are not logged.
    #[must_use]
    pub fn filter_errors(mut self) -> Self {
        self.stream = self
            .stream
            .filter_map(|result| async {
                match result {
                    Ok(node) => Some(Ok(node)),
                    Err(_e) => None,
                }
            })
            .boxed()
            .into();
        self
    }

    /// Provide a closure to selectively filter nodes or errors
    ///
    /// This allows you to skip specific errors or nodes, or do ad hoc inspection.
    ///
    /// If the closure returns true, the result is kept, otherwise it is skipped.
    #[must_use]
    pub fn filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&Result<IngestionNode>) -> bool + Send + Sync + 'static,
    {
        self.stream = self
            .stream
            .filter(move |result| {
                let will_retain = filter(result);

                async move { will_retain }
            })
            .boxed()
            .into();
        self
    }

    /// Logs all results processed by the pipeline.
    ///
    /// This method logs all results processed by the pipeline at the `DEBUG` level.
    #[must_use]
    pub fn log_all(self) -> Self {
        self.log_errors().log_nodes()
    }

    /// Logs all errors encountered by the pipeline.
    ///
    /// This method logs all errors encountered by the pipeline at the `ERROR` level.
    #[must_use]
    pub fn log_errors(mut self) -> Self {
        self.stream = self
            .stream
            .inspect_err(|e| tracing::error!("Error processing node: {:?}", e))
            .boxed()
            .into();
        self
    }

    /// Logs all nodes processed by the pipeline.
    ///
    /// This method logs all nodes processed by the pipeline at the `DEBUG` level.
    #[must_use]
    pub fn log_nodes(mut self) -> Self {
        self.stream = self
            .stream
            .inspect_ok(|node| tracing::debug!("Processed node: {:?}", node))
            .boxed()
            .into();
        self
    }

    /// Runs the ingestion pipeline.
    ///
    /// This method processes the stream of nodes, applying all configured transformations and storing the results.
    ///
    /// # Returns
    ///
    /// A `Result` indicating the success or failure of the pipeline execution.
    ///
    /// # Errors
    ///
    /// Returns an error if no storage backend is configured or if any stage of the pipeline fails.
    #[tracing::instrument(skip_all, fields(total_nodes), name = "ingestion_pipeline.run")]
    pub async fn run(mut self) -> Result<()> {
        tracing::info!(
            "Starting ingestion pipeline with {} concurrency",
            self.concurrency
        );
        if self.storage.is_empty() {
            anyhow::bail!("No storage configured for ingestion pipeline");
        }

        // Ensure all storage backends are set up before processing nodes
        let setup_futures = self
            .storage
            .into_iter()
            .map(|storage| async move { storage.setup().await })
            .collect::<Vec<_>>();
        futures_util::future::try_join_all(setup_futures).await?;

        let mut total_nodes = 0;
        while self.stream.try_next().await?.is_some() {
            total_nodes += 1;
        }

        tracing::warn!("Processed {} nodes", total_nodes);
        tracing::Span::current().record("total_nodes", total_nodes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::ingestion::IngestionNode;
    use crate::persist::MemoryStorage;
    use crate::traits::*;
    use mockall::Sequence;

    /// Tests a simple run of the ingestion pipeline.
    #[test_log::test(tokio::test)]
    async fn test_simple_run() {
        let mut loader = MockLoader::new();
        let mut transformer = MockTransformer::new();
        let mut batch_transformer = MockBatchableTransformer::new();
        let mut chunker = MockChunkerTransformer::new();
        let mut storage = MockPersist::new();

        let mut seq = Sequence::new();

        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| vec![Ok(IngestionNode::default())].into());

        transformer.expect_transform_node().returning(|mut node| {
            node.chunk = "transformed".to_string();
            Ok(node)
        });
        transformer.expect_concurrency().returning(|| None);

        batch_transformer
            .expect_batch_transform()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|nodes| IngestionStream::iter(nodes.into_iter().map(Ok)));
        batch_transformer.expect_concurrency().returning(|| None);

        chunker
            .expect_transform_node()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|node| {
                let mut nodes = vec![];
                for i in 0..3 {
                    let mut node = node.clone();
                    node.chunk = format!("transformed_chunk_{i}");
                    nodes.push(Ok(node));
                }
                nodes.into()
            });
        chunker.expect_concurrency().returning(|| None);

        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);
        storage
            .expect_store()
            .times(3)
            .in_sequence(&mut seq)
            .withf(|node| node.chunk.starts_with("transformed_chunk_"))
            .returning(Ok);

        let pipeline = IngestionPipeline::from_loader(loader)
            .then(transformer)
            .then_in_batch(1, batch_transformer)
            .then_chunk(chunker)
            .then_store_with(storage);

        pipeline.run().await.unwrap();
    }

    #[tokio::test]
    async fn test_skipping_errors() {
        let mut loader = MockLoader::new();
        let mut transformer = MockTransformer::new();
        let mut storage = MockPersist::new();
        let mut seq = Sequence::new();
        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| vec![Ok(IngestionNode::default())].into());
        transformer
            .expect_transform_node()
            .returning(|_node| Err(anyhow::anyhow!("Error transforming node")));
        transformer.expect_concurrency().returning(|| None);
        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);
        storage.expect_store().times(0).returning(Ok);
        let pipeline = IngestionPipeline::from_loader(loader)
            .then(transformer)
            .then_store_with(storage)
            .filter_errors();
        pipeline.run().await.unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_calls_with_simple_transformer() {
        let mut loader = MockLoader::new();
        let mut transformer = MockTransformer::new();
        let mut storage = MockPersist::new();
        let mut seq = Sequence::new();
        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| {
                vec![
                    Ok(IngestionNode::default()),
                    Ok(IngestionNode::default()),
                    Ok(IngestionNode::default()),
                ]
                .into()
            });
        transformer
            .expect_transform_node()
            .times(3)
            .in_sequence(&mut seq)
            .returning(|mut node| {
                node.chunk = "transformed".to_string();
                Ok(node)
            });
        transformer.expect_concurrency().returning(|| Some(3));
        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);
        storage.expect_store().times(3).returning(Ok);

        let pipeline = IngestionPipeline::from_loader(loader)
            .then(transformer)
            .then_store_with(storage);
        pipeline.run().await.unwrap();
    }

    #[tokio::test]
    async fn test_arbitrary_closures_as_transformer() {
        let mut loader = MockLoader::new();
        let transformer = |node: IngestionNode| {
            let mut node = node;
            node.chunk = "transformed".to_string();
            Ok(node)
        };
        let storage = MemoryStorage::default();
        let mut seq = Sequence::new();
        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| vec![Ok(IngestionNode::default())].into());

        let pipeline = IngestionPipeline::from_loader(loader)
            .then(transformer)
            .then_store_with(storage.clone());
        pipeline.run().await.unwrap();

        dbg!(storage.clone());
        let processed_node = storage.get("0").await.unwrap();
        assert_eq!(processed_node.chunk, "transformed");
    }

    #[tokio::test]
    async fn test_arbitrary_closures_as_batch_transformer() {
        let mut loader = MockLoader::new();
        let batch_transformer = |nodes: Vec<IngestionNode>| {
            IngestionStream::iter(nodes.into_iter().map(|mut node| {
                node.chunk = "transformed".to_string();
                Ok(node)
            }))
        };
        let storage = MemoryStorage::default();
        let mut seq = Sequence::new();
        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| vec![Ok(IngestionNode::default())].into());

        let pipeline = IngestionPipeline::from_loader(loader)
            .then_in_batch(10, batch_transformer)
            .then_store_with(storage.clone());
        pipeline.run().await.unwrap();

        dbg!(storage.clone());
        let processed_node = storage.get("0").await.unwrap();
        assert_eq!(processed_node.chunk, "transformed");
    }

    #[tokio::test]
    async fn test_filter_closure() {
        let mut loader = MockLoader::new();
        let storage = MemoryStorage::default();
        let mut seq = Sequence::new();
        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| {
                vec![
                    Ok(IngestionNode::default()),
                    Ok(IngestionNode {
                        chunk: "skip".to_string(),
                        ..IngestionNode::default()
                    }),
                    Ok(IngestionNode::default()),
                ]
                .into()
            });
        let pipeline = IngestionPipeline::from_loader(loader)
            .filter(|result| {
                let node = result.as_ref().unwrap();
                node.chunk != "skip"
            })
            .then_store_with(storage.clone());
        pipeline.run().await.unwrap();
        let nodes = storage.get_all().await;
        assert_eq!(nodes.len(), 2);
    }

    #[test_log::test(tokio::test)]
    async fn test_split_and_merge() {
        let mut loader = MockLoader::new();
        let storage = MemoryStorage::default();
        let mut seq = Sequence::new();
        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| {
                vec![
                    Ok(IngestionNode::default()),
                    Ok(IngestionNode {
                        chunk: "will go left".to_string(),
                        ..IngestionNode::default()
                    }),
                    Ok(IngestionNode::default()),
                ]
                .into()
            });

        let pipeline = IngestionPipeline::from_loader(loader);
        let (mut left, mut right) = pipeline.split_by(|node| {
            if let Ok(node) = node {
                node.chunk.starts_with("will go left")
            } else {
                false
            }
        });

        // change the chunk to 'left'
        left = left
            .then(move |mut node: IngestionNode| {
                node.chunk = "left".to_string();

                Ok(node)
            })
            .log_all();

        right = right.then(move |mut node: IngestionNode| {
            node.chunk = "right".to_string();
            Ok(node)
        });

        left.merge(right)
            .then_store_with(storage.clone())
            .run()
            .await
            .unwrap();
        dbg!(storage.clone());

        let all_nodes = storage.get_all_values().await;
        assert_eq!(
            all_nodes.iter().filter(|node| node.chunk == "left").count(),
            1
        );
        assert_eq!(
            all_nodes
                .iter()
                .filter(|node| node.chunk == "right")
                .count(),
            2
        );
    }
}
