//! This module provides integration with the Qdrant vector database.
//! It includes functionalities to interact with Qdrant, such as creating and managing vector collections,
//! storing data, and ensuring proper indexing for efficient searches.

mod indexing_node;
mod persist;

use std::sync::Arc;

use anyhow::{Context as _, Result};
use derive_builder::Builder;
use qdrant_client::qdrant::{CreateCollectionBuilder, Distance, VectorParamsBuilder};

const DEFAULT_COLLECTION_NAME: &str = "swiftide";
const DEFAULT_QDRANT_URL: &str = "http://localhost:6334";

/// A struct representing a Qdrant client with configuration options.
///
/// This struct is used to interact with the Qdrant vector database, providing methods to create and manage
/// vector collections, store data, and ensure proper indexing for efficient searches.
///
/// Can be cloned with relative low cost as the client is shared.
#[derive(Builder, Clone)]
#[builder(
    pattern = "owned",
    setter(strip_option),
    build_fn(error = "anyhow::Error")
)]
pub struct Qdrant {
    /// The Qdrant client used to interact with the Qdrant vector database.
    ///
    /// By default the client will be build from `QDRANT_URL` and option `QDRANT_API_KEY`.
    /// It will fall back to `http://localhost:6334` if `QDRANT_URL` is not set.
    #[builder(setter(into), default = "self.default_client()?")]
    #[allow(clippy::missing_fields_in_debug)]
    client: Arc<qdrant_client::Qdrant>,
    /// The name of the collection to be used in Qdrant. Defaults to "swiftide".
    #[builder(default = "DEFAULT_COLLECTION_NAME.to_string()")]
    #[builder(setter(into))]
    collection_name: String,
    /// The size of the vectors to be stored in the collection.
    vector_size: u64,
    /// The batch size for operations. Optional.
    #[builder(default)]
    batch_size: Option<usize>,
}

impl Qdrant {
    /// Returns a new `QdrantBuilder` for constructing a `Qdrant` instance.
    pub fn builder() -> QdrantBuilder {
        QdrantBuilder::default()
    }

    /// Tries to create a `QdrantBuilder` from a given URL. Will use the api key in `QDRANT_API_KEY` if present.
    ///
    /// Returns
    ///
    /// # Arguments
    ///
    /// * `url` - A string slice that holds the URL for the Qdrant client.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `QdrantBuilder` if successful, or an error otherwise.
    ///
    /// # Errors
    ///
    /// Errors if client fails build
    pub fn try_from_url(url: impl AsRef<str>) -> Result<QdrantBuilder> {
        Ok(QdrantBuilder::default().client(
            qdrant_client::Qdrant::from_url(url.as_ref())
                .api_key(std::env::var("QDRANT_API_KEY"))
                .build()?,
        ))
    }

    /// Creates an index in the Qdrant collection if it does not already exist.
    ///
    /// This method checks if the specified collection exists in Qdrant. If it does not exist, it creates a new collection
    /// with the specified vector size and cosine distance metric.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Errors if client fails build
    pub async fn create_index_if_not_exists(&self) -> Result<()> {
        tracing::info!("Checking if collection {} exists", self.collection_name);
        if self.client.collection_exists(&self.collection_name).await? {
            tracing::warn!("Collection {} exists", self.collection_name);
            return Ok(());
        }

        tracing::warn!("Creating collection {}", self.collection_name);
        self.client
            .create_collection(
                CreateCollectionBuilder::new(self.collection_name.clone())
                    .vectors_config(VectorParamsBuilder::new(self.vector_size, Distance::Cosine)),
            )
            .await?;
        Ok(())
    }
}

impl QdrantBuilder {
    #[allow(clippy::unused_self)]
    fn default_client(&self) -> Result<Arc<qdrant_client::Qdrant>> {
        let client = qdrant_client::Qdrant::from_url(
            &std::env::var("QDRANT_URL").unwrap_or(DEFAULT_QDRANT_URL.to_string()),
        )
        .api_key(std::env::var("QDRANT_API_KEY"))
        .build()
        .context("Could not build default qdrant client")?;

        Ok(Arc::new(client))
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for Qdrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Qdrant")
            .field("collection_name", &self.collection_name)
            .field("vector_size", &self.vector_size)
            .field("batch_size", &self.batch_size)
            .finish()
    }
}
