use anyhow::Result;
use async_trait::async_trait;
use duckdb::params;
use swiftide_core::{
    indexing,
    template::{Context, Template},
    Persist,
};

use super::Duckdb;

const SCHEMA: &str = include_str!("schema.sql");
const UPSERT: &str = include_str!("upsert.sql");

#[async_trait]
impl Persist for Duckdb {
    async fn setup(&self) -> Result<()> {
        let mut context = Context::default();
        context.insert("table_name", &self.table_name);
        context.insert("vectors", &self.vectors);

        let rendered = Template::Static(SCHEMA).render(&context).await?;
        self.connection.lock().await.execute_batch(&rendered)?;

        context.insert(
            "vector_field_names",
            &self.vectors.keys().collect::<Vec<_>>(),
        );

        // User could have overridden the upsert sql
        // Which is fine
        let upsert = Template::Static(UPSERT).render(&context).await?;
        self.node_upsert_sql
            .set(upsert)
            .map_err(|_| anyhow::anyhow!("Failed to set upsert sql"))?;

        Ok(())
    }

    async fn store(&self, node: indexing::Node) -> Result<indexing::Node> {
        let Some(query) = self.node_upsert_sql.get() else {
            anyhow::bail!("Upsert sql in Duckdb not set");
        };

        let mut stmt = self.connection.lock().await.prepare(query)?;
        let value_iter = [
            node.id(),
            node.chunk,
            node.path,
            node.metadata,
        ]
        stmt.execute(params![
            node.id(),
            node.chunk,
            node.path,
            node.metadata,
            node.original_size,
            node.range.start,
            node.range.end,
            node.range.line_start,
            node.range.line_end,
            node.range.column_start,
            node.range.column_end,
        ])?;

        // TODO: Investigate concurrency in duckdb, maybe optmistic if it works
        self.connection
            .lock()
            .await
            .execute_batch(&self.node_upsert_sql.get())?;
    }

    async fn batch_store(&self, nodes: Vec<indexing::Node>) -> indexing::IndexingStream {
        todo!()
    }
}
