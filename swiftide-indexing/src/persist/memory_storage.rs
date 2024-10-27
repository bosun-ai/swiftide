use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use tokio::sync::RwLock;

use swiftide_core::{
    indexing::{IndexingStream, Node},
    Persist,
};

#[derive(Debug, Default, Builder, Clone)]
#[builder(pattern = "owned")]
/// A simple in-memory storage implementation.
///
/// Great for experimentation and testing.
///
/// The storage will use a zero indexed, incremental counter as the key for each node if the node id
/// is not set.
pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<String, Node>>>,
    #[builder(default)]
    batch_size: Option<usize>,
    #[builder(default = Arc::new(AtomicUsize::new(0)))]
    node_count: Arc<AtomicUsize>,
}

impl MemoryStorage {
    fn key(&self) -> String {
        self.node_count.fetch_add(1, Ordering::Relaxed).to_string()
    }

    /// Retrieve a node by its key
    pub async fn get(&self, key: impl AsRef<str>) -> Option<Node> {
        self.data.read().await.get(key.as_ref()).cloned()
    }

    /// Retrieve all nodes in the storage
    pub async fn get_all_values(&self) -> Vec<Node> {
        self.data.read().await.values().cloned().collect()
    }

    /// Retrieve all nodes in the storage with their keys
    pub async fn get_all(&self) -> Vec<(String, Node)> {
        self.data
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

#[async_trait]
impl Persist for MemoryStorage {
    async fn setup(&self) -> Result<()> {
        Ok(())
    }

    /// Store a node by its id
    ///
    /// If the node does not have an id, a simple counter is used as the key.
    async fn store(&self, node: Node) -> Result<Node> {
        self.data.write().await.insert(self.key(), node.clone());

        Ok(node)
    }

    /// Store multiple nodes at once
    ///
    /// If a node does not have an id, a simple counter is used as the key.
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        let mut lock = self.data.write().await;

        for node in &nodes {
            lock.insert(self.key(), node.clone());
        }

        IndexingStream::iter(nodes.into_iter().map(Ok))
    }

    fn batch_size(&self) -> Option<usize> {
        self.batch_size
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures_util::TryStreamExt;
    use swiftide_core::indexing::Node;

    #[tokio::test]
    async fn test_memory_storage() {
        let storage = MemoryStorage::default();
        let node = Node::default();
        let node = storage.store(node.clone()).await.unwrap();
        assert_eq!(storage.get("0").await, Some(node));
    }

    #[tokio::test]
    async fn test_inserting_multiple_nodes() {
        let storage = MemoryStorage::default();
        let node1 = Node::default();
        let node2 = Node::default();

        storage.store(node1.clone()).await.unwrap();
        storage.store(node2.clone()).await.unwrap();

        dbg!(storage.get_all().await);
        assert_eq!(storage.get("0").await, Some(node1));
        assert_eq!(storage.get("1").await, Some(node2));
    }

    #[tokio::test]
    async fn test_batch_store() {
        let storage = MemoryStorage::default();
        let node1 = Node::default();
        let node2 = Node::default();

        let stream = storage
            .batch_store(vec![node1.clone(), node2.clone()])
            .await;

        let result: Vec<Node> = stream.try_collect().await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], node1);
        assert_eq!(result[1], node2);
    }
}
