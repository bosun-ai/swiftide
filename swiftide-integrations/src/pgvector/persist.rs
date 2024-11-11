//! This module implements the `Persist` trait for the `PgVector` struct.
//! It provides methods for setting up storage, saving individual nodes, and batch-storing multiple nodes.
//! This integration enables the Swiftide project to use `PgVector` as a storage backend.
use crate::pgvector::PgVector;
use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{
    indexing::{IndexingStream, Node},
    Persist,
};

#[async_trait]
impl Persist for PgVector {
    #[tracing::instrument(skip_all)]
    async fn setup(&self) -> Result<()> {
        // Get or initialize the connection pool
        let pool = self.pool_get_or_initialize().await?;

        let mut tx = pool.begin().await?;

        // Create extension
        let sql = "CREATE EXTENSION IF NOT EXISTS vector";
        sqlx::query(sql).execute(&mut *tx).await?;

        // Create table
        let create_table_sql = self.generate_create_table_sql()?;
        sqlx::query(&create_table_sql).execute(&mut *tx).await?;

        // Create HNSW index
        let index_sql = self.create_index_sql()?;
        sqlx::query(&index_sql).execute(&mut *tx).await?;

        tx.commit().await?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn store(&self, node: Node) -> Result<Node> {
        let mut nodes = vec![node; 1];
        self.store_nodes(&nodes).await?;

        let node = nodes.swap_remove(0);

        Ok(node)
    }

    #[tracing::instrument(skip_all)]
    async fn batch_store(&self, nodes: Vec<Node>) -> IndexingStream {
        self.store_nodes(&nodes).await.map(|()| nodes).into()
    }

    fn batch_size(&self) -> Option<usize> {
        Some(self.batch_size)
    }
}

#[cfg(test)]
mod tests {
    use crate::pgvector::fixtures::TestContext;
    use std::collections::HashSet;
    use swiftide_core::{indexing::EmbeddedField, Persist};

    #[test_log::test(tokio::test)]
    async fn test_persist_setup_no_error_when_table_exists() {
        let test_context = TestContext::setup_with_cfg(
            vec!["filter"].into(),
            HashSet::from([EmbeddedField::Combined]),
        )
        .await
        .expect("Test setup failed");

        test_context
            .pgv_storage
            .setup()
            .await
            .expect("PgVector setup should not fail when the table already exists");
    }
}
