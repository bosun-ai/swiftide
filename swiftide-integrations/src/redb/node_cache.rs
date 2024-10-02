use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{indexing::Node, NodeCache};

use super::Redb;

#[async_trait]
impl NodeCache for Redb {
    #[tracing::instrument(skip_all)]
    async fn get(&self, node: &Node) -> bool {
        let table_definition = self.table_definition();
        let read_txn = self.database.begin_read().unwrap();

        let result = read_txn.open_table(table_definition);

        let table = match result {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist { .. }) => {
                // Create the table
                {
                    let write_txn = self
                        .database
                        .begin_write()
                        .expect("Failed to begin write transaction");

                    write_txn
                        .open_table(table_definition)
                        .expect("Failed to open table");

                    write_txn.commit().unwrap();
                }

                self.database
                    .begin_read()
                    .unwrap()
                    .open_table(table_definition)
                    .unwrap()
            }
            Err(e) => panic!("Failed to open table: {e:?}"),
        };

        match table.get(self.node_key(node)).unwrap() {
            Some(access_guard) => access_guard.value(),
            None => false,
        }
    }

    #[tracing::instrument(skip_all)]
    async fn set(&self, node: &Node) {
        let write_txn = self.database.begin_write().unwrap();

        let mut table = write_txn.open_table(self.table_definition()).unwrap();

        table.insert(self.node_key(node), true).unwrap();
    }

    /// Deletes the full cache table from the database.
    async fn clear(&self) -> Result<()> {
        let write_txn = self.database.begin_write().unwrap();
        let _ = write_txn.delete_table(self.table_definition());

        Ok(())
    }

    fn name(&self) -> &'static str {
        "redb"
    }
}
