//! This module provides integration with the Qdrant vector database.
//! It includes functionalities to interact with Qdrant, such as creating and managing vector collections,
//! storing data, and ensuring proper indexing for efficient searches.

mod ingestion_node;
mod persist;
use std::collections::HashMap;
use std::rc::Rc;

use std::sync::Arc;

use anyhow::{bail, Context as _, Result};
use derive_builder::Builder;
use qdrant_client::qdrant::vectors_config::Config;
use qdrant_client::qdrant::{CreateCollectionBuilder, Distance};
use qdrant_client::qdrant::{
    SparseIndexConfig, SparseVectorConfig, SparseVectorParams, VectorParams, VectorParamsBuilder,
    VectorParamsMap, VectorsConfig as QVectorsConfig,
};
use qdrant_client::{qdrant, Qdrant as QClient};

use crate::ingestion::{EmbedMode, EmbeddableType};

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
    /// By default the client will be build from QDRANT_URL and option QDRANT_API_KEY.
    /// It will fall back to `http://localhost:6334` if QDRANT_URL is not set.
    #[builder(setter(into), default = "self.default_client()?")]
    client: Arc<qdrant_client::Qdrant>,
    /// The name of the collection to be used in Qdrant. Defaults to "swiftide".
    #[builder(default = "DEFAULT_COLLECTION_NAME.to_string()")]
    #[builder(setter(into))]
    collection_name: String,
    /// The default size of the vectors to be stored in the collection.
    vector_size: u64,
    /// The batch size for operations. Optional.
    #[builder(default)]
    batch_size: Option<usize>,
    #[builder(private, default = "Self::default_vectors()")]
    vectors: HashMap<EmbeddableType, VectorConfig>,
}

impl QdrantBuilder {
    pub fn with_vector(
        mut self,
        embeddable_type: EmbeddableType,
        vector: VectorConfig,
    ) -> QdrantBuilder {
        if self.vectors.is_none() {
            self = self.vectors(Default::default());
        }
        if let Some(vectors) = self.vectors.as_mut() {
            vectors.insert(embeddable_type, vector);
        }
        self
    }

    fn default_vectors() -> HashMap<EmbeddableType, VectorConfig> {
        HashMap::from([(Default::default(), Default::default())])
    }
}

impl Qdrant {
    /// Returns a new `QdrantBuilder` for constructing a `Qdrant` instance.
    pub fn builder() -> QdrantBuilder {
        QdrantBuilder::default()
    }

    /// Tries to create a `QdrantBuilder` from a given URL. Will use the api key in QDRANT_API_KEY if present.
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
    pub async fn create_index_if_not_exists(&self) -> Result<()> {
        tracing::info!("Checking if collection {} exists", self.collection_name);
        if self.client.collection_exists(&self.collection_name).await? {
            tracing::warn!("Collection {} exists", self.collection_name);
            return Ok(());
        }

        tracing::warn!("Creating collection {}", self.collection_name);
        let vectors_config = self.create_vectors_config()?;
        let create_collection_req = CreateCollectionBuilder::new(self.collection_name.clone())
            .vectors_config(vectors_config);
        self.client
            .create_collection(create_collection_req).await?;

        // self.client
        //     .create_collection(
        //         CreateCollectionBuilder::new(self.collection_name.clone())
        //             .vectors_config(VectorParamsBuilder::new(self.vector_size, Distance::Cosine)),
        //     )
            // .create_collection(&CreateCollection {
            //     collection_name: self.collection_name.to_string(),
            //     // vectors_config: Some(VectorsConfig {
            //     //     config: Some(Config::Params(VectorParams {
            //     //         size: self.vector_size,
            //     //         distance: Distance::Cosine.into(),
            //     //         ..Default::default()
            //     //     })),
            //     // }),
            //     vectors_config: Some(VectorsConfig {
            //         config: Some(Config::ParamsMap(VectorParamsMap {
            //             map: [
            //                 (
            //                     "image".to_string(),
            //                     VectorParams {
            //                         size: 4,
            //                         distance: Distance::Dot.into(),
            //                         ..Default::default()
            //                     },
            //                 ),
            //                 (
            //                     "text".to_string(),
            //                     VectorParams {
            //                         size: 8,
            //                         distance: Distance::Cosine.into(),
            //                         ..Default::default()
            //                     },
            //                 ),
            //             ]
            //             .into(),
            //         })),
            //     }),
            //     sparse_vectors_config: Some(SparseVectorConfig {
            //         map: HashMap::from([(
            //             "sparsed".into(),
            //             SparseVectorParams {
            //                 index: Some(SparseIndexConfig {
            //                     ..Default::default()
            //                 }),
            //             },
            //         )]),
            //     }),
            //     ..Default::default()
            // })
            // .await?;
        Ok(())
    }

    fn create_vectors_config(&self) -> Result<impl Into<qdrant_client::qdrant::VectorsConfig>>
    {
        if self.vectors.is_empty() {
            bail!("No configured vectors");
        } else if self.vectors.len() == 1 {
            let config = self.vectors.values().next().expect("Has one vector config");
            return Ok(VectorParamsBuilder::new(self.vector_size, Distance::Cosine).build());
        }

        todo!()
    }

    fn create_vector_config(
        &self,
        vector_config: &VectorConfig,
    ) -> qdrant_client::qdrant::VectorsConfig {
        let vector_size = vector_config.vector_size.unwrap_or(self.vector_size);
        let distance = vector_config.distance;
    }
}

impl QdrantBuilder {
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

impl std::fmt::Debug for Qdrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Qdrant")
            .field("collection_name", &self.collection_name)
            .field("vector_size", &self.vector_size)
            .field("batch_size", &self.batch_size)
            .finish()
    }
}

#[derive(Clone, Builder, Default)]
pub struct VectorConfig {
    #[builder(setter(into, strip_option), default)]
    vector_size: Option<u64>,
    // TODO: do not export qdrant type
    #[builder(default = "qdrant_client::qdrant::Distance::Cosine")]
    distance: qdrant::Distance,
}
