//! This module provides an implementation of the `Storage` trait for the `Qdrant` struct.
//! It includes methods for setting up the storage, storing a single node, and storing a batch of
//! nodes. This integration allows the Swiftide project to use Qdrant as a storage backend.

use std::collections::HashSet;
use swiftide_core::{
    indexing::{EmbeddedField, IndexingStream, Node, Persist},
    prelude::*,
};

use qdrant_client::qdrant::UpsertPointsBuilder;

use super::{NodeWithVectors, Qdrant};

#[async_trait]
impl Persist for Qdrant {
    /// Returns the batch size for the Qdrant storage.
    ///
    /// # Returns
    ///
    /// An `Option<usize>` representing the batch size if set, otherwise `None`.
    fn batch_size(&self) -> Option<usize> {
        self.batch_size
    }

    /// Sets up the Qdrant storage by creating the necessary index if it does not exist.
    ///
    /// # Returns
    ///
    /// A `Result<()>` which is `Ok` if the setup is successful, otherwise an error.
    ///
    /// # Errors
    ///
    /// This function will return an error if the index creation fails.
    #[tracing::instrument(skip_all, err)]
    async fn setup(&self) -> Result<()> {
        tracing::debug!("Setting up Qdrant storage");
        self.create_index_if_not_exists().await
    }

    /// Stores a single indexing node in the Qdrant storage.
    ///
    /// WARN: If running debug builds, the store is blocking and will impact performance
    ///
    /// # Parameters
    ///
    /// - `node`: The `Node` to be stored.
    ///
    /// # Returns
    ///
    /// A `Result<()>` which is `Ok` if the storage is successful, otherwise an error.
    ///
    /// # Errors
    ///
    /// This function will return an error if the node conversion or storage operation fails.
    #[tracing::instrument(skip_all, err, name = "storage.qdrant.store")]
    async fn store(&self, node: Node) -> Result<Node> {
        let node_with_vectors = NodeWithVectors::new(&node, self.vector_fields());
        let point = node_with_vectors.try_into()?;

        tracing::debug!("Storing node");

        self.client
            .upsert_points(
                UpsertPointsBuilder::new(self.collection_name.to_string(), vec![point])
                    .wait(cfg!(debug_assertions)),
            )
            .await?;
        Ok(node)
    }

    /// Stores a batch of indexing nodes in the Qdrant storage.
    ///
    /// # Parameters
    ///
    /// - `nodes`: A vector of `Node` to be stored.
    ///
    /// # Returns
    ///
    /// A `Result<()>` which is `Ok` if the storage is successful, otherwise an error.
    ///
    /// # Errors
    ///
    /// This function will return an error if any node conversion or storage operation fails.
    #[tracing::instrument(skip_all, name = "storage.qdrant.batch_store")]
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        let points = nodes
            .iter()
            .map(|node| NodeWithVectors::new(node, self.vector_fields()))
            .map(NodeWithVectors::try_into)
            .collect::<Result<Vec<_>>>();

        let Ok(points) = points else {
            return vec![Err(points.unwrap_err())].into();
        };

        tracing::debug!("Storing batch of {} nodes", points.len());

        let result = self
            .client
            .upsert_points(
                UpsertPointsBuilder::new(self.collection_name.to_string(), points)
                    .wait(cfg!(debug_assertions)),
            )
            .await;

        if result.is_ok() {
            IndexingStream::iter(nodes.into_iter().map(Ok))
        } else {
            vec![Err(result.unwrap_err().into())].into()
        }
    }
}

impl Qdrant {
    fn vector_fields(&self) -> HashSet<&EmbeddedField> {
        self.vectors.keys().collect::<HashSet<_>>()
    }
}
