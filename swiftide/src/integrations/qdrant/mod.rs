mod ingestion_node;
mod persist;

use anyhow::Result;
use derive_builder::Builder;
use qdrant_client::client::QdrantClient;
use qdrant_client::prelude::*;
use qdrant_client::qdrant::vectors_config::Config;
use qdrant_client::qdrant::{VectorParams, VectorsConfig};

const DEFAULT_COLLECTION_NAME: &str = "swiftide";

#[derive(Builder)]
#[builder(pattern = "owned")]
pub struct Qdrant {
    client: QdrantClient,
    #[builder(default = "DEFAULT_COLLECTION_NAME.to_string()")]
    collection_name: String,
    vector_size: usize,
    #[builder(default, setter(strip_option))]
    batch_size: Option<usize>,
}

impl Qdrant {
    pub fn builder() -> QdrantBuilder {
        QdrantBuilder::default()
    }

    pub fn try_from_url(url: impl AsRef<str>) -> Result<QdrantBuilder> {
        Ok(QdrantBuilder::default().client(QdrantClient::from_url(url.as_ref()).build()?))
    }

    pub async fn create_index_if_not_exists(&self) -> Result<()> {
        tracing::info!("Checking if collection {} exists", self.collection_name);
        if self.client.collection_exists(&self.collection_name).await? {
            tracing::warn!("Collection {} exists", self.collection_name);
            return Ok(());
        }

        tracing::warn!("Creating collection {}", self.collection_name);
        self.client
            .create_collection(&CreateCollection {
                collection_name: self.collection_name.to_string(),
                vectors_config: Some(VectorsConfig {
                    config: Some(Config::Params(VectorParams {
                        size: self.vector_size as u64,
                        distance: Distance::Cosine.into(),
                        ..Default::default()
                    })),
                }),
                ..Default::default()
            })
            .await?;
        Ok(())
    }
}
