use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use qdrant_client::{
    client::QdrantClient,
    qdrant::{
        vectors_config::Config, Condition, CreateCollection, Distance, Filter, PointsSelector,
        VectorParams, VectorsConfig,
    },
};
use tokio::sync::RwLock;

use crate::traits::Storage;

#[derive(Debug)]
pub struct CollectionName(String);

pub struct Qdrant {
    client: QdrantClient,
    collection_name: CollectionName,
    embedding_size: usize,
    batch_size: Option<usize>,
    seen_files: RwLock<HashSet<String>>,
}

impl Qdrant {
    pub fn try_from_url(url: &str, collection_name: impl Into<CollectionName>) -> Result<Self> {
        let client = QdrantClient::from_url(url).build()?;
        Ok(Qdrant {
            client,
            collection_name: collection_name.into(),
            embedding_size: 1536,
            batch_size: None,
            seen_files: RwLock::new(HashSet::new()),
        })
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    pub async fn create_index_if_not_exists(&self) -> Result<()> {
        let collection_name = &self.collection_name;

        if self.client.collection_exists(collection_name).await? {
            return Ok(());
        }

        self.client
            .create_collection(&CreateCollection {
                collection_name: self.collection_name.to_string(),
                vectors_config: Some(VectorsConfig {
                    config: Some(Config::Params(VectorParams {
                        size: self.embedding_size as u64,
                        distance: Distance::Cosine.into(),
                        ..Default::default()
                    })),
                }),
                ..Default::default()
            })
            .await?;
        Ok(())
    }

    /// Since files might be split over multiple nodes
    /// and node id does not represent the file
    /// When we see a file for the first time, we delete all points referencing that file
    async fn delete_file_if_exists(
        &self,
        node: &crate::ingestion_node::IngestionNode,
    ) -> Result<()> {
        if self.seen_files.read().await.contains(
            node.path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?,
        ) {
            return Ok(());
        }

        self.client
            .delete_points_blocking(
                self.collection_name.to_string(),
                None,
                &PointsSelector::from(Filter::all([Condition::matches(
                    "path".to_string(),
                    node.path.to_string_lossy().to_string(),
                )])),
                None,
            )
            .await?;

        self.seen_files
            .write()
            .await
            .insert(node.path.to_string_lossy().to_string());
        Ok(())
    }
}

#[async_trait]
impl Storage for Qdrant {
    fn batch_size(&self) -> Option<usize> {
        self.batch_size
    }
    async fn setup(&self) -> Result<()> {
        self.create_index_if_not_exists().await
    }

    async fn store(&self, node: crate::ingestion_node::IngestionNode) -> Result<()> {
        self.delete_file_if_exists(&node).await?;
        self.client
            .upsert_points_blocking(
                self.collection_name.to_string(),
                None,
                vec![node.try_into()?],
                None,
            )
            .await?;
        Ok(())
    }

    async fn batch_store(&self, nodes: Vec<crate::ingestion_node::IngestionNode>) -> Result<()> {
        for node in nodes.iter() {
            self.delete_file_if_exists(node).await?;
        }
        self.client
            .upsert_points_blocking(
                self.collection_name.to_string(),
                None,
                nodes
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>>>()?,
                None,
            )
            .await?;
        Ok(())
    }
}

#[allow(clippy::to_string_trait_impl)]
impl ToString for &CollectionName {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

#[allow(clippy::to_string_trait_impl)]
impl ToString for CollectionName {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl From<String> for CollectionName {
    fn from(val: String) -> Self {
        CollectionName(val)
    }
}

impl From<&str> for CollectionName {
    fn from(val: &str) -> Self {
        CollectionName(val.to_string())
    }
}
