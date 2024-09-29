use async_trait::async_trait;
use swiftide_core::{indexing::Node, NodeCache};

use super::Redb;

#[async_trait]
impl NodeCache for Redb<'_> {
    #[tracing::instrument(skip_all)]
    async fn get(&self, node: &Node) -> bool {
        let read_txn = self.database.begin_read().unwrap();
        let table = read_txn.open_table(self.table).unwrap();

        match table.get(self.node_key(node)).unwrap() {
            Some(access_guard) => access_guard.value(),
            None => false,
        }
    }

    #[tracing::instrument(skip_all)]
    async fn set(&self, node: &Node) {
        let write_txn = self.database.begin_write().unwrap();
        // TODO: Either mutex this, or better, parallel buffered write
        let mut table = write_txn.open_table(self.table).unwrap();

        table.insert(self.node_key(node), true).unwrap();
    }
}
