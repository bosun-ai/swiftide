//! This module integrates with the pgvector database, providing functionalities to create and manage vector collections,
//! store data, and optimize indexing for efficient searches.
//!
//! pgvector is utilized in both the `indexing::Pipeline` and `query::Pipeline` modules.

mod persist;
mod pgv_table_types;

use anyhow::Result;
use derive_builder::Builder;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::fmt;

use pgv_table_types::{FieldConfig, MetadataConfig, VectorConfig};

const PG_POOL_MAX_CONN: u32 = 10;
const DEFAULT_BATCH_SIZE: usize = 50;
const DEFAULT_VEC_DIM: i32 = 384;

/// Represents a Pgvector client with configuration options.
///
/// This struct is used to interact with the Pgvector vector database, providing methods to manage vector collections,
/// store data, and ensure efficient searches. The client can be cloned with low cost as it shares connections.
#[derive(Builder, Clone)]
#[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
pub struct PgVector {
    /// Database connection pool.
    #[builder(default)]
    connection_pool: Option<PgPool>,

    /// Table name to store vectors in.
    #[builder(default = "String::from(\"swiftide_pgv_store\")")]
    table_name: String,

    /// Size of the vectors to store.
    #[builder(default = "DEFAULT_VEC_DIM")]
    vector_size: i32,

    /// Batch size for storing nodes.
    #[builder(default = "Some(DEFAULT_BATCH_SIZE)")]
    batch_size: Option<usize>,

    /// Field configuration for the Pgvector table, determining the eventual table schema.
    ///
    /// Supports multiple field types; see [`FieldConfig`] for details.
    #[builder(default)]
    fields: Vec<FieldConfig>,
}

impl fmt::Debug for PgVector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let connection_status = if let Some(pool) = &self.connection_pool {
            if pool.is_closed() {
                "Closed"
            } else {
                "Open"
            }
        } else {
            "Not initialized"
        };

        f.debug_struct("PgVector")
            .field("table_name", &self.table_name)
            .field("vector_size", &self.vector_size)
            .field("batch_size", &self.batch_size)
            .field("connection_status", &connection_status)
            .finish()
    }
}

impl PgVector {
    /// Creates a new `PgVectorBuilder` instance using the default configuration.
    ///
    /// # Returns
    ///
    /// * `PgVectorBuilder` - A builder instance that can be used to configure
    ///   and construct a `PgVector` object.
    ///
    /// This function returns a default `PgVectorBuilder` that can be customized
    /// using builder methods such as `with_vector`, `with_metadata`, and others.
    pub fn builder() -> PgVectorBuilder {
        PgVectorBuilder::default()
    }

    /// The `get_pool` function retrieves a reference to the connection pool associated with this object.
    ///
    /// # Returns
    ///
    /// * `Option<&PgPool>` - A reference to the connection pool if it exists, otherwise `None`.
    ///
    /// # Arguments
    ///
    /// None
    ///
    /// # Errors
    ///
    /// This function does not return any errors.
    /// It will always return a `Some` containing a reference to the `PgPool` if available, or `None` if no pool is set.
    pub fn get_pool(&self) -> Option<&PgPool> {
        self.connection_pool.as_ref()
    }
}

impl PgVectorBuilder {
    /// Tries to create a `PgVectorBuilder` from a given URL.
    ///
    /// Returns
    ///
    /// # Arguments
    ///
    /// * `url` - A string slice that holds the URL for the Pgvector client.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `PgVectorBuilder` if successful, or an error otherwise.
    ///
    /// # Errors
    ///
    /// Errors if client fails build
    pub async fn try_from_url(
        mut self,
        url: impl AsRef<str>,
        connection_max: Option<u32>,
    ) -> Result<Self> {
        self.connection_pool = Some(Some(
            PgPoolOptions::new()
                .max_connections(connection_max.unwrap_or(PG_POOL_MAX_CONN))
                .connect(url.as_ref())
                .await?,
        ));
        Ok(self)
    }

    /// The `with_vector` function adds a vector configuration to the object.
    ///
    /// # Arguments
    ///
    /// * `config` - A configuration that can be converted into a `VectorConfig`.
    ///
    /// # Returns
    ///
    /// * `&mut Self` - A mutable reference to the current object, allowing method chaining.
    ///
    /// This function will no longer panic.
    pub fn with_vector(&mut self, config: impl Into<VectorConfig>) -> &mut Self {
        // Use `get_or_insert_with` to initialize `fields` if it's `None`
        self.fields
            .get_or_insert_with(Self::default_fields)
            .push(FieldConfig::Vector(config.into()));

        self
    }

    /// Adds a metadata configuration to the object.
    ///
    /// # Arguments
    ///
    /// * `config` - The metadata configuration to add.
    ///
    /// # Returns
    ///
    /// * `&mut Self` - A mutable reference to the current object, allowing method chaining.
    ///
    /// This function will no longer panic.
    pub fn with_metadata(&mut self, config: impl Into<MetadataConfig>) -> &mut Self {
        // Use `get_or_insert_with` to initialize `fields` if it's `None`
        self.fields
            .get_or_insert_with(Self::default_fields)
            .push(FieldConfig::Metadata(config.into()));

        self
    }

    fn default_fields() -> Vec<FieldConfig> {
        vec![FieldConfig::ID, FieldConfig::Chunk]
    }
}
