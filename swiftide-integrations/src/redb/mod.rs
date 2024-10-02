//! Redb is a simple, portable, high-performance, ACID, embedded key-value store.
//!
//! Redb can be used as a fast, embedded node cache, without the need for external services.

use anyhow::Result;
use std::{path::PathBuf, sync::Arc};

use derive_builder::Builder;

mod node_cache;

#[derive(Clone, Builder)]
#[builder(build_fn(error = "anyhow::Error"), setter(into))]
pub struct Redb {
    #[builder(setter(into), default = "Arc::new(self.default_database()?)")]
    database: Arc<redb::Database>,

    /// Path to the database, required if no database override is provided
    #[builder(setter(into, strip_option))]
    database_path: Option<PathBuf>,
    #[builder(default = "\"swiftide\".to_string()")]
    table_name: String,
    #[builder(default = "String::new()")]
    cache_key_prefix: String,
}

impl std::fmt::Debug for Redb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redb")
            .field("database", &self.database)
            .field("database_path", &self.database_path)
            .field("table_name", &self.table_name)
            .field("cache_key_prefix", &self.cache_key_prefix)
            .finish()
    }
}

impl RedbBuilder {
    fn default_database(&self) -> Result<redb::Database> {
        let db = redb::Database::create(
            self.database_path
                .clone()
                .flatten()
                .ok_or(anyhow::anyhow!("Expected database path"))?,
        )?;

        Ok(db)
    }
}

impl Redb {
    pub fn builder() -> RedbBuilder {
        RedbBuilder::default()
    }
    pub fn node_key(&self, node: &swiftide_core::indexing::Node) -> String {
        format!("{}.{}", self.cache_key_prefix, node.id())
    }

    fn table_definition(&self) -> redb::TableDefinition<String, bool> {
        // TODO: Should this be static?
        redb::TableDefinition::<String, bool>::new(&self.table_name)
    }
}
