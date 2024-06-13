//! This module provides functionality to convert an `IngestionNode` into a `qdrant::PointStruct`.
//! The conversion is essential for storing data in the Qdrant vector database, which is used
//! for efficient vector similarity search. The module handles metadata augmentation and ensures
//! data compatibility with Qdrant's required format.

use anyhow::{Context as _, Result};
use std::collections::HashMap;

use crate::ingestion::IngestionNode;
use qdrant_client::{
    client::Payload,
    qdrant::{self, Value},
};

/// Implements the `TryInto` trait to convert an `IngestionNode` into a `qdrant::PointStruct`.
/// This conversion is necessary for storing the node in the Qdrant vector database.
impl TryInto<qdrant::PointStruct> for IngestionNode {
    type Error = anyhow::Error;

    /// Converts the `IngestionNode` into a `qdrant::PointStruct`.
    ///
    /// # Errors
    ///
    /// Returns an error if the vector is not set in the `IngestionNode`.
    ///
    /// # Returns
    ///
    /// A `Result` which is `Ok` if the conversion is successful, containing the `qdrant::PointStruct`.
    /// If the conversion fails, it returns an `anyhow::Error`.
    fn try_into(mut self) -> Result<qdrant::PointStruct> {
        // Calculate a unique identifier for the node.
        let id = self.calculate_hash();

        // Extend the metadata with additional information.
        self.metadata.extend([
            ("path".to_string(), self.path.to_string_lossy().to_string()),
            ("content".to_string(), self.chunk),
            (
                "last_updated_at".to_string(),
                chrono::Utc::now().to_rfc3339(),
            ),
        ]);

        // Create a payload compatible with Qdrant's API.
        let payload: Payload = self
            .metadata
            .iter()
            .map(|(k, v)| (k.as_str(), Value::from(v.as_str())))
            .collect::<HashMap<&str, Value>>()
            .into();

        // Construct the `qdrant::PointStruct` and return it.
        Ok(qdrant::PointStruct::new(
            id,
            self.vector.context("Vector is not set")?,
            payload,
        ))
    }
}
