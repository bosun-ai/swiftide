use crate::ingestion_node::IngestionNode;
use crate::traits::{
    BatchableTransformer, ChunkerTransformer, Loader, NodeCache, Storage, Transformer,
};
use anyhow::Result;
use futures_util::{Stream, StreamExt, TryFutureExt, TryStreamExt};

use std::pin::Pin;
use std::sync::Arc;

pub type IngestionStream = Pin<Box<dyn Stream<Item = Result<IngestionNode>> + Send>>;

pub struct IngestionPipeline {
    stream: IngestionStream,
    storage: Option<Box<dyn Storage>>,
    concurrency: usize,
}

// A lazy pipeline for ingesting files, adding metadata, chunking, transforming, embedding and then storing them.
impl IngestionPipeline {
    // TODO: fix lifetime
    pub fn from_loader(loader: impl Loader + 'static) -> Self {
        let stream = loader.into_stream();
        Self {
            stream: stream.boxed(),
            storage: None,
            concurrency: 10,
        }
    }

    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    pub fn filter_cached(mut self, cache: impl NodeCache + 'static) -> Self {
        let cache = Arc::new(cache);
        self.stream = self
            .stream
            .try_filter(move |node| {
                let cache = Arc::clone(&cache);
                // TODO: Maybe Cow or arc instead? Lots of nodes
                let node = node.clone();
                tokio::spawn(async move {
                    if !cache.get(&node).await {
                        cache.set(&node).await;

                        true
                    } else {
                        false
                    }
                })
                .unwrap_or_else(|_e| true)
            })
            .boxed();
        self
    }

    #[tracing::instrument(skip_all)]
    pub fn then(mut self, transformer: impl Transformer + 'static) -> Self {
        let transformer = Arc::new(transformer);
        self.stream = self
            .stream
            .map_ok(move |node| {
                let transformer = Arc::clone(&transformer);
                tokio::spawn(async move { transformer.transform_node(node).await })
                    .map_err(anyhow::Error::from)
            })
            .try_buffer_unordered(self.concurrency)
            // Flatten the double result
            .map(|x| x.and_then(|x| x))
            .boxed();

        self
    }

    #[tracing::instrument(skip_all)]
    pub fn then_in_batch(
        mut self,
        batch_size: usize,
        transformer: impl BatchableTransformer + 'static,
    ) -> Self {
        let transformer = Arc::new(transformer);
        self.stream = self
            .stream
            .try_chunks(batch_size)
            .map_ok(move |chunks| {
                let transformer = Arc::clone(&transformer);
                tokio::spawn(async move { transformer.batch_transform(chunks).await })
                    .map_err(anyhow::Error::from)
            })
            // We need to coerce both the stream error and tokio error to anyhow manually
            .err_into::<anyhow::Error>()
            .try_buffer_unordered(self.concurrency)
            .try_flatten()
            .boxed();
        self
    }

    // Takes a single node, splits it into multiple, then flattens the stream
    #[tracing::instrument(skip_all)]
    pub fn then_chunk(mut self, chunker: impl ChunkerTransformer + 'static) -> Self {
        let chunker = Arc::new(chunker);
        self.stream = self
            .stream
            .map_ok(move |node| {
                let chunker = Arc::clone(&chunker);
                tokio::spawn(async move { chunker.transform_node(node).await })
                    .map_err(anyhow::Error::from)
            })
            // We need to coerce both the stream error and tokio error to anyhow manually
            .err_into::<anyhow::Error>()
            .try_buffer_unordered(self.concurrency)
            .try_flatten()
            .boxed();

        self
    }

    pub fn store_with(mut self, storage: impl Storage + 'static) -> Self {
        self.storage = Some(Box::new(storage));
        self
    }

    #[tracing::instrument(skip_all)]
    pub async fn run(mut self) -> Result<()> {
        let Some(ref storage) = self.storage else {
            return Ok(());
        };

        storage.setup().await?;

        if let Some(batch_size) = storage.batch_size() {
            // Chunk both Ok and Err results, early return on any error
            let mut stream = self.stream.chunks(batch_size).boxed();
            while let Some(nodes) = stream.next().await {
                let nodes = nodes.into_iter().collect::<Result<Vec<IngestionNode>>>()?;
                storage.batch_store(nodes).await?;
            }
        } else {
            while let Some(node) = self.stream.next().await {
                storage.store(node?).await?;
            }
        }

        Ok(())
    }
}
