use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{Context as _, Result};
use derive_builder::Builder;
use swiftide_core::{
    indexing::{Chunk, EmbeddedField},
    querying::search_strategies::HybridSearch,
};
use tera::Context;
use tokio::sync::RwLock;

pub mod node_cache;
pub mod persist;
pub mod retrieve;

const DEFAULT_INDEXING_SCHEMA: &str = include_str!("schema.sql");
const DEFAULT_UPSERT_QUERY: &str = include_str!("upsert.sql");
const DEFAULT_HYBRID_QUERY: &str = include_str!("hybrid_query.sql");

/// Provides `Persist`, `Retrieve`, and `NodeCache` for duckdb
///
/// Unfortunately Metadata is not stored.
///
/// Supports the following search strategies:
/// - `SimilaritySingleEmbedding`
/// - `HybridSearch` (<https://motherduck.com/blog/search-using-duckdb-part-3>/)
/// - Custom
///
/// NOTE: The integration is not optimized for ultra large datasets / load. It might work, if it
/// doesn't let us know <3.
#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct Duckdb<T: Chunk = String> {
    /// The connection to the database
    ///
    /// Note that this uses the tokio version of a mutex because the duckdb connection contains a
    /// `RefCell`. This is not ideal, but it is what it is.
    #[builder(setter(custom))]
    connection: Arc<Mutex<duckdb::Connection>>,

    /// The name of the table to use for storing nodes. Defaults to "swiftide".
    #[builder(default = "swiftide".into())]
    table_name: String,

    /// The schema to use for the table
    ///
    /// Note that if you change the schema, you probably also need to change the upsert query.
    ///
    /// Additionally, if you intend to use vectors, you must install and load the vss extension.
    #[builder(default = self.default_schema())]
    schema: String,

    // The vectors to be stored, field name -> size
    #[builder(default)]
    vectors: HashMap<EmbeddedField, usize>,

    /// Batch size for storing nodes
    #[builder(default = "256")]
    batch_size: usize,

    /// Sql to upsert a node
    #[builder(private, default = self.default_node_upsert_sql())]
    node_upsert_sql: String,

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

    /// If enabled, vectors will be upserted with an ON CONFLICT DO UPDATE. If disabled, ON
    /// conflict does nothing. Requires `duckdb` >= 1.2.1
    #[builder(default)]
    #[allow(dead_code)]
    upsert_vectors: bool,

    #[builder(default)]
    chunk_type: std::marker::PhantomData<T>,
}

impl<T: Chunk> std::fmt::Debug for Duckdb<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Duckdb")
            .field("connection", &"Arc<Mutex<duckdb::Connection>>")
            .field("table_name", &self.table_name)
            .field("batch_size", &self.batch_size)
            .finish()
    }
}

impl Duckdb<String> {
    pub fn builder() -> DuckdbBuilder<String> {
        DuckdbBuilder::<String>::default()
    }
}
impl<T: Chunk> Duckdb<T> {
    // pub fn builder() -> DuckdbBuilder<String> {
    //     DuckdbBuilder::<String>::default()
    // }

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

    /// Creates HNSW indices on the vector fields
    ///
    /// These are *not* persisted. You must recreate them on startup.
    ///
    /// If you want to persist them, refer to the duckdb documentation.
    ///
    /// # Errors
    ///
    /// Errors if the connection or statement fails
    ///
    /// # Panics
    ///
    /// If the mutex locking the connection is poisoned
    pub fn create_vector_indices(&self) -> Result<()> {
        let table_name = &self.table_name;
        let mut conn = self.connection.lock().unwrap();
        let tx = conn.transaction().context("Failed to start transaction")?;
        {
            for vector in self.vectors.keys() {
                tx.execute(
                    &format!(
                        "CREATE INDEX IF NOT EXISTS idx_{vector} ON {table_name} USING hnsw ({vector}) WITH (metric = 'cosine')",
                    ),
                    [],
                )
                .context("Could not create index")?;
            }
        }
        tx.commit().context("Failed to commit transaction")?;
        Ok(())
    }

    /// Safely creates the cache table if it does not exist. Can be used concurrently
    ///
    /// # Errors
    ///
    /// Errors if the table or index could not be created
    ///
    /// # Panics
    ///
    /// If the mutex locking the connection is poisoned
    pub async fn lazy_create_cache(&self) -> anyhow::Result<()> {
        if !*self.cache_table_created.read().await {
            let mut lock = self.cache_table_created.write().await;
            let conn = self.connection.lock().unwrap();
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
    pub fn node_key(&self, node: &swiftide_core::indexing::Node<T>) -> String {
        format!("{}.{}", self.cache_key_prefix, node.id())
    }

    fn hybrid_query_sql(
        &self,
        search_strategy: &HybridSearch,
        query: &str,
        embedding: &[f32],
    ) -> Result<String> {
        let table_name = &self.table_name;

        // Silently ignores multiple vector fields
        let (field_name, embedding_size) = self
            .vectors
            .iter()
            .next()
            .context("No vectors configured")?;

        if self.vectors.len() > 1 {
            tracing::warn!(
                "Multiple vectors configured, but only the first one will be used: {:?}",
                self.vectors
            );
        }

        let embedding = embedding
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");

        let context = Context::from_value(serde_json::json!({
            "table_name": table_name,
            "top_n": search_strategy.top_n(),
            "top_k": search_strategy.top_k(),
            "embedding_name": field_name,
            "embedding_size": embedding_size,
            "query": wrap_and_escape(query),
            "embedding": embedding,


        }))?;

        let rendered = tera::Tera::one_off(DEFAULT_HYBRID_QUERY, &context, false)?;
        Ok(rendered)
    }
}

fn wrap_and_escape(s: &str) -> String {
    let quote = '\'';
    let mut buf = String::new();
    buf.push(quote);
    let chars = s.chars();
    for ch in chars {
        // escape `quote` by doubling it
        if ch == quote {
            buf.push(ch);
        }
        buf.push(ch);
    }
    buf.push(quote);

    buf
}
impl<T: Chunk> DuckdbBuilder<T> {
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
        context.insert("vectors", &self.vectors.clone().unwrap_or_default());

        tera::Tera::one_off(DEFAULT_INDEXING_SCHEMA, &context, false)
            .expect("Could not render schema; infalllible")
    }

    fn default_node_upsert_sql(&self) -> String {
        let mut context = Context::default();
        context.insert("table_name", &self.table_name);
        context.insert("vectors", &self.vectors.clone().unwrap_or_default());
        context.insert("upsert_vectors", &self.upsert_vectors);

        context.insert(
            "vector_field_names",
            &self
                .vectors
                .as_ref()
                .map(|v| v.keys().collect::<Vec<_>>())
                .unwrap_or_default(),
        );

        tracing::info!("Rendering upsert sql");
        tera::Tera::one_off(DEFAULT_UPSERT_QUERY, &context, false)
            .expect("could not render upsert query; infallible")
    }
}
