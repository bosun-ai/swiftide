//! This module integrates with the pgvector database, providing functionalities to create and manage vector collections,
//! store data, and optimize indexing for efficient searches.
//!
//! pgvector is utilized in both the `indexing::Pipeline` and `query::Pipeline` modules.

#[cfg(test)]
mod fixtures;

mod persist;
mod pgv_table_types;
use anyhow::Result;
use derive_builder::Builder;
use once_cell::sync::OnceCell;
use sqlx::PgPool;
use std::fmt;
use std::sync::Arc;
use tokio::time::Duration;

use pgv_table_types::{FieldConfig, MetadataConfig, VectorConfig};

/// Default maximum connections for the database connection pool.
const DB_POOL_CONN_MAX: u32 = 10;

/// Default maximum retries for database connection attempts.
const DB_POOL_CONN_RETRY_MAX: u32 = 3;

/// Delay between connection retry attempts, in seconds.
const DB_POOL_CONN_RETRY_DELAY_SECS: u64 = 3;

/// Default batch size for storing nodes.
const BATCH_SIZE: usize = 50;

/// Represents a Pgvector client with configuration options.
///
/// This struct is used to interact with the Pgvector vector database, providing methods to manage vector collections,
/// store data, and ensure efficient searches. The client can be cloned with low cost as it shares connections.
#[derive(Builder, Clone)]
#[builder(setter(into, strip_option), build_fn(error = "anyhow::Error"))]
pub struct PgVector {
    /// Name of the table to store vectors.
    #[builder(default = "String::from(\"swiftide_pgv_store\")")]
    table_name: String,

    /// Default vector size; can be customized per configuration.
    vector_size: i32,

    /// Batch size for storing nodes.
    #[builder(default = "BATCH_SIZE")]
    batch_size: usize,

    /// Field configurations for the `PgVector` table schema.
    ///
    /// Supports multiple field types (see [`FieldConfig`]).
    #[builder(default)]
    fields: Vec<FieldConfig>,

    /// Database connection URL.
    db_url: String,

    /// Maximum connections allowed in the connection pool.
    #[builder(default = "DB_POOL_CONN_MAX")]
    db_max_connections: u32,

    /// Maximum retry attempts for establishing a database connection.
    #[builder(default = "DB_POOL_CONN_RETRY_MAX")]
    db_max_retry: u32,

    /// Delay between retry attempts for database connections.
    #[builder(default = "Duration::from_secs(DB_POOL_CONN_RETRY_DELAY_SECS)")]
    db_conn_retry_delay: Duration,

    /// Lazy-initialized database connection pool.
    #[builder(default = "Arc::new(OnceCell::new())")]
    connection_pool: Arc<OnceCell<PgPool>>,
}

impl fmt::Debug for PgVector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgVector")
            .field("table_name", &self.table_name)
            .field("vector_size", &self.vector_size)
            .field("batch_size", &self.batch_size)
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
    pub async fn get_pool(&self) -> Result<&PgPool> {
        self.pool_get_or_initialize().await
    }
}

impl PgVectorBuilder {
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

#[cfg(test)]
mod tests {
    use crate::pgvector::fixtures::{PgVectorTestData, TestContext};
    use futures_util::TryStreamExt;
    use std::collections::HashSet;
    use swiftide_core::{
        indexing::{self, EmbedMode, EmbeddedField},
        Persist,
    };
    use test_case::test_case;

    #[test_case(
        // SingleWithMetadata - No Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_no_meta_1",
                metadata: None,
                vectors: vec![PgVectorTestData::create_test_vector(EmbeddedField::Combined, 1.0)],
            },
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_no_meta_2",
                metadata: None,
                vectors: vec![PgVectorTestData::create_test_vector(EmbeddedField::Combined, 1.1)],
            }
        ],
        HashSet::from([EmbeddedField::Combined])
        ; "SingleWithMetadata mode without metadata")]
    #[test_case(
        // SingleWithMetadata - With Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_with_meta_1",
                metadata: Some(vec![
                    ("category", "A"),
                    ("priority", "high")
                ].into()),
                vectors: vec![PgVectorTestData::create_test_vector(EmbeddedField::Combined, 1.2)],
            },
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_with_meta_2",
                metadata: Some(vec![
                    ("category", "B"),
                    ("priority", "low")
                ].into()),
                vectors: vec![PgVectorTestData::create_test_vector(EmbeddedField::Combined, 1.3)],
            }
        ],
        HashSet::from([EmbeddedField::Combined])
        ; "SingleWithMetadata mode with metadata")]
    #[test_case(
        // PerField - No Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::PerField,
                chunk: "per_field_no_meta_1",
                metadata: None,
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 1.2),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("category".into()), 2.2),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("priority".into()), 3.2),
                ],
            },
            PgVectorTestData {
                embed_mode: EmbedMode::PerField,
                chunk: "per_field_no_meta_2",
                metadata: None,
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 1.3),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("category".into()), 2.3),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("priority".into()), 3.3),
                ],
            }
        ],
        HashSet::from([
            EmbeddedField::Chunk,
            EmbeddedField::Metadata("category".into()),
            EmbeddedField::Metadata("priority".into()),
        ])
        ; "PerField mode without metadata")]
    #[test_case(
        // PerField - With Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::PerField,
                chunk: "single_with_meta_1",
                metadata: Some(vec![
                    ("category", "A"),
                    ("priority", "high")
                ].into()),
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 1.2),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("category".into()), 2.2),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("priority".into()), 3.2),
                ],
            },
            PgVectorTestData {
                embed_mode: EmbedMode::PerField,
                chunk: "single_with_meta_2",
                metadata: Some(vec![
                    ("category", "B"),
                    ("priority", "low")
                ].into()),
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 1.3),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("category".into()), 2.3),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("priority".into()), 3.3),
                ],
            }
        ],
        HashSet::from([
            EmbeddedField::Chunk,
            EmbeddedField::Metadata("category".into()),
            EmbeddedField::Metadata("priority".into()),
        ])
        ; "PerField mode with metadata")]
    #[test_case(
        // Both - No Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::Both,
                chunk: "both_no_meta_1",
                metadata: None,
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Combined, 3.0),
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 3.1)
                ],
            },
            PgVectorTestData {
                embed_mode: EmbedMode::Both,
                chunk: "both_no_meta_2",
                metadata: None,
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Combined, 3.2),
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 3.3)
                ],
            }
        ],
        HashSet::from([EmbeddedField::Combined, EmbeddedField::Chunk])
        ; "Both mode without metadata")]
    #[test_case(
        // Both - With Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::Both,
                chunk: "both_with_meta_1",
                metadata: Some(vec![
                    ("category", "P"),
                    ("priority", "urgent"),
                    ("tag", "test1")
                ].into()),
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Combined, 3.4),
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 3.5),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("category".into()), 3.6),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("priority".into()), 3.7),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("tag".into()), 3.8)
                ],
            },
            PgVectorTestData {
                embed_mode: EmbedMode::Both,
                chunk: "both_with_meta_2",
                metadata: Some(vec![
                    ("category", "Q"),
                    ("priority", "low"),
                    ("tag", "test2")
                ].into()),
                vectors: vec![
                    PgVectorTestData::create_test_vector(EmbeddedField::Combined, 3.9),
                    PgVectorTestData::create_test_vector(EmbeddedField::Chunk, 4.0),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("category".into()), 4.1),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("priority".into()), 4.2),
                    PgVectorTestData::create_test_vector(EmbeddedField::Metadata("tag".into()), 4.3)
                ],
            }
        ],
        HashSet::from([
            EmbeddedField::Combined,
            EmbeddedField::Chunk,
            EmbeddedField::Metadata("category".into()),
            EmbeddedField::Metadata("priority".into()),
            EmbeddedField::Metadata("tag".into()),
        ])
        ; "Both mode with metadata")]
    #[test_log::test(tokio::test)]
    async fn test_persist_nodes(
        test_cases: Vec<PgVectorTestData<'_>>,
        vector_fields: HashSet<EmbeddedField>,
    ) {
        // Extract all possible metadata fields from test cases
        let metadata_fields: Vec<&str> = test_cases
            .iter()
            .filter_map(|case| case.metadata.as_ref())
            .flat_map(|metadata| metadata.iter().map(|(key, _)| key.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Initialize test context with all required metadata fields
        let test_context = TestContext::setup_with_cfg(Some(metadata_fields), vector_fields)
            .await
            .expect("Test setup failed");

        // Convert test cases to nodes and store them
        let nodes: Vec<indexing::Node> = test_cases.iter().map(PgVectorTestData::to_node).collect();

        // Test batch storage
        let stored_nodes = test_context
            .pgv_storage
            .batch_store(nodes.clone())
            .await
            .try_collect::<Vec<_>>()
            .await
            .expect("Failed to store nodes");

        assert_eq!(
            stored_nodes.len(),
            nodes.len(),
            "All nodes should be stored"
        );

        // Verify storage for each test case
        for (test_case, stored_node) in test_cases.iter().zip(stored_nodes.iter()) {
            // 1. Verify basic node properties
            assert_eq!(
                stored_node.chunk, test_case.chunk,
                "Stored chunk should match"
            );
            assert_eq!(
                stored_node.embed_mode, test_case.embed_mode,
                "Embed mode should match"
            );

            // 2. Verify vectors were stored correctly
            let stored_vectors = stored_node
                .vectors
                .as_ref()
                .expect("Vectors should be present");
            assert_eq!(
                stored_vectors.len(),
                test_case.vectors.len(),
                "Vector count should match"
            );
        }
    }
}
