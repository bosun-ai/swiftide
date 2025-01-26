use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use derive_builder::Builder;
use swiftide_core::indexing::EmbeddedField;
use tokio::sync::Mutex;

pub mod persist;
pub mod retrieve;

#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct Duckdb {
    #[builder(setter(custom))]
    connection: Arc<Mutex<duckdb::Connection>>,
    table_name: String,

    // The vectors to be stored, field name -> size
    vectors: HashMap<EmbeddedField, usize>,

    #[builder(default = "256")]
    batch_size: usize,

    #[builder(default = OnceLock::new())]
    node_upsert_sql: OnceLock<String>,
}

impl std::fmt::Debug for Duckdb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Duckdb")
            .field("connection", &"Arc<Mutex<duckdb::Connection>>")
            .field("table_name", &self.table_name)
            .field("batch_size", &self.batch_size)
            .finish()
    }
}

impl Duckdb {
    pub fn builder() -> DuckdbBuilder {
        DuckdbBuilder::default()
    }

    pub async fn connection(&self) -> &Mutex<duckdb::Connection> {
        &self.connection
    }
}

impl DuckdbBuilder {
    pub fn connection(
        &mut self,
        connection: impl Into<Arc<Mutex<duckdb::Connection>>>,
    ) -> &mut Self {
        self.connection = Some(connection.into());
        self
    }
}
