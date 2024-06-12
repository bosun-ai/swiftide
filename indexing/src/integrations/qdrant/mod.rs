mod ingestion_node;
mod persist;
use anyhow::Result;
use qdrant_client::client::QdrantClient;
use qdrant_client::prelude::*;
use qdrant_client::qdrant::vectors_config::Config;
use qdrant_client::qdrant::{VectorParams, VectorsConfig};

pub struct Qdrant {
    client: QdrantClient,
    collection_name: String,
    vector_size: usize,
    batch_size: Option<usize>,
}

impl Qdrant {
    pub fn from_client(client: QdrantClient, collection_name: impl Into<String>) -> Self {
        Qdrant {
            client,
            collection_name: collection_name.into(),
            vector_size: 1536,
            batch_size: None,
        }
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    /// The size (dimensions) of the embedding vectors being stored
    ///
    /// I.e. for small openai embeddings this is 1536
    pub fn with_vector_size(mut self, vector_size: usize) -> Self {
        self.vector_size = vector_size;
        self
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
