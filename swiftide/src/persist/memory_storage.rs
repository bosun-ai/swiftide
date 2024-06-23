use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use tokio::sync::RwLock;

use crate::{
    ingestion::{IngestionNode, IngestionStream},
    Persist,
};

#[derive(Debug, Default, Builder)]
#[builder(pattern = "owned")]
/// A simple in-memory storage implementation.
///
/// Great for experimentation and testing.
pub struct MemoryStorage {
    data: RwLock<HashMap<String, IngestionNode>>,
    #[builder(default)]
    batch_size: Option<usize>,
}

impl MemoryStorage {
    fn key(&self, node: &IngestionNode) -> String {
        node.path.clone().to_string_lossy().to_string()
    }

    #[allow(dead_code)]
    async fn get(&self, key: &str) -> Option<IngestionNode> {
        self.data.read().await.get(key).cloned()
    }
}

#[async_trait]
impl Persist for MemoryStorage {
    async fn setup(&self) -> Result<()> {
        Ok(())
    }

    async fn store(&self, node: IngestionNode) -> Result<IngestionNode> {
        self.data
            .write()
            .await
            .insert(self.key(&node), node.clone());
        Ok(node)
    }

    async fn batch_store(&self, nodes: Vec<IngestionNode>) -> IngestionStream {
        let mut lock = self.data.write().await;
        for node in &nodes {
            lock.insert(self.key(node), node.clone());
        }
        IngestionStream::iter(nodes.into_iter().map(Ok))
    }

    fn batch_size(&self) -> Option<usize> {
        self.batch_size
    }
}
