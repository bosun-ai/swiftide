//! This module provides an implementation of the `Storage` trait for the `Qdrant` struct.
//! It includes methods for setting up the storage, storing a single node, and storing a batch of nodes.
//! This integration allows the Swiftide project to use Qdrant as a storage backend.

use anyhow::Result;
use async_trait::async_trait;
use qdrant_client::qdrant::UpsertPointsBuilder;

use crate::{
    indexing::{IndexingStream, Node},
    traits::Persist,
};

use super::Qdrant;

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
    async fn store(&self, node: crate::indexing::Node) -> Result<Node> {
        let point = node.clone().try_into()?;

        tracing::debug!(?node, ?point, "Storing node");

        self.client
            .upsert_points(UpsertPointsBuilder::new(
                self.collection_name.to_string(),
                vec![point],
            ))
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
    async fn batch_store(&self, nodes: Vec<crate::indexing::Node>) -> IndexingStream {
        let points = nodes
            .iter()
            .map(|node| node.clone().try_into())
            .collect::<Result<Vec<_>>>();

        if points.is_err() {
            return vec![Err(points.unwrap_err())].into();
        }

        let points = points.unwrap();

        tracing::debug!("Storing batch of {} nodes", points.len());

        let result = self
            .client
            .upsert_points(UpsertPointsBuilder::new(
                self.collection_name.to_string(),
                points,
            ))
            .await;

        if result.is_ok() {
            IndexingStream::iter(nodes.into_iter().map(Ok))
        } else {
            vec![Err(result.unwrap_err().into())].into()
        }
    }
}
