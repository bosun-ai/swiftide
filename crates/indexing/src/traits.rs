use std::fmt::Debug;

use crate::{ingestion_node::IngestionNode, ingestion_pipeline::IngestionStream};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
/// Transforms single nodes into single nodes
pub trait Transformer: Send + Sync + Debug {
    async fn transform_node(&self, node: IngestionNode) -> Result<IngestionNode>;
}

#[async_trait]
/// Transforms batched single nodes into streams of nodes
pub trait BatchableTransformer: Send + Sync + Debug {
    fn batch_size(&self) -> Option<usize> {
        None
    }
    async fn batch_transform(&self, nodes: Vec<IngestionNode>) -> IngestionStream;
}

/// Starting point of a stream
pub trait Loader {
    fn into_stream(self) -> IngestionStream;
}

#[async_trait]
/// Turns one node into many nodes
pub trait ChunkerTransformer: Send + Sync + Debug {
    async fn transform_node(&self, node: IngestionNode) -> IngestionStream;
}

#[async_trait]
/// Persists nodes
pub trait Storage: Send + Sync {
    async fn setup(&self) -> Result<()>;
    async fn store(&self, node: IngestionNode) -> Result<()>;
    async fn batch_store(&self, nodes: Vec<IngestionNode>) -> Result<()>;
    fn batch_size(&self) -> Option<usize> {
        None
    }
}

#[async_trait]
/// Caches nodes, typically by their path and hash
/// Recommended to namespace on the storage
///
/// For now just bool return value for easy filter
pub trait NodeCache: Send + Sync + Debug {
    async fn get(&self, node: &IngestionNode) -> bool;
    async fn set(&self, node: &IngestionNode);
}
