//! This module integrates with the pgvector database, providing functionalities to create and manage vector collections,
//! store data, and optimize indexing for efficient searches.
//!
//! pgvector is utilized in both the `indexing::Pipeline` and `query::Pipeline` modules.
mod persist;
mod pgv_table_types;
use anyhow::Result;
use derive_builder::Builder;
use sqlx::PgPool;
use std::fmt;

use pgv_table_types::{FieldConfig, MetadataConfig, PgDBConnectionPool, VectorConfig};

const DEFAULT_BATCH_SIZE: usize = 50;

/// Represents a Pgvector client with configuration options.
///
/// This struct is used to interact with the Pgvector vector database, providing methods to manage vector collections,
/// store data, and ensure efficient searches. The client can be cloned with low cost as it shares connections.
#[derive(Builder, Clone)]
#[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
pub struct PgVector {
    /// Database connection pool.
    #[builder(default = "PgDBConnectionPool::default()")]
    connection_pool: PgDBConnectionPool,

    /// Table name to store vectors in.
    #[builder(default = "String::from(\"swiftide_pgv_store\")")]
    table_name: String,

    /// Default sizes of vectors. Vectors can also be of different
    /// sizes by specifying the size in the vector configuration.
    vector_size: Option<i32>,

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
        // Access the connection pool synchronously and determine the status.
        let connection_status = self.connection_pool.connection_status();

        f.debug_struct("PgVector")
            .field("table_name", &self.table_name)
            .field("vector_size", &self.vector_size)
            .field("batch_size", &self.batch_size)
            .field("connection_status", &connection_status)
            .finish()
    }
}

impl PgVector {
    /// Creates a new instance of `PgVectorBuilder` with default settings.
    ///
    /// # Returns
    ///
    /// A new `PgVectorBuilder`.
    pub fn builder() -> PgVectorBuilder {
        PgVectorBuilder::default()
    }

    /// Retrieves a connection pool for `PostgreSQL`.
    ///
    /// This function returns the connection pool used for interacting with the `PostgreSQL` database.
    /// It fetches the pool from the `PgDBConnectionPool` struct.
    ///
    /// # Returns
    ///
    /// A `Result` that, on success, contains the `PgPool` representing the database connection pool.
    /// On failure, an error is returned.
    ///
    /// # Errors
    ///
    /// This function will return an error if it fails to retrieve the connection pool, which could occur
    /// if the underlying connection to `PostgreSQL` has not been properly established.
    pub fn get_pool(&self) -> Result<PgPool> {
        self.connection_pool.get_pool()
    }
}

impl PgVectorBuilder {
    /// Tries to asynchronously connect to a `Postgres` server and initialize a connection pool.
    ///
    /// This function attempts to establish a connection to the specified `Postgres` server and
    /// sets up a connection pool with an optional maximum number of connections.
    ///
    /// # Arguments
    ///
    /// * `url` - A string reference representing the URL of the `Postgres` server to connect to.
    /// * `connection_max` - An optional value specifying the maximum number of connections in the pool.
    ///
    /// # Returns
    ///
    /// A `Result` that contains an updated `PgVector` instance with the new connection pool on success.
    /// On failure, an error is returned.
    ///
    /// # Errors
    ///
    /// This function returns an error if the connection to the database fails or if retries are exhausted.
    /// Possible reasons include invalid database URLs, unreachable servers, or exceeded retry limits.
    pub async fn try_connect_to_pool(
        mut self,
        url: impl AsRef<str>,
        connection_max: Option<u32>,
    ) -> Result<Self> {
        let pool = self.connection_pool.clone().unwrap_or_default();

        self.connection_pool = Some(pool.try_connect_to_url(url, connection_max).await?);

        Ok(self)
    }

    /// Adds a vector configuration to the builder.
    ///
    /// # Arguments
    ///
    /// * `config` - The vector configuration to add, which can be converted into a `VectorConfig`.
    ///
    /// # Returns
    ///
    /// A mutable reference to the builder with the new vector configuration added.
    pub fn with_vector(&mut self, config: impl Into<VectorConfig>) -> &mut Self {
        // Use `get_or_insert_with` to initialize `fields` if it's `None`
        self.fields
            .get_or_insert_with(Self::default_fields)
            .push(FieldConfig::Vector(config.into()));

        self
    }

    /// Sets the metadata configuration for the vector similarity search.
    ///
    /// This method allows you to specify metadata configurations for vector similarity search using `MetadataConfig`.
    /// The provided configuration will be added as a new field in the builder.
    ///
    /// # Arguments
    ///
    /// * `config` - The metadata configuration to use.
    ///
    /// # Returns
    ///
    /// * Returns a mutable reference to `self` for method chaining.
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
