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
pub trait Transformer: Send + Sync + Debug {
    async fn transform_node(&self, node: IngestionNode) -> Result<IngestionNode>;
}

#[cfg_attr(test, automock)]
#[async_trait]
/// Transforms batched single nodes into streams of nodes
pub trait BatchableTransformer: Send + Sync + Debug {
    fn batch_size(&self) -> Option<usize> {
        None
    }
    async fn batch_transform(&self, nodes: Vec<IngestionNode>) -> IngestionStream;
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
pub trait Embed: Debug + Send + Sync {
    async fn embed(&self, input: Vec<String>) -> Result<Embeddings>;
}

#[async_trait]
pub trait SimplePrompt: Debug + Send + Sync {
    // Takes a simple prompt, prompts the llm and returns the response
    async fn prompt(&self, prompt: &str) -> Result<String>;
}

#[cfg_attr(test, automock)]
#[async_trait]
/// Persists nodes
pub trait Persist: Send + Sync {
    async fn setup(&self) -> Result<()>;
    async fn store(&self, node: IngestionNode) -> Result<IngestionNode>;
    async fn batch_store(&self, nodes: Vec<IngestionNode>) -> IngestionStream;
    fn batch_size(&self) -> Option<usize> {
        None
    }
}
