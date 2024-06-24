//! Traits in Swiftide allow for easy extendability
//!
//! All steps defined in the ingestion pipeline and the generic transformers can also take a
//! trait. To bring your own transformers, models and loaders, all you need to do is implement the
//! trait and it should work out of the box.
use std::fmt::Debug;

use crate::{ingestion::IngestionNode, ingestion::IngestionStream, Embeddings};
use anyhow::Result;
use async_trait::async_trait;

/// All traits are easilly mockable under tests
#[cfg(test)]
use mockall::{automock, predicate::*};

#[cfg_attr(test, automock)]
#[async_trait]
/// Transforms single nodes into single nodes
pub trait Transformer: Send + Sync {
    async fn transform_node(&self, node: IngestionNode) -> Result<IngestionNode>;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }
}

#[async_trait]
/// Use a closure as a transformer
impl<F> Transformer for F
where
    F: Fn(IngestionNode) -> Result<IngestionNode> + Send + Sync,
{
    async fn transform_node(&self, node: IngestionNode) -> Result<IngestionNode> {
        self(node)
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
/// Transforms batched single nodes into streams of nodes
pub trait BatchableTransformer: Send + Sync {
    /// Defines the batch size for the transformer
    fn batch_size(&self) -> Option<usize> {
        None
    }

    /// Transforms a batch of nodes into a stream of nodes
    async fn batch_transform(&self, nodes: Vec<IngestionNode>) -> IngestionStream;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }
}

#[async_trait]
/// Use a closure as a batchable transformer
impl<F> BatchableTransformer for F
where
    F: Fn(Vec<IngestionNode>) -> IngestionStream + Send + Sync,
{
    async fn batch_transform(&self, nodes: Vec<IngestionNode>) -> IngestionStream {
        self(nodes)
    }
}

/// Starting point of a stream
#[cfg_attr(test, automock)]
pub trait Loader {
    fn into_stream(self) -> IngestionStream;
}

#[cfg_attr(test, automock)]
#[async_trait]
/// Turns one node into many nodes
pub trait ChunkerTransformer: Send + Sync + Debug {
    async fn transform_node(&self, node: IngestionNode) -> IngestionStream;

    /// Overrides the default concurrency of the pipeline
    fn concurrency(&self) -> Option<usize> {
        None
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
/// Caches nodes, typically by their path and hash
/// Recommended to namespace on the storage
///
/// For now just bool return value for easy filter
pub trait NodeCache: Send + Sync + Debug {
    async fn get(&self, node: &IngestionNode) -> bool;
    async fn set(&self, node: &IngestionNode);
}

#[async_trait]
/// Embeds a list of strings and returns its embeddings.
/// Assumes the strings will be moved.
pub trait EmbeddingModel: Send + Sync {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings>;
}

#[cfg_attr(test, automock)]
#[async_trait]
/// Given a string prompt, queries an LLM
pub trait SimplePrompt: Debug + Send + Sync {
    // Takes a simple prompt, prompts the llm and returns the response
    async fn prompt(&self, prompt: &str) -> Result<String>;
}

#[cfg_attr(test, automock)]
#[async_trait]
/// Persists nodes
pub trait Persist: Debug + Send + Sync {
    async fn setup(&self) -> Result<()>;
    async fn store(&self, node: IngestionNode) -> Result<IngestionNode>;
    async fn batch_store(&self, nodes: Vec<IngestionNode>) -> IngestionStream;
    fn batch_size(&self) -> Option<usize> {
        None
    }
}
