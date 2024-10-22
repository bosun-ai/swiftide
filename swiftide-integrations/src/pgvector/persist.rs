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
        tracing::info!("Setting up table {} for PgVector", &self.table_name);

        let mut tx = self.connection_pool.get_pool()?.begin().await?;

        // Create extension
        let sql = "CREATE EXTENSION IF NOT EXISTS vector";
        sqlx::query(sql).execute(&mut *tx).await?;

        // Create table
        let create_table_sql = self.generate_create_table_sql()?;
        tracing::debug!("Executing CREATE TABLE SQL: {}", create_table_sql);
        sqlx::query(&create_table_sql).execute(&mut *tx).await?;

        // Create HNSW index
        let index_sql = self.create_index_sql()?;
        tracing::debug!("Executing CREATE INDEX SQL: {}", index_sql);
        sqlx::query(&index_sql).execute(&mut *tx).await?;

        tx.commit().await?;

        tracing::info!("Table {} setup completed", &self.table_name);
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
        self.batch_size
    }
}

#[cfg(test)]
mod tests {
    use crate::pgvector::PgVector;
    use swiftide_core::{indexing::EmbeddedField, Persist};
    use temp_dir::TempDir;
    use testcontainers::{ContainerAsync, GenericImage};

    struct TestContext {
        pgv_storage: PgVector,
        _temp_dir: TempDir,
        _pgv_db_container: ContainerAsync<GenericImage>,
    }

    impl TestContext {
        /// Set up the test context, initializing `PostgreSQL` and `PgVector` storage
        async fn setup() -> Result<Self, Box<dyn std::error::Error>> {
            // Start PostgreSQL container and obtain the connection URL
            let (pgv_db_container, pgv_db_url, temp_dir) =
                swiftide_test_utils::start_postgres().await;

            tracing::info!("Postgres database URL: {:#?}", pgv_db_url);

            // Configure and build PgVector storage
            let pgv_storage = PgVector::builder()
                .try_connect_to_pool(pgv_db_url, Some(10))
                .await
                .map_err(|err| {
                    tracing::error!("Failed to connect to Postgres server: {}", err);
                    err
                })?
                .vector_size(384)
                .with_vector(EmbeddedField::Combined)
                .with_metadata("filter")
                .table_name("swiftide_pgvector_test".to_string())
                .build()
                .map_err(|err| {
                    tracing::error!("Failed to build PgVector: {}", err);
                    err
                })?;

            // Set up PgVector storage (create the table if not exists)
            pgv_storage.setup().await.map_err(|err| {
                tracing::error!("PgVector setup failed: {}", err);
                err
            })?;

            Ok(Self {
                pgv_storage,
                _temp_dir: temp_dir,
                _pgv_db_container: pgv_db_container,
            })
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_persist_setup_no_error_when_table_exists() {
        let test_context = TestContext::setup().await.expect("Test setup failed");

        test_context
            .pgv_storage
            .setup()
            .await
            .expect("PgVector setup should not fail when the table already exists");
    }
}
