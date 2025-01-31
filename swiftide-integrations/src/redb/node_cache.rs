use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{indexing::Node, NodeCache};

use super::Redb;

// Simple proc macro that gets the ok value of a result or logs the error and returns false (not
// cached)
//
// The underlying issue is that redb can be fickly if panics happened. We just want to make sure it
// does not become worse. There probably is a better solution.
macro_rules! unwrap_or_log {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                tracing::error!("Error: {:#}", e);
                debug_assert!(
                    true,
                    "Redb should not give errors unless in very weird situations; this is a bug: {:#}",
                    e
                );
                return false;
            }
        }
    };
}
#[async_trait]
impl NodeCache for Redb {
    async fn get(&self, node: &Node) -> bool {
        let table_definition = self.table_definition();
        let read_txn = unwrap_or_log!(self.database.begin_read());

        let result = read_txn.open_table(table_definition);

        let table = match result {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist { .. }) => {
                // Create the table
                {
                    let write_txn = unwrap_or_log!(self.database.begin_write());

                    unwrap_or_log!(write_txn.open_table(table_definition));
                    unwrap_or_log!(write_txn.commit());
                }

                let read_tx = unwrap_or_log!(self.database.begin_read());
                unwrap_or_log!(read_tx.open_table(table_definition))
            }
            Err(e) => {
                tracing::error!("Failed to open table: {e:#}");
                return false;
            }
        };

        match table.get(self.node_key(node)).unwrap() {
            Some(access_guard) => access_guard.value(),
            None => false,
        }
    }

    async fn set(&self, node: &Node) {
        let write_txn = self.database.begin_write().unwrap();

        {
            let mut table = write_txn.open_table(self.table_definition()).unwrap();

            table.insert(self.node_key(node), true).unwrap();
        }
        write_txn.commit().unwrap();
    }

    /// Deletes the full cache table from the database.
    async fn clear(&self) -> Result<()> {
        let write_txn = self.database.begin_write().unwrap();
        let _ = write_txn.delete_table(self.table_definition());

        write_txn.commit().unwrap();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swiftide_core::indexing::Node;
    use temp_dir::TempDir;

    fn setup_redb() -> Redb {
        let tempdir = TempDir::new().unwrap();
        Redb::builder()
            .database_path(tempdir.child("test_clear"))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn test_get_set() {
        let redb = setup_redb();
        let node = Node::new("test_get_set");
        assert!(!redb.get(&node).await);
        redb.set(&node).await;
        assert!(redb.get(&node).await);
    }
    #[tokio::test]
    async fn test_clear() {
        let redb = setup_redb();
        let node = Node::new("test_clear");
        redb.set(&node).await;
        assert!(redb.get(&node).await);
        redb.clear().await.unwrap();
        assert!(!redb.get(&node).await);
    }
}
