use anyhow::Context as _;
use async_trait::async_trait;
use swiftide_core::{indexing::Node, NodeCache};

use super::Duckdb;

macro_rules! unwrap_or_log {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                tracing::error!("Error: {:#}", e);
                debug_assert!(
                    true,
                    "Duckdb should not give errors unless in very weird situations; this is a bug: {:#}",
                    e
                );
                return false;
            }
        }
    };
}

#[async_trait]
impl NodeCache for Duckdb {
    async fn get(&self, node: &Node) -> bool {
        unwrap_or_log!(self
            .lazy_create_cache()
            .await
            .context("failed to create cache table"));

        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE uuid = ?)",
            &self.cache_table
        );

        let lock = self.connection.lock().unwrap();
        let mut stmt = unwrap_or_log!(lock
            .prepare(&sql)
            .context("Failed to prepare duckdb statement for persist"));

        let present = unwrap_or_log!(stmt
            .query_map([self.node_key(node)], |row| row.get::<_, bool>(0))
            .context("failed to query for documents"))
        .next()
        .transpose();

        unwrap_or_log!(present).unwrap_or(false)
    }

    async fn set(&self, node: &Node) {
        if let Err(err) = self
            .lazy_create_cache()
            .await
            .context("failed to create cache table")
        {
            tracing::error!("Failed to create cache table: {:#}", err);
            return;
        }

        let sql = format!(
            "INSERT INTO {} (uuid, path) VALUES (?, ?) ON CONFLICT (uuid) DO NOTHING",
            &self.cache_table
        );

        let lock = self.connection.lock().unwrap();
        let mut stmt = match lock
            .prepare(&sql)
            .context("Failed to prepare duckdb statement for cache set")
        {
            Ok(stmt) => stmt,
            Err(err) => {
                tracing::error!(
                    "Failed to prepare duckdb statement for cache set: {:#}",
                    err
                );
                return;
            }
        };

        if let Err(err) = stmt
            .execute([self.node_key(node), node.path.to_string_lossy().into()])
            .context("failed to insert into cache table")
        {
            tracing::error!("Failed to insert into cache table: {:#}", err);
        }
    }

    async fn clear(&self) -> anyhow::Result<()> {
        let sql = format!("DROP TABLE IF EXISTS {}", &self.cache_table);
        let lock = self.connection.lock().unwrap();
        let mut stmt = lock
            .prepare(&sql)
            .context("Failed to prepare duckdb statement for cache clear")?;

        stmt.execute([]).context("failed to delete cache table")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swiftide_core::indexing::Node;

    fn setup_duckdb() -> Duckdb {
        Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn test_get_set() {
        let duckdb = setup_duckdb();
        let node = Node::new("test_get_set");

        assert!(!duckdb.get(&node).await);
        duckdb.set(&node).await;
        assert!(duckdb.get(&node).await);
    }

    #[tokio::test]
    async fn test_clear() {
        let duckdb = setup_duckdb();
        let node = Node::new("test_clear");

        duckdb.set(&node).await;
        assert!(duckdb.get(&node).await);
        duckdb.clear().await.unwrap();
        assert!(!duckdb.get(&node).await);
    }
}
