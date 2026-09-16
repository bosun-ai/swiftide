use anyhow::Result;
use futures_util::{StreamExt, TryFutureExt, TryStreamExt};
use swiftide_core::{
    BatchableTransformer, ChunkerTransformer, Loader, NodeCache, Persist, SimplePrompt,
    Transformer, WithBatchIndexingDefaults, WithIndexingDefaults,
    indexing::{Chunk, IndexingDefaults},
    statistics::StatsCollector,
};
use tokio::{
    sync::{Mutex, mpsc},
    task,
};
use tracing::Instrument;

use std::{
    collections::HashSet,
    pin::Pin,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use swiftide_core::indexing::{EmbedMode, IndexingStream, Node};
use uuid::Uuid;

macro_rules! trace_span {
    ($op:literal, $step:expr) => {
        tracing::trace_span!($op, "otel.name" = format!("{}.{}", $op, $step.name()),)
    };

    ($op:literal) => {
        tracing::trace_span!($op, "otel.name" = format!("{}", $op),)
    };
}

macro_rules! node_trace_log {
    ($step:expr, $node:expr, $msg:literal) => {
        tracing::trace!(
            node = ?$node,
            node_id = ?$node.id(),
            step = $step.name(),
            $msg
        )
    };
}

macro_rules! batch_node_trace_log {
    ($step:expr, $nodes:expr, $msg:literal) => {
        tracing::trace!(batch_size = $nodes.len(), nodes = ?$nodes, step = $step.name(), $msg)
    };
}

macro_rules! pipeline_with_new_stream {
    ($pipeline:expr, $stream:expr) => {
        Pipeline {
            stream: $stream.into(),
            storage_setup_fns: $pipeline.storage_setup_fns.clone(),
            concurrency: $pipeline.concurrency,
            indexing_defaults: $pipeline.indexing_defaults.clone(),
            batch_size: $pipeline.batch_size,
            stats: $pipeline.stats.clone(),
            node_caches: $pipeline.node_caches.clone(),
            failed_ids: $pipeline.failed_ids.clone(),
            passed_cache_ids: $pipeline.passed_cache_ids.clone(),
        }
    };
}

/// The default batch size for batch processing.
const DEFAULT_BATCH_SIZE: usize = 256;

/// A pipeline for indexing files, adding metadata, chunking, transforming, embedding, and then
/// storing them.
///
/// The `Pipeline` struct orchestrates the entire file indexing process. It is designed to be
/// flexible and performant, allowing for various stages of data transformation and storage to be
/// configured and executed asynchronously.
///
/// # Fields
///
/// * `stream` - The stream of `Node` items to be processed.
/// * `storage` - Optional storage backend where the processed nodes will be stored.
/// * `concurrency` - The level of concurrency for processing nodes.
/// * `stats` - Statistics collector for monitoring pipeline execution.
pub struct Pipeline<T: Chunk> {
    stream: IndexingStream<T>,
    // storage: Vec<Arc<dyn Persist<Input = T, Output = T>>>,
    storage_setup_fns: Vec<DynStorageSetupFn>,
    concurrency: usize,
    indexing_defaults: IndexingDefaults,
    batch_size: usize,
    stats: StatsCollector,
    node_caches: Vec<DynNodeCacheSet>,
    // Source ids (the id a node entered the pipeline with, kept in `parent_id`)
    // that had a failure anywhere in their fan-out. Populated by the pipeline
    // stages while nodes flow through them so failures survive `filter_errors`.
    failed_ids: Arc<FailedIds>,
    // Pairs of (cache registration pointer, source id) for nodes that passed
    // each cache's filter. `run` marks each source only in the caches that
    // actually processed it instead of every cache combined by `merge`.
    passed_cache_ids: Arc<PassedCacheIds>,
}

type DynStorageSetupFn =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

/// Marks a node id as processed in a `NodeCache`. Type erased so the cache registered in
/// [`Pipeline::filter_cached`] survives the node type changing across pipeline stages.
type DynNodeCacheSet =
    Arc<dyn Fn(uuid::Uuid) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Shared registry of source ids whose processing failed. Pipelines produced
/// by `split_by` share one registry; `merge` links independent registries so
/// [`Pipeline::run`] sees failures recorded by either side.
#[derive(Default)]
struct FailedIds {
    own: StdMutex<HashSet<Uuid>>,
    linked: StdMutex<Vec<Arc<FailedIds>>>,
}

impl FailedIds {
    fn record(&self, ids: &[Uuid]) {
        self.own
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(ids.iter().copied());
    }

    /// Whether `id` was recorded in this registry or in any registry linked
    /// into it by [`Pipeline::merge`].
    fn contains(id: &Uuid, root: &Arc<FailedIds>) -> bool {
        let mut visited = HashSet::new();
        let mut stack = vec![Arc::clone(root)];

        while let Some(node) = stack.pop() {
            let key = Arc::as_ptr(&node) as usize;
            if !visited.insert(key) {
                continue;
            }

            if node
                .own
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(id)
            {
                return true;
            }

            let linked = node
                .linked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            stack.extend(linked);
        }

        false
    }
}

/// Shared registry of cache-filter passes, keyed by the cache registration
/// pointer and the source id of the node that passed. Pipelines produced by
/// `split_by` share one registry; `merge` links independent registries so
/// [`Pipeline::run`] sees passes recorded by either side.
///
/// `run` marks each source id only in the caches whose filter it actually
/// passed. Without this scoping a merged pipeline would mark every completed
/// source in every cache, and a later run with a changed `split_by` predicate
/// could skip a node in a branch that never processed it.
#[derive(Default)]
struct PassedCacheIds {
    own: StdMutex<HashSet<(usize, Uuid)>>,
    linked: StdMutex<Vec<Arc<PassedCacheIds>>>,
}

impl PassedCacheIds {
    fn record(&self, cache_key: usize, id: Uuid) {
        self.own
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert((cache_key, id));
    }

    /// All `(cache registration, source id)` pairs recorded in this registry
    /// or in any registry linked into it by [`Pipeline::merge`].
    fn collect(root: &Arc<PassedCacheIds>) -> HashSet<(usize, Uuid)> {
        let mut visited = HashSet::new();
        let mut stack = vec![Arc::clone(root)];
        let mut passes = HashSet::new();

        while let Some(node) = stack.pop() {
            let key = Arc::as_ptr(&node) as usize;
            if !visited.insert(key) {
                continue;
            }

            passes.extend(
                node.own
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .iter()
                    .copied(),
            );

            let linked = node
                .linked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            stack.extend(linked);
        }

        passes
    }
}

/// Records the source ids of nodes whose processing failed. The ids are
/// checked by [`Pipeline::run`] when it decides which source nodes may be
/// marked as cached.
fn record_failed_ids(failed_ids: &Arc<FailedIds>, ids: &[Uuid]) {
    failed_ids.record(ids);
}

impl<T: Chunk> Default for Pipeline<T> {
    /// Creates a default `Pipeline` with an empty stream, no storage, and a concurrency level equal
    /// to the number of CPUs.
    fn default() -> Self {
        Self {
            stream: IndexingStream::<T>::empty(),
            storage_setup_fns: Vec::new(),
            concurrency: num_cpus::get(),
            indexing_defaults: IndexingDefaults::default(),
            batch_size: DEFAULT_BATCH_SIZE,
            stats: StatsCollector::new(),
            node_caches: Vec::new(),
            failed_ids: Arc::new(FailedIds::default()),
            passed_cache_ids: Arc::new(PassedCacheIds::default()),
        }
    }
}

impl<T: Chunk> Pipeline<T> {
    /// Creates a `Pipeline` from a given loader.
    ///
    /// # Arguments
    ///
    /// * `loader` - A loader that implements the `Loader` trait.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` initialized with the provided loader.
    pub fn from_loader(loader: impl Loader<Output = T> + 'static) -> Self {
        let stream = loader.into_stream();
        Self {
            stream,
            ..Default::default()
        }
    }

    /// Sets the default LLM client to be used for LLM prompts for all transformers in the
    /// pipeline.
    #[must_use]
    pub fn with_default_llm_client(mut self, client: impl SimplePrompt + 'static) -> Self {
        self.indexing_defaults = IndexingDefaults::from_simple_prompt(Box::new(client));
        self
    }

    /// Creates a `Pipeline` from a given stream.
    ///
    /// # Arguments
    ///
    /// * `stream` - An `IndexingStream` containing the nodes to be processed.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` initialized with the provided stream.
    pub fn from_stream(stream: impl Into<IndexingStream<T>>) -> Self {
        Self {
            stream: stream.into(),
            ..Default::default()
        }
    }

    /// Sets the concurrency level for the pipeline. By default the concurrency is set to the
    /// number of cpus.
    ///
    /// # Arguments
    ///
    /// * `concurrency` - The desired level of concurrency.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the updated concurrency level.
    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    /// Sets the embed mode for the pipeline. The embed mode controls what (combination) fields of a
    /// [`Node`] be embedded with a vector when transforming with [`crate::transformers::Embed`]
    ///
    /// See also [`swiftide_core::indexing::EmbedMode`].
    ///
    /// # Arguments
    ///
    /// * `embed_mode` - The desired embed mode.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the updated embed mode.
    #[must_use]
    pub fn with_embed_mode(mut self, embed_mode: EmbedMode) -> Self {
        self.stream = self
            .stream
            .map_ok(move |mut node| {
                node.embed_mode = embed_mode;
                node
            })
            .boxed()
            .into();
        self
    }

    /// Filters out cached nodes using the provided cache.
    ///
    /// Nodes are only marked in the cache once they have made it through the whole pipeline, so
    /// a node that fails partway is retried on the next run instead of being skipped.
    ///
    /// # Arguments
    ///
    /// * `cache` - A cache that implements the `NodeCache` trait.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the updated stream that filters out cached nodes.
    #[must_use]
    pub fn filter_cached(mut self, cache: impl NodeCache<Input = T> + 'static) -> Self {
        let cache = Arc::new(cache);

        let mark_cached: DynNodeCacheSet = Arc::new({
            let cache = Arc::clone(&cache);
            move |id: uuid::Uuid| {
                let cache = Arc::clone(&cache);
                Box::pin(async move { cache.set_by_id(id).await })
                    as Pin<Box<dyn Future<Output = ()> + Send>>
            }
        });
        let cache_key = Arc::as_ptr(&mark_cached).cast::<()>() as usize;
        self.node_caches.push(mark_cached);

        let passed_cache_ids = Arc::clone(&self.passed_cache_ids);

        self.stream = self
            .stream
            .try_filter_map(move |node| {
                let cache = Arc::clone(&cache);
                let passed_cache_ids = Arc::clone(&passed_cache_ids);
                let span = trace_span!("filter_cached", cache);

                async move {
                    if cache.get(&node).await {
                        node_trace_log!(cache, node, "node in cache, skipping");
                        Ok(None)
                    } else {
                        node_trace_log!(cache, node, "node not in cache, processing");
                        let id = node.parent_id.unwrap_or_else(|| node.id());
                        passed_cache_ids.record(cache_key, id);
                        Ok(Some(node))
                    }
                }
                .instrument(span.or_current())
            })
            .boxed()
            .into();
        self
    }

    /// Adds a transformer to the pipeline.
    ///
    /// Closures can also be provided as transformers.
    ///
    /// # Arguments
    ///
    /// * `transformer` - A transformer that implements the `Transformer` trait.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the updated stream that applies the transformer to each node.
    #[must_use]
    pub fn then<Output: Chunk>(
        self,
        mut transformer: impl Transformer<Input = T, Output = Output> + WithIndexingDefaults + 'static,
    ) -> Pipeline<Output> {
        let concurrency = transformer.concurrency().unwrap_or(self.concurrency);

        transformer.with_indexing_defaults(self.indexing_defaults.clone());

        let transformer = Arc::new(transformer);
        let failed_ids = self.failed_ids.clone();
        let stream = self
            .stream
            .map_ok(move |node| {
                let transformer = transformer.clone();
                let failed_ids = failed_ids.clone();
                let span = trace_span!("then", transformer);

                task::spawn(
                    async move {
                        node_trace_log!(transformer, node, "Transforming node");

                        let id = node.parent_id.unwrap_or_else(|| node.id());
                        let result = transformer.transform_node(node).await;
                        if result.is_err() {
                            record_failed_ids(&failed_ids, &[id]);
                        }
                        result
                    }
                    .instrument(span.or_current()),
                )
                .err_into::<anyhow::Error>()
            })
            .try_buffer_unordered(concurrency)
            .map(|x| x.and_then(|x| x));

        pipeline_with_new_stream!(self, stream.boxed())
    }

    /// Adds a batch transformer to the pipeline.
    ///
    /// If the transformer has a batch size set, the batch size from the transformer is used,
    /// otherwise the pipeline default batch size ([`DEFAULT_BATCH_SIZE`]).
    ///
    /// # Arguments
    ///
    /// * `transformer` - A transformer that implements the `BatchableTransformer` trait.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the updated stream that applies the batch transformer to each
    /// batch of nodes.
    #[must_use]
    pub fn then_in_batch<Output: Chunk>(
        self,
        mut transformer: impl BatchableTransformer<Input = T, Output = Output>
        + WithBatchIndexingDefaults
        + 'static,
    ) -> Pipeline<Output> {
        let concurrency = transformer.concurrency().unwrap_or(self.concurrency);

        transformer.with_indexing_defaults(self.indexing_defaults.clone());

        let transformer = Arc::new(transformer);
        let failed_ids = self.failed_ids.clone();
        let stream = self
            .stream
            .try_chunks(transformer.batch_size().unwrap_or(self.batch_size))
            .map_ok(move |nodes| {
                let transformer = Arc::clone(&transformer);
                let failed_ids = failed_ids.clone();
                let span = trace_span!("then_in_batch", transformer);

                let parent_ids: Vec<Uuid> = nodes
                    .iter()
                    .map(|node| node.parent_id.unwrap_or_else(|| node.id()))
                    .collect();

                tokio::spawn(
                    async move {
                        batch_node_trace_log!(transformer, nodes, "batch transforming nodes");

                        transformer
                            .batch_transform(nodes)
                            .await
                            .inspect(move |item| {
                                if item.is_err() {
                                    record_failed_ids(&failed_ids, &parent_ids);
                                }
                            })
                    }
                    .instrument(span.or_current()),
                )
                .map_err(anyhow::Error::from)
            })
            .err_into::<anyhow::Error>()
            .try_buffer_unordered(concurrency) // First get the streams from each future
            .try_flatten_unordered(None) // Then flatten the streams into a single stream
            .boxed();

        pipeline_with_new_stream!(self, stream)
    }

    /// Adds a chunker transformer to the pipeline.
    ///
    /// # Arguments
    ///
    /// * `chunker` - A transformer that implements the `ChunkerTransformer` trait.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the updated stream that applies the chunker transformer to
    /// each node.
    #[must_use]
    pub fn then_chunk<Output: Chunk>(
        self,
        chunker: impl ChunkerTransformer<Input = T, Output = Output> + 'static,
    ) -> Pipeline<Output> {
        let chunker = Arc::new(chunker);
        let concurrency = chunker.concurrency().unwrap_or(self.concurrency);
        let failed_ids = self.failed_ids.clone();
        let stream = self
            .stream
            .map_ok(move |node| {
                let chunker = Arc::clone(&chunker);
                let failed_ids = failed_ids.clone();
                let span = trace_span!("then_chunk", chunker);

                tokio::spawn(
                    async move {
                        node_trace_log!(chunker, node, "Chunking node");

                        let id = node.parent_id.unwrap_or_else(|| node.id());
                        chunker.transform_node(node).await.inspect(move |item| {
                            if item.is_err() {
                                record_failed_ids(&failed_ids, &[id]);
                            }
                        })
                    }
                    .instrument(span.or_current()),
                )
                .map_err(anyhow::Error::from)
            })
            .err_into::<anyhow::Error>()
            .try_buffer_unordered(concurrency)
            .try_flatten_unordered(None);

        pipeline_with_new_stream!(self, stream.boxed())
    }

    /// Transforms and expands a single node into many nodes
    ///
    /// Sementacially identical to `then_chunk` and repurposes the `ChunkerTransformer` trait.
    ///
    /// The real difference is in communicating intent and the trace/span names.
    ///
    /// # Arguments
    ///
    /// * `transformer` - A transformer that implements the `ChunkerTransformer` trait.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the updated stream that applies the chunker transformer to
    /// each node.
    #[must_use]
    pub fn then_expand<Output: Chunk>(
        self,
        transformer: impl ChunkerTransformer<Input = T, Output = Output> + 'static,
    ) -> Pipeline<Output> {
        let chunker = Arc::new(transformer);
        let concurrency = chunker.concurrency().unwrap_or(self.concurrency);
        let failed_ids = self.failed_ids.clone();
        let stream = self
            .stream
            .map_ok(move |node| {
                let chunker = Arc::clone(&chunker);
                let failed_ids = failed_ids.clone();
                let span = trace_span!("then_expand", chunker);

                tokio::spawn(
                    async move {
                        node_trace_log!(chunker, node, "Expanding node");

                        let id = node.parent_id.unwrap_or_else(|| node.id());
                        chunker.transform_node(node).await.inspect(move |item| {
                            if item.is_err() {
                                record_failed_ids(&failed_ids, &[id]);
                            }
                        })
                    }
                    .instrument(span.or_current()),
                )
                .map_err(anyhow::Error::from)
            })
            .err_into::<anyhow::Error>()
            .try_buffer_unordered(concurrency)
            .try_flatten_unordered(None);

        pipeline_with_new_stream!(self, stream.boxed())
    }

    /// Persists indexing nodes using the provided storage backend.
    ///
    /// # Arguments
    ///
    /// * `storage` - A storage backend that implements the `Storage` trait.
    ///
    /// # Returns
    ///
    /// An instance of `Pipeline` with the configured storage backend.
    ///
    /// # Panics
    ///
    /// Panics if batch size turns out to be not set and batch storage is still invoked.
    /// Pipeline only invokes batch storing if the batch size is set, so should be alright.
    #[must_use]
    pub fn then_store_with<Output: Chunk>(
        mut self,
        storage: impl Persist<Input = T, Output = Output> + 'static,
    ) -> Pipeline<Output> {
        let storage = Arc::new(storage);

        let storage_closure = storage.clone();

        // Ensure we run the setup function only once.
        let completed = Arc::new(Mutex::new(false));
        let setup_fn: DynStorageSetupFn = Arc::new(move || {
            let completed = Arc::clone(&completed);
            let storage_closure = Arc::clone(&storage_closure);
            Box::pin(async move {
                let mut lock = completed.lock().await;

                tracing::trace!(?storage_closure, "Setting up storage");
                storage_closure.setup().await?;
                *lock = true;
                Ok(())
            })
        });
        self.storage_setup_fns.push(setup_fn);

        // add storage to the stream instead of doing it at the end
        let failed_ids = self.failed_ids.clone();
        let stream = if storage.batch_size().is_some() {
            self.stream
                .try_chunks(storage.batch_size().unwrap())
                .map_ok(move |nodes| {
                    let storage = Arc::clone(&storage);
                    let failed_ids = failed_ids.clone();
                    let span = trace_span!("then_store_with_batched", storage);

                    let parent_ids: Vec<Uuid> = nodes
                        .iter()
                        .map(|node| node.parent_id.unwrap_or_else(|| node.id()))
                        .collect();

                    tokio::spawn(
                        async move {
                            batch_node_trace_log!(storage, nodes, "batch storing nodes");

                            storage.batch_store(nodes).await.inspect(move |item| {
                                if item.is_err() {
                                    record_failed_ids(&failed_ids, &parent_ids);
                                }
                            })
                        }
                        .instrument(span.or_current()),
                    )
                    .map_err(anyhow::Error::from)
                })
                .err_into::<anyhow::Error>()
                .try_buffer_unordered(self.concurrency)
                .try_flatten_unordered(None)
                .boxed()
        } else {
            self.stream
                .map_ok(move |node| {
                    let storage = Arc::clone(&storage);
                    let failed_ids = failed_ids.clone();
                    let span = trace_span!("then_store_with", storage);

                    tokio::spawn(
                        async move {
                            node_trace_log!(storage, node, "Storing node");

                            let id = node.parent_id.unwrap_or_else(|| node.id());
                            let result = storage.store(node).await;
                            if result.is_err() {
                                record_failed_ids(&failed_ids, &[id]);
                            }
                            result
                        }
                        .instrument(span.or_current()),
                    )
                    .err_into::<anyhow::Error>()
                })
                .try_buffer_unordered(self.concurrency)
                .map(|x| x.and_then(|x| x))
                .boxed()
        };

        pipeline_with_new_stream!(self, stream)
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
        P: Fn(&Result<Node<T>>) -> bool + Send + Sync + 'static,
    {
        let predicate = Arc::new(predicate);

        let (left_tx, left_rx) = mpsc::channel(1000);
        let (right_tx, right_rx) = mpsc::channel(1000);

        let stream = self.stream;
        let span = trace_span!("split_by");
        tokio::spawn(
            async move {
                stream
                    .for_each_concurrent(self.concurrency, move |item| {
                        let predicate = Arc::clone(&predicate);
                        let left_tx = left_tx.clone();
                        let right_tx = right_tx.clone();
                        async move {
                            if predicate(&item) {
                                tracing::trace!(?item, "Sending to left stream");
                                left_tx
                                    .send(item)
                                    .await
                                    .expect("Failed to send to left stream");
                            } else {
                                tracing::trace!(?item, "Sending to right stream");
                                right_tx
                                    .send(item)
                                    .await
                                    .expect("Failed to send to right stream");
                            }
                        }
                    })
                    .await;
            }
            .instrument(span.or_current()),
        );

        let left_pipeline = pipeline_with_new_stream!(self, left_rx);

        let right_pipeline = pipeline_with_new_stream!(self, right_rx);

        (left_pipeline, right_pipeline)
    }

    /// Merges two streams into one
    ///
    /// This is useful for merging two streams that have been split using the `split_by` method.
    ///
    /// The full stream can then be processed using the `run` method.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        let stream = tokio_stream::StreamExt::merge(self.stream, other.stream);

        // Combine the cache registrations of both pipelines so nodes that
        // complete in either stream mark all caches. Pipelines produced by
        // `split_by` share the same `Arc` allocations for registrations they
        // inherited, so pointer equality dedupes those.
        for cache in other.node_caches {
            if !self
                .node_caches
                .iter()
                .any(|existing| Arc::ptr_eq(existing, &cache))
            {
                self.node_caches.push(cache);
            }
        }

        // Link the failure registries so `run` sees failures recorded by
        // either side. Pipelines produced by `split_by` already share one
        // registry; skip the link in that case.
        if !Arc::ptr_eq(&self.failed_ids, &other.failed_ids) {
            self.failed_ids
                .linked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(other.failed_ids);
        }

        // Link the cache-pass registries so `run` sees which caches processed
        // each node. Pipelines produced by `split_by` already share one
        // registry; skip the link in that case.
        if !Arc::ptr_eq(&self.passed_cache_ids, &other.passed_cache_ids) {
            self.passed_cache_ids
                .linked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(other.passed_cache_ids);
        }

        Self {
            stream: stream.boxed().into(),
            ..self
        }
    }

    /// Throttles the stream of nodes, limiting the rate to 1 per duration.
    ///
    /// Useful for rate limiting the indexing pipeline. Uses `tokio_stream::StreamExt::throttle`
    /// internally which has a granularity of 1ms.
    #[must_use]
    pub fn throttle(mut self, duration: impl Into<Duration>) -> Self {
        self.stream = tokio_stream::StreamExt::throttle(self.stream, duration.into())
            .boxed()
            .into();
        self
    }

    // Silently filters out errors encountered by the pipeline.
    //
    // This method filters out errors encountered by the pipeline, preventing them from bubbling up
    // and terminating the stream. Note that errors are not logged.
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
        F: Fn(&Result<Node<T>>) -> bool + Send + Sync + 'static,
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

    /// Returns a snapshot of the current pipeline statistics
    ///
    /// This method provides real-time access to pipeline statistics during and after
    /// execution. The returned statistics include node counts, token usage, and timing
    /// information.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let pipeline = Pipeline::from_loader(loader).then(transformer);
    ///
    /// // During or after execution
    /// let stats = pipeline.stats();
    /// println!("Processed {} nodes", stats.nodes_processed);
    /// ```
    #[must_use]
    pub fn stats(&self) -> swiftide_core::statistics::PipelineStats {
        self.stats.get_stats()
    }

    /// Returns a reference to the statistics collector
    ///
    /// This provides direct access to the `StatsCollector` for recording additional
    /// metrics or for use by transformers that need to report their own statistics.
    #[must_use]
    pub fn stats_collector(&self) -> &StatsCollector {
        &self.stats
    }

    /// Logs all errors encountered by the pipeline.
    ///
    /// This method logs all errors encountered by the pipeline at the `ERROR` level.
    #[must_use]
    pub fn log_errors(mut self) -> Self {
        self.stream = self
            .stream
            .inspect_err(|e| tracing::error!(?e, "Error processing node"))
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
            .inspect_ok(|node| tracing::debug!(?node, "Processed node: {:?}", node))
            .boxed()
            .into();
        self
    }

    /// Runs the indexing pipeline.
    ///
    /// This method processes the stream of nodes, applying all configured transformations and
    /// storing the results.
    ///
    /// # Returns
    ///
    /// A `Result` indicating the success or failure of the pipeline execution.
    ///
    /// # Errors
    ///
    /// Returns an error if no storage backend is configured or if any stage of the pipeline fails.
    #[tracing::instrument(skip_all, fields(total_nodes), name = "indexing_pipeline.run")]
    pub async fn run(mut self) -> Result<()> {
        self.stats.start();

        tracing::info!(
            "Starting indexing pipeline with {} concurrency",
            self.concurrency
        );

        // Ensure all storage backends are set up before processing nodes
        let setup_futures = self
            .storage_setup_fns
            .into_iter()
            .map(|func| async move { func().await })
            .collect::<Vec<_>>();
        futures_util::future::try_join_all(setup_futures).await?;

        let mut total_nodes = 0u64;
        let mut completed_ids = HashSet::new();

        while let Some(node) = self.stream.try_next().await? {
            total_nodes += 1;
            // Count successful nodes as stored (nodes that reach the end of the stream)
            self.stats.increment_nodes_stored(1);

            // Nodes reaching the end of the stream made it through the whole
            // pipeline, but a source node is only marked as cached once every
            // child it produced completed successfully. Collect the
            // candidates now and mark them only after the stream finished
            // without errors. Chunked nodes share the id of the node that
            // entered the pipeline, so each is cached once.
            if !self.node_caches.is_empty() {
                completed_ids.insert(node.parent_id.unwrap_or_else(|| node.id()));
            }
        }

        // The stream completed without errors; only now mark the parents in
        // the caches. Parents that had a failure recorded anywhere in their
        // fan-out are skipped so their failed chunks are retried on the next
        // run.
        if !self.node_caches.is_empty() {
            // Each source id is marked only in the caches whose filter it
            // actually passed. Marking every cache combined by `merge` would
            // let one branch skip a node another branch processed when the
            // `split_by` predicate changes between runs.
            let passed = PassedCacheIds::collect(&self.passed_cache_ids);

            let to_mark: Vec<(DynNodeCacheSet, Uuid)> = completed_ids
                .into_iter()
                .filter(|id| !FailedIds::contains(id, &self.failed_ids))
                .flat_map(|id| {
                    let passed = &passed;
                    self.node_caches
                        .iter()
                        .cloned()
                        .filter_map(move |mark_cached| {
                            let cache_key = Arc::as_ptr(&mark_cached).cast::<()>() as usize;
                            passed
                                .contains(&(cache_key, id))
                                .then_some((mark_cached, id))
                        })
                })
                .collect();

            // Bound the marking work to the pipeline's concurrency so a large
            // corpus does not issue one serial round-trip per source id.
            futures_util::stream::iter(
                to_mark.into_iter().map(|(mark_cached, id)| mark_cached(id)),
            )
            .buffer_unordered(self.concurrency)
            .collect::<Vec<()>>()
            .await;
        }

        self.stats.increment_nodes_processed(total_nodes);
        self.stats.complete();

        let stats = self.stats.get_stats();
        let elapsed = stats.duration();

        if let Some(duration) = elapsed {
            let elapsed_secs = duration.as_secs_f64();
            let nodes_per_sec = stats.nodes_per_second().unwrap_or(0.0);

            tracing::info!(
                nodes_processed = total_nodes,
                nodes_stored = stats.nodes_stored,
                total_tokens = stats.total_tokens(),
                total_requests = stats.total_requests(),
                elapsed_secs,
                nodes_per_sec,
                "Pipeline completed"
            );
        }

        tracing::Span::current().record("total_nodes", total_nodes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::persist::MemoryStorage;
    use mockall::Sequence;
    use swiftide_core::indexing::*;

    /// Tests a simple run of the indexing pipeline.
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
            .returning(|| vec![Ok(Node::default())].into());

        transformer.expect_transform_node().returning(|mut node| {
            node.chunk = "transformed".to_string();
            Ok(node)
        });
        transformer.expect_concurrency().returning(|| None);
        transformer.expect_name().returning(|| "transformer");

        batch_transformer
            .expect_batch_transform()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|nodes| IndexingStream::iter(nodes.into_iter().map(Ok)));
        batch_transformer.expect_concurrency().returning(|| None);
        batch_transformer.expect_name().returning(|| "transformer");
        batch_transformer.expect_batch_size().returning(|| None);

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
        chunker.expect_name().returning(|| "chunker");

        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);
        storage
            .expect_store()
            .times(3)
            .in_sequence(&mut seq)
            .withf(|node| node.chunk.starts_with("transformed_chunk_"))
            .returning(Ok);
        storage.expect_name().returning(|| "storage");

        let pipeline = Pipeline::from_loader(loader)
            .then(transformer)
            .then_in_batch(batch_transformer)
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
            .returning(|| vec![Ok(Node::default())].into());
        transformer
            .expect_transform_node()
            .returning(|_node| Err(anyhow::anyhow!("Error transforming node")));
        transformer.expect_concurrency().returning(|| None);
        transformer.expect_name().returning(|| "mock");
        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);
        storage.expect_store().times(0).returning(Ok);
        let pipeline = Pipeline::from_loader(loader)
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
                    Ok(Node::default()),
                    Ok(Node::default()),
                    Ok(Node::default()),
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
        transformer.expect_name().returning(|| "transformer");
        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);
        storage.expect_store().times(3).returning(Ok);
        storage.expect_name().returning(|| "storage");

        let pipeline = Pipeline::from_loader(loader)
            .then(transformer)
            .then_store_with(storage);
        pipeline.run().await.unwrap();
    }

    #[tokio::test]
    async fn test_arbitrary_closures_as_transformer() {
        let mut loader = MockLoader::new();
        let transformer = |node: TextNode| {
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
            .returning(|| vec![Ok(TextNode::default())].into());

        let pipeline = Pipeline::from_loader(loader)
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
        let batch_transformer = |nodes: Vec<TextNode>| {
            IndexingStream::iter(nodes.into_iter().map(|mut node| {
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
            .returning(|| vec![Ok(TextNode::default())].into());

        let pipeline = Pipeline::from_loader(loader)
            .then_in_batch(batch_transformer)
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
                    Ok(TextNode::default()),
                    Ok(TextNode::new("skip")),
                    Ok(TextNode::default()),
                ]
                .into()
            });
        let pipeline = Pipeline::from_loader(loader)
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
                    Ok(TextNode::default()),
                    Ok(TextNode::new("will go left")),
                    Ok(TextNode::default()),
                ]
                .into()
            });

        let pipeline = Pipeline::from_loader(loader);
        let (mut left, mut right) = pipeline.split_by(|node| {
            if let Ok(node) = node {
                node.chunk.starts_with("will go left")
            } else {
                false
            }
        });

        // change the chunk to 'left'
        left = left
            .then(move |mut node: TextNode| {
                node.chunk = "left".to_string();

                Ok(node)
            })
            .log_all();

        right = right.then(move |mut node: TextNode| {
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

    #[tokio::test]
    async fn test_all_steps_should_work_as_dyn_box() {
        let mut loader = MockLoader::new();
        loader
            .expect_into_stream_boxed()
            .returning(|| vec![Ok(TextNode::default())].into());

        let mut transformer = MockTransformer::new();
        transformer.expect_transform_node().returning(Ok);
        transformer.expect_concurrency().returning(|| None);
        transformer.expect_name().returning(|| "mock");

        let mut batch_transformer = MockBatchableTransformer::new();
        batch_transformer
            .expect_batch_transform()
            .returning(std::convert::Into::into);
        batch_transformer.expect_concurrency().returning(|| None);
        batch_transformer.expect_name().returning(|| "mock");
        let mut chunker = MockChunkerTransformer::new();
        chunker
            .expect_transform_node()
            .returning(|node| vec![node].into());
        chunker.expect_concurrency().returning(|| None);
        chunker.expect_name().returning(|| "mock");

        let mut storage = MockPersist::new();
        storage.expect_setup().returning(|| Ok(()));
        storage.expect_store().returning(Ok);
        storage.expect_batch_size().returning(|| None);
        storage.expect_name().returning(|| "mock");

        let pipeline = Pipeline::from_loader(Box::new(loader) as Box<dyn Loader<Output = String>>)
            .then(Box::new(transformer) as Box<dyn Transformer<Input = String, Output = String>>)
            .then_in_batch(Box::new(batch_transformer) as Box<dyn BatchableTransformer<Input = String, Output = String>>)
            .then_chunk(Box::new(chunker) as Box<dyn ChunkerTransformer<Input = String, Output = String>>)
            .then_store_with(Box::new(storage) as Box<dyn Persist<Input = String, Output = String>>);
        pipeline.run().await.unwrap();
    }

    #[test_log::test(tokio::test)]
    async fn test_pipeline_statistics() {
        let mut loader = MockLoader::new();
        let mut storage = MockPersist::new();
        let mut seq = Sequence::new();

        loader
            .expect_into_stream()
            .times(1)
            .in_sequence(&mut seq)
            .returning(|| {
                vec![
                    Ok(TextNode::default()),
                    Ok(TextNode::default()),
                    Ok(TextNode::default()),
                ]
                .into()
            });

        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);
        storage.expect_store().times(3).returning(Ok);
        storage.expect_name().returning(|| "storage");

        let pipeline = Pipeline::from_loader(loader).then_store_with(storage);

        // Test that we can access stats before running
        let initial_stats = pipeline.stats();
        assert_eq!(initial_stats.nodes_processed, 0);
        assert_eq!(initial_stats.nodes_stored, 0);

        pipeline.run().await.unwrap();

        // After running, stats should be updated (access via the moved pipeline would not work,
        // but we verify the internal behavior through the run method's logging)
    }

    #[test_log::test(tokio::test)]
    async fn test_stats_collector_access() {
        let mut loader = MockLoader::new();
        let storage = MemoryStorage::default();

        loader
            .expect_into_stream()
            .returning(|| vec![Ok(TextNode::default()), Ok(TextNode::default())].into());

        let pipeline = Pipeline::from_loader(loader).then_store_with(storage.clone());

        // Access the stats collector
        let collector = pipeline.stats_collector();

        // Record some token usage manually (simulating what transformers would do)
        collector.record_token_usage("gpt-4", 100, 50);
        collector.record_token_usage("gpt-3.5", 50, 25);

        let stats = collector.get_stats();
        assert_eq!(stats.total_requests(), 2);
        assert_eq!(stats.total_tokens(), 225);

        // Run the pipeline
        pipeline.run().await.unwrap();

        // Verify storage has the nodes
        let nodes = storage.get_all().await;
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_nodes_only_cached_on_success() {
        let mut loader = MockLoader::new();
        let mut cache = MockNodeCache::new();
        let mut storage = MockPersist::new();
        let mut transformer = MockTransformer::new();

        loader
            .expect_into_stream()
            .returning(|| vec![Ok(Node::default())].into());

        cache.expect_get().times(1).returning(|_| false);
        cache.expect_name().returning(|| "test_cache");

        transformer
            .expect_transform_node()
            .returning(|_| Err(anyhow::anyhow!("Transformation failed")));
        transformer.expect_concurrency().returning(|| None);
        transformer
            .expect_name()
            .returning(|| "failing_transformer");

        storage.expect_setup().returning(|| Ok(()));
        storage.expect_batch_size().returning(|| None);

        // The node never makes it through the pipeline, so nothing may be cached
        cache.expect_set().times(0);
        cache.expect_set_by_id().times(0);

        let pipeline = Pipeline::from_loader(loader)
            .filter_cached(cache)
            .then(transformer)
            .then_store_with(storage)
            .filter_errors();

        pipeline.run().await.unwrap();
    }

    #[tokio::test]
    async fn test_nodes_cached_on_successful_run() {
        let mut loader = MockLoader::new();
        let mut cache = MockNodeCache::new();
        let storage = MemoryStorage::default();

        loader
            .expect_into_stream()
            .returning(|| vec![Ok(Node::default())].into());

        cache.expect_name().returning(|| "test_cache");
        cache.expect_get().times(1).returning(|_| false);

        cache.expect_set().times(0);
        cache.expect_set_by_id().times(1).returning(|_| ());

        let pipeline = Pipeline::from_loader(loader)
            .filter_cached(cache)
            .then_store_with(storage);

        pipeline.run().await.unwrap();
    }

    #[tokio::test]
    async fn test_cached_nodes_are_skipped() {
        let mut loader = MockLoader::new();
        let mut cache = MockNodeCache::new();
        let storage = MemoryStorage::default();

        loader
            .expect_into_stream()
            .returning(|| vec![Ok(Node::default())].into());

        cache.expect_name().returning(|| "test_cache");
        cache.expect_get().times(1).returning(|_| true);

        // Already cached, so no marking either
        cache.expect_set_by_id().times(0);

        let pipeline = Pipeline::from_loader(loader)
            .filter_cached(cache)
            .then_store_with(storage.clone());

        pipeline.run().await.unwrap();
        assert!(storage.get_all().await.is_empty());
    }

    #[tokio::test]
    async fn test_chunked_nodes_cache_their_parent_once() {
        let mut loader = MockLoader::new();
        let mut cache = MockNodeCache::new();
        let mut chunker = MockChunkerTransformer::new();
        let storage = MemoryStorage::default();

        let parent = Node::from("parent document");
        let parent_id = parent.id();

        loader
            .expect_into_stream()
            .returning(move || vec![Ok(parent.clone())].into());

        cache.expect_name().returning(|| "test_cache");
        cache.expect_get().times(1).returning(|_| false);

        chunker.expect_transform_node().returning(|node| {
            vec![
                Ok(Node::build_from_other(&node)
                    .chunk("chunk 1".to_string())
                    .build()
                    .unwrap()),
                Ok(Node::build_from_other(&node)
                    .chunk("chunk 2".to_string())
                    .build()
                    .unwrap()),
                Ok(Node::build_from_other(&node)
                    .chunk("chunk 3".to_string())
                    .build()
                    .unwrap()),
            ]
            .into()
        });
        chunker.expect_concurrency().returning(|| None);
        chunker.expect_name().returning(|| "chunker");

        // Three chunks make it through, but only the shared parent id gets cached
        cache
            .expect_set_by_id()
            .times(1)
            .withf(move |id| *id == parent_id)
            .returning(|_| ());

        let pipeline = Pipeline::from_loader(loader)
            .filter_cached(cache)
            .then_chunk(chunker)
            .then_store_with(storage.clone());

        pipeline.run().await.unwrap();
        assert_eq!(storage.get_all().await.len(), 3);
    }

    #[tokio::test]
    async fn test_multiple_caches_all_marked_on_success() {
        let mut loader = MockLoader::new();
        let mut first_cache = MockNodeCache::new();
        let mut second_cache = MockNodeCache::new();
        let storage = MemoryStorage::default();

        loader
            .expect_into_stream()
            .returning(|| vec![Ok(Node::default())].into());

        first_cache.expect_name().returning(|| "first_cache");
        first_cache.expect_get().times(1).returning(|_| false);
        first_cache.expect_set_by_id().times(1).returning(|_| ());

        second_cache.expect_name().returning(|| "second_cache");
        second_cache.expect_get().times(1).returning(|_| false);
        second_cache.expect_set_by_id().times(1).returning(|_| ());

        let pipeline = Pipeline::from_loader(loader)
            .filter_cached(first_cache)
            .filter_cached(second_cache)
            .then_store_with(storage);

        pipeline.run().await.unwrap();
    }

    #[tokio::test]
    async fn test_parent_not_cached_when_chunker_fails_after_success() {
        let mut loader = MockLoader::new();
        let mut cache = MockNodeCache::new();
        let mut chunker = MockChunkerTransformer::new();
        let storage = MemoryStorage::default();

        let parent = Node::from("parent document");

        loader
            .expect_into_stream()
            .returning(move || vec![Ok(parent.clone())].into());

        cache.expect_name().returning(|| "test_cache");
        cache.expect_get().times(1).returning(|_| false);
        // The chunker yields one child and then errors: the source must not
        // be cached, or its failed chunks would be skipped on the next run.
        cache.expect_set_by_id().times(0);

        chunker.expect_transform_node().returning(|node| {
            vec![
                Ok(Node::build_from_other(&node)
                    .chunk("chunk 1".to_string())
                    .build()
                    .unwrap()),
                Err(anyhow::anyhow!("chunking failed halfway")),
            ]
            .into()
        });
        chunker.expect_concurrency().returning(|| None);
        chunker.expect_name().returning(|| "chunker");

        let pipeline = Pipeline::from_loader(loader)
            .filter_cached(cache)
            .then_chunk(chunker)
            .then_store_with(storage.clone())
            .filter_errors();

        pipeline.run().await.unwrap();

        // The successful child was still stored
        assert_eq!(storage.get_all().await.len(), 1);
    }

    #[tokio::test]
    async fn test_merge_only_marks_caches_the_branch_processed() {
        let mut loader = MockLoader::new();
        let mut shared_cache = MockNodeCache::new();
        let mut left_cache = MockNodeCache::new();
        let mut right_cache = MockNodeCache::new();
        let storage = MemoryStorage::default();

        loader
            .expect_into_stream()
            .returning(|| vec![Ok(Node::default())].into());

        shared_cache.expect_name().returning(|| "shared_cache");
        shared_cache.expect_get().times(1).returning(|_| false);
        // Registered before split_by, so both halves inherit the same
        // registration; merge must not mark it twice.
        shared_cache.expect_set_by_id().times(1).returning(|_| ());

        left_cache.expect_name().returning(|| "left_cache");
        left_cache.expect_get().times(0);
        // The node was routed to the right branch, so the left cache must
        // not be marked; otherwise a later run with a changed predicate
        // would skip the node on the left branch.
        left_cache.expect_set_by_id().times(0);

        right_cache.expect_name().returning(|| "right_cache");
        right_cache.expect_get().times(1).returning(|_| false);
        right_cache.expect_set_by_id().times(1).returning(|_| ());

        let pipeline = Pipeline::from_loader(loader).filter_cached(shared_cache);
        let (left, right) = pipeline.split_by(|_| false);
        let left = left.filter_cached(left_cache);
        let right = right.filter_cached(right_cache);

        left.merge(right)
            .then_store_with(storage)
            .run()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_merge_tracks_failures_from_independent_pipelines() {
        let mut loader_left = MockLoader::new();
        let mut loader_right = MockLoader::new();
        let mut left_cache = MockNodeCache::new();
        let mut right_cache = MockNodeCache::new();
        let mut chunker = MockChunkerTransformer::new();
        let storage = MemoryStorage::default();

        let left_node = TextNode::new("left document");
        let right_node = TextNode::new("right document");
        let left_id = left_node.id();

        loader_left
            .expect_into_stream()
            .returning(move || vec![Ok(left_node.clone())].into());
        loader_right
            .expect_into_stream()
            .returning(move || vec![Ok(right_node.clone())].into());

        left_cache.expect_name().returning(|| "left_cache");
        left_cache.expect_get().times(1).returning(|_| false);
        left_cache
            .expect_set_by_id()
            .times(1)
            .withf(move |id| *id == left_id)
            .returning(|_| ());

        right_cache.expect_name().returning(|| "right_cache");
        right_cache.expect_get().times(1).returning(|_| false);
        // The right source fails midway, so it must never be marked, and the
        // left node never passed the right cache's filter, so the right cache
        // gets no marks at all.
        right_cache.expect_set_by_id().times(0);

        chunker.expect_transform_node().returning(|node| {
            vec![
                Ok(Node::build_from_other(&node)
                    .chunk("chunk 1".to_string())
                    .build()
                    .unwrap()),
                Err(anyhow::anyhow!("chunking failed halfway")),
            ]
            .into()
        });
        chunker.expect_concurrency().returning(|| None);
        chunker.expect_name().returning(|| "chunker");

        let left = Pipeline::from_loader(loader_left).filter_cached(left_cache);
        let right = Pipeline::from_loader(loader_right)
            .filter_cached(right_cache)
            .then_chunk(chunker)
            .filter_errors();

        left.merge(right)
            .then_store_with(storage.clone())
            .run()
            .await
            .unwrap();

        // The left node and the one successful right child were stored
        assert_eq!(storage.get_all().await.len(), 2);
    }
}
