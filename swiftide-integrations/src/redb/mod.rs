//! Redb is a simple, portable, high-performance, ACID, embedded key-value store.
//!
//! Redb can be used as a fast, embedded node cache, without the need for external services.

use anyhow::Result;
use std::{path::PathBuf, sync::Arc};

use derive_builder::Builder;

mod node_cache;

/// `Redb` provides a caching filter for indexing nodes using Redb.
///
/// Redb is a simple, portable, high-performance, ACID, embedded key-value store.
/// It enables using a local file based cache without the need for external services.
///
/// # Example
///
/// ```no_run
/// # use swiftide_integrations::redb::{Redb};
/// Redb::builder()
///     .database_path("/my/redb")
///     .table_name("swiftide_test")
///     .cache_key_prefix("my_cache")
///     .build().unwrap();
/// ```
#[derive(Clone, Builder)]
#[builder(build_fn(error = "anyhow::Error"), setter(into))]
pub struct Redb {
    /// The database to use for caching nodes. Allows overwriting the default database created from
    /// `database_path`.
    #[builder(setter(into), default = "Arc::new(self.default_database()?)")]
    database: Arc<redb::Database>,

    /// Path to the database, required if no database override is provided. This is the recommended
    /// usage.
    #[builder(setter(into, strip_option))]
    database_path: Option<PathBuf>,
    /// The name of the table to use for caching nodes. Defaults to "swiftide".
    #[builder(default = "\"swiftide\".to_string()")]
    table_name: String,
    /// Prefix to be used for keys stored in the database to avoid collisions. Can be used to
    /// manually invalidate the cache.
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

    pub fn table_definition(&self) -> redb::TableDefinition<String, bool> {
        redb::TableDefinition::<String, bool>::new(&self.table_name)
    }

    pub fn database(&self) -> &redb::Database {
        &self.database
    }
}
