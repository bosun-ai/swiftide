use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use anyhow::Context as _;
use derive_builder::Builder;
use swiftide_core::indexing::EmbeddedField;
use tera::Context;
use tokio::sync::{Mutex, RwLock};

pub mod node_cache;
pub mod persist;
pub mod retrieve;

const DEFAULT_INDEXING_SCHEMA: &str = include_str!("schema.sql");

/// Provides `Persist`, `Retrieve`, and `NodeCache` for duckdb
///
/// Unfortunately Metadata is not stored.
///
/// By default `hnsw_enable_experimental_persistance` is enabled.
///
/// NOTE: The integration is not optimized for ultra large datasets / load. It might work, if it
/// doesn't let us know <3.
#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct Duckdb {
    /// The connection to the database
    #[builder(setter(custom))]
    connection: Arc<Mutex<duckdb::Connection>>, // should be rwlock, however, internally duckdb uses a refcell, so we need the mutex for sync :(

    /// The name of the table to use for storing nodes. Defaults to "swiftide".
    #[builder(default = "swiftide".into())]
    table_name: String,

    /// The schema to use for the table
    /// You *can* customize it. However, it is recommended to use the default schema.
    ///
    /// The generated upsert statement expects uuid, path, and vector columns.
    #[builder(default = "self.default_schema()")]
    schema: String,

    // The vectors to be stored, field name -> size
    #[builder(default)]
    vectors: HashMap<EmbeddedField, usize>,

    /// Batch size for storing nodes
    #[builder(default = "256")]
    batch_size: usize,

    /// Sql to upsert a node
    #[builder(private, default = OnceLock::new())]
    node_upsert_sql: OnceLock<String>,

    /// Name of the table to use for caching nodes. Defaults to `"swiftide_cache"`.
    #[builder(default = "swiftide_cache".into())]
    cache_table: String,

    /// Tracks if the cache table has been created
    #[builder(private, default = Arc::new(false.into()))]
    cache_table_created: Arc<RwLock<bool>>, // note might need a mutex

    /// Prefix to be used for keys stored in the database to avoid collisions. Can be used to
    /// manually invalidate the cache.
    #[builder(default = "String::new()")]
    cache_key_prefix: String,
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

    /// Name of the indexing table
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Name of the cache table
    pub fn cache_table(&self) -> &str {
        &self.cache_table
    }

    /// Returns the connection to the database
    pub fn connection(&self) -> &Mutex<duckdb::Connection> {
        &self.connection
    }

    /// Safely creates the cache table if it does not exist. Can be used concurrently
    ///
    /// # Errors
    ///
    /// Errors if the table or index could not be created
    pub async fn lazy_create_cache(&self) -> anyhow::Result<()> {
        if !*self.cache_table_created.read().await {
            let mut lock = self.cache_table_created.write().await;
            let conn = self.connection.lock().await;
            conn.execute(
                &format!(
                    "CREATE TABLE IF NOT EXISTS {} (uuid TEXT PRIMARY KEY, path TEXT)",
                    self.cache_table
                ),
                [],
            )
            .context("Could not create table")?;
            // Create an extra index on path
            conn.execute(
                &format!(
                    "CREATE INDEX IF NOT EXISTS idx_path ON {} (path)",
                    self.cache_table
                ),
                [],
            )
            .context("Could not create index")?;
            *lock = true;
        }
        Ok(())
    }

    /// Formats a node key for the cache table
    pub fn node_key(&self, node: &swiftide_core::indexing::Node) -> String {
        format!("{}.{}", self.cache_key_prefix, node.id())
    }
}

impl DuckdbBuilder {
    pub fn connection(&mut self, connection: impl Into<duckdb::Connection>) -> &mut Self {
        self.connection = Some(Arc::new(Mutex::new(connection.into())));
        self
    }

    pub fn with_vector(&mut self, field: EmbeddedField, size: usize) -> &mut Self {
        self.vectors
            .get_or_insert_with(HashMap::new)
            .insert(field, size);
        self
    }

    fn default_schema(&self) -> String {
        let mut context = Context::default();
        context.insert("table_name", &self.table_name);
        context.insert("vectors", &self.vectors);

        tera::Tera::one_off(DEFAULT_INDEXING_SCHEMA, &context, false)
            .expect("Could not render schema")
    }
}
