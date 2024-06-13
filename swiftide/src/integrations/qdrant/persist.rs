use anyhow::Result;
use async_trait::async_trait;

use crate::traits::Storage;

use super::Qdrant;

#[async_trait]
impl Storage for Qdrant {
    fn batch_size(&self) -> Option<usize> {
        self.batch_size
    }

    #[tracing::instrument(skip_all, err)]
    async fn setup(&self) -> Result<()> {
        tracing::debug!("Setting up Qdrant storage");
        self.create_index_if_not_exists().await
    }

    #[tracing::instrument(skip_all, err, name = "storage.qdrant.store")]
    async fn store(&self, node: crate::ingestion::IngestionNode) -> Result<()> {
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

    #[tracing::instrument(skip_all, err, name = "storage.qdrant.batch_store")]
    async fn batch_store(&self, nodes: Vec<crate::ingestion::IngestionNode>) -> Result<()> {
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
