//! Traits in Swiftide allow for easy extendability
//!
//! All steps defined in the indexing pipeline and the generic transformers can also take a
//! trait. To bring your own transformers, models and loaders, all you need to do is implement the
//! trait and it should work out of the box.
use crate::node::Node;
use crate::Embeddings;
use crate::{indexing_defaults::IndexingDefaults, indexing_stream::IndexingStream, SparseEmbeddings};
use std::fmt::Debug;

use crate::prompt::Prompt;
use anyhow::Result;
use async_trait::async_trait;

/// All traits are easily mockable under tests
#[cfg(feature = "test-utils")]
#[doc(hidden)]
use mockall::{automock, predicate::str};

#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Transforms single nodes into single nodes
pub trait Transformer: Send + Sync {
    async fn transform_node(&self, node: Node) -> Result<Node>;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }
}

#[async_trait]
/// Use a closure as a transformer
impl<F> Transformer for F
where
    F: Fn(Node) -> Result<Node> + Send + Sync,
{
    async fn transform_node(&self, node: Node) -> Result<Node> {
        self(node)
    }
}

#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Transforms batched single nodes into streams of nodes
pub trait BatchableTransformer: Send + Sync {
    /// Transforms a batch of nodes into a stream of nodes
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }
}

#[async_trait]
/// Use a closure as a batchable transformer
impl<F> BatchableTransformer for F
where
    F: Fn(Vec<Node>) -> IndexingStream + Send + Sync,
{
    async fn batch_transform(&self, nodes: Vec<Node>) -> IndexingStream {
        self(nodes)
    }
}

/// Starting point of a stream
#[cfg_attr(feature = "test-utils", automock, doc(hidden))]
pub trait Loader {
    fn into_stream(self) -> IndexingStream;
}

#[cfg_attr(feature = "test-utils", automock, doc(hidden))]
#[async_trait]
/// Turns one node into many nodes
pub trait ChunkerTransformer: Send + Sync + Debug {
    async fn transform_node(&self, node: Node) -> IndexingStream;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }
}

#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Caches nodes, typically by their path and hash
/// Recommended to namespace on the storage
///
/// For now just bool return value for easy filter
pub trait NodeCache: Send + Sync + Debug {
    async fn get(&self, node: &Node) -> bool;
    async fn set(&self, node: &Node);
}

#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Embeds a list of strings and returns its embeddings.
/// Assumes the strings will be moved.
pub trait EmbeddingModel: Send + Sync + Debug {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings>;
}

#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Embeds a list of strings and returns its embeddings.
/// Assumes the strings will be moved.
pub trait SparseEmbeddingModel: Send + Sync + Debug {
    async fn sparse_embed(&self, input: Vec<String>) -> Result<SparseEmbeddings>;
}

#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Given a string prompt, queries an LLM
pub trait SimplePrompt: Debug + Send + Sync {
    // Takes a simple prompt, prompts the llm and returns the response
    async fn prompt(&self, prompt: Prompt) -> Result<String>;
}

#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
/// Persists nodes
pub trait Persist: Debug + Send + Sync {
    async fn setup(&self) -> Result<()>;
    async fn store(&self, node: Node) -> Result<Node>;
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream;
    fn batch_size(&self) -> Option<usize> {
        None
    }
}

pub trait WithIndexingDefaults {
    fn with_indexing_defaults(&mut self, _indexing_defaults: IndexingDefaults) {}
}

pub trait WithBatchIndexingDefaults {
    fn with_indexing_defaults(&mut self, _indexing_defaults: IndexingDefaults) {}
}

impl WithIndexingDefaults for dyn Transformer {}
impl WithBatchIndexingDefaults for dyn BatchableTransformer {}

impl<F> WithIndexingDefaults for F where F: Fn(Node) -> Result<Node> {}
impl<F> WithBatchIndexingDefaults for F where F: Fn(Vec<Node>) -> IndexingStream {}

#[cfg(feature = "test-utils")]
impl WithIndexingDefaults for MockTransformer {}
//
#[cfg(feature = "test-utils")]
impl WithBatchIndexingDefaults for MockBatchableTransformer {}
