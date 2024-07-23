//! This module provides integration with the Qdrant vector database.
//! It includes functionalities to interact with Qdrant, such as creating and managing vector collections,
//! storing data, and ensuring proper indexing for efficient searches.

mod indexing_node;
mod persist;
use std::collections::{HashMap, HashSet};

use std::sync::Arc;

use anyhow::{bail, Context as _, Result};
use derive_builder::Builder;
use qdrant_client::qdrant;

use swiftide_core::node::{EmbeddedField, Node};

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
    /// The default size of the vectors to be stored in the collection.
    vector_size: u64,
    #[builder(default = "Distance::Cosine")]
    /// The default distance of the vectors to be stored in the collection
    vector_distance: Distance,
    /// The batch size for operations. Optional.
    #[builder(default)]
    batch_size: Option<usize>,
    #[builder(private, default = "Self::default_vectors()")]
    vectors: HashMap<EmbeddedField, VectorConfig>,
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
        let vectors_config = self.create_vectors_config()?;
        let request = qdrant::CreateCollectionBuilder::new(self.collection_name.clone())
            .vectors_config(vectors_config);

        self.client.create_collection(request).await?;
        Ok(())
    }

    fn create_vectors_config(&self) -> Result<qdrant_client::qdrant::vectors_config::Config> {
        if self.vectors.is_empty() {
            bail!("No configured vectors");
        } else if self.vectors.len() == 1 {
            let config = self
                .vectors
                .values()
                .next()
                .context("Has one vector config")?;
            let vector_params = self.create_vector_params(config);
            return Ok(qdrant::vectors_config::Config::Params(vector_params));
        }
        let mut map = HashMap::<String, qdrant::VectorParams>::default();
        for (emebddable_type, config) in &self.vectors {
            let vector_name = emebddable_type.to_string();
            let vector_params = self.create_vector_params(config);
            map.insert(vector_name, vector_params.clone());
        }

        Ok(qdrant::vectors_config::Config::ParamsMap(
            qdrant::VectorParamsMap { map },
        ))
    }

    fn create_vector_params(&self, config: &VectorConfig) -> qdrant::VectorParams {
        let size = config.vector_size.unwrap_or(self.vector_size);
        let distance = config.distance.unwrap_or(self.vector_distance);
        qdrant::VectorParamsBuilder::new(size, distance).build()
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

    /// Adds new [`VectorConfig`]
    ///
    /// When not configured Pipeline by default configures vector only for [`EmbeddedField::Combined`]
    /// Default config is enough when [`crate::indexing::Pipeline::with_embed_mode`] is not set
    /// or when the value is set to [`crate::indexing::EmbedMode::SingleWithMetadata`].
    #[must_use]
    pub fn with_vector(mut self, vector: impl Into<VectorConfig>) -> QdrantBuilder {
        if self.vectors.is_none() {
            self = self.vectors(HashMap::default());
        }
        let vector = vector.into();
        if let Some(vectors) = self.vectors.as_mut() {
            if let Some(overridden_vector) = vectors.insert(vector.embedded_field.clone(), vector) {
                tracing::warn!(
                    "Overriding named vector config: {}",
                    overridden_vector.embedded_field
                );
            }
        }
        self
    }

    fn default_vectors() -> HashMap<EmbeddedField, VectorConfig> {
        HashMap::from([(EmbeddedField::default(), VectorConfig::default())])
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

/// Vector config
///
/// See also [`QdrantBuilder::with_vector`]
#[derive(Clone, Builder, Default)]
pub struct VectorConfig {
    /// A type of the embeddable of the stored vector.
    #[builder(default)]
    pub(super) embedded_field: EmbeddedField,
    /// A size of the vector to be stored in the collection.
    ///
    /// Overrides default set in [`QdrantBuilder::vector_size`]
    #[builder(setter(into, strip_option), default)]
    vector_size: Option<u64>,
    /// A distance of the vector to be stored in the collection.
    ///
    /// Overrides default set in [`QdrantBuilder::vector_distance`]
    #[builder(setter(into, strip_option), default)]
    distance: Option<qdrant::Distance>,
}

impl VectorConfig {
    pub fn builder() -> VectorConfigBuilder {
        VectorConfigBuilder::default()
    }
}

impl From<EmbeddedField> for VectorConfig {
    fn from(value: EmbeddedField) -> Self {
        Self {
            embedded_field: value,
            ..Default::default()
        }
    }
}

pub type Distance = qdrant::Distance;

/// Utility struct combining `Node` with `EmbeddedField`s of configured _Qdrant_ vectors.
struct NodeWithVectors {
    vector_fields: HashSet<EmbeddedField>,
    node: Node,
}

impl NodeWithVectors {
    pub fn new(node: Node, vector_fields: HashSet<EmbeddedField>) -> Self {
        Self {
            vector_fields,
            node,
        }
    }
}
