//! Integration module for `PostgreSQL` vector database (pgvector) operations.
//!
//! This module provides a client interface for vector similarity search operations using pgvector,
//! supporting:
//! - Vector collection management with configurable schemas
//! - Efficient vector storage and indexing
//! - Connection pooling with automatic retries
//! - Batch operations for optimized performance
//! - Metadata included in retrieval
//!
//! The functionality is primarily used through the [`PgVector`] client, which implements
//! the [`Persist`] trait for seamless integration with indexing and query pipelines.
//!
//! # Example
//! ```rust
//! # use swiftide_integrations::pgvector::PgVector;
//! # async fn example() -> anyhow::Result<()> {
//! let client = PgVector::builder()
//!     .db_url("postgresql://localhost:5432/vectors")
//!     .vector_size(384)
//!     .build()?;
//!
//! # Ok(())
//! # }
//! ```
#[cfg(test)]
mod fixtures;

mod persist;
mod pgv_table_types;
mod retrieve;
use anyhow::Result;
use derive_builder::Builder;
use sqlx::PgPool;
use std::fmt;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::time::Duration;

pub use pgv_table_types::{FieldConfig, MetadataConfig, VectorConfig};

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
/// This struct is used to interact with the Pgvector vector database, providing methods to manage
/// vector collections, store data, and ensure efficient searches. The client can be cloned with low
/// cost as it shares connections.
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
    #[builder(default = "Arc::new(OnceLock::new())")]
    connection_pool: Arc<OnceLock<PgPool>>,

    /// SQL statement used for executing bulk insert.
    #[builder(default = "Arc::new(OnceLock::new())")]
    sql_stmt_bulk_insert: Arc<OnceLock<String>>,
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
    /// This function returns the connection pool used for interacting with the `PostgreSQL`
    /// database. It fetches the pool from the `PgDBConnectionPool` struct.
    ///
    /// # Returns
    ///
    /// A `Result` that, on success, contains the `PgPool` representing the database connection
    /// pool. On failure, an error is returned.
    ///
    /// # Errors
    ///
    /// This function will return an error if it fails to retrieve the connection pool, which could
    /// occur if the underlying connection to `PostgreSQL` has not been properly established.
    pub async fn get_pool(&self) -> Result<&PgPool> {
        self.pool_get_or_initialize().await
    }

    pub fn get_table_name(&self) -> &str {
        &self.table_name
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
    /// This method allows you to specify metadata configurations for vector similarity search using
    /// `MetadataConfig`. The provided configuration will be added as a new field in the
    /// builder.
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

    pub fn default_fields() -> Vec<FieldConfig> {
        vec![FieldConfig::ID, FieldConfig::Chunk]
    }
}

#[cfg(test)]
mod tests {
    use crate::pgvector::fixtures::{PgVectorTestData, TestContext};
    use futures_util::TryStreamExt;
    use std::collections::HashSet;
    use swiftide_core::{
        document::Document,
        indexing::{self, EmbedMode, EmbeddedField},
        querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
        Persist, Retrieve,
    };
    use test_case::test_case;

    #[test_log::test(tokio::test)]
    async fn test_metadata_filter_with_vector_search() {
        let test_context = TestContext::setup_with_cfg(
            vec!["category", "priority"].into(),
            HashSet::from([EmbeddedField::Combined]),
        )
        .await
        .expect("Test setup failed");

        // Create nodes with different metadata and vectors
        let nodes = vec![
            indexing::Node::new("content1")
                .with_vectors([(EmbeddedField::Combined, vec![1.0; 384])])
                .with_metadata(vec![("category", "A"), ("priority", "1")]),
            indexing::Node::new("content2")
                .with_vectors([(EmbeddedField::Combined, vec![1.1; 384])])
                .with_metadata(vec![("category", "A"), ("priority", "2")]),
            indexing::Node::new("content3")
                .with_vectors([(EmbeddedField::Combined, vec![1.2; 384])])
                .with_metadata(vec![("category", "B"), ("priority", "1")]),
        ]
        .into_iter()
        .map(|node| node.to_owned())
        .collect();

        // Store all nodes
        test_context
            .pgv_storage
            .batch_store(nodes)
            .await
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        // Test combined metadata and vector search
        let mut query = Query::<states::Pending>::new("test_query");
        query.embedding = Some(vec![1.0; 384]);

        let search_strategy =
            SimilaritySingleEmbedding::from_filter("category = \"A\"".to_string());

        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 2);

        let contents = result
            .documents()
            .iter()
            .map(Document::content)
            .collect::<Vec<_>>();
        assert!(contents.contains(&"content1"));
        assert!(contents.contains(&"content2"));

        // Additional test with priority filter
        let search_strategy =
            SimilaritySingleEmbedding::from_filter("priority = \"1\"".to_string());
        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query)
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 2);
        let contents = result
            .documents()
            .iter()
            .map(Document::content)
            .collect::<Vec<_>>();
        assert!(contents.contains(&"content1"));
        assert!(contents.contains(&"content3"));
    }

    #[test_log::test(tokio::test)]
    async fn test_vector_similarity_search_accuracy() {
        let test_context = TestContext::setup_with_cfg(
            vec!["category", "priority"].into(),
            HashSet::from([EmbeddedField::Combined]),
        )
        .await
        .expect("Test setup failed");

        // Create nodes with known vector relationships
        let base_vector = vec![1.0; 384];
        let similar_vector = base_vector.iter().map(|x| x + 0.1).collect::<Vec<_>>();
        let dissimilar_vector = vec![-1.0; 384];

        let nodes = vec![
            indexing::Node::new("base_content")
                .with_vectors([(EmbeddedField::Combined, base_vector)])
                .with_metadata(vec![("category", "A"), ("priority", "1")]),
            indexing::Node::new("similar_content")
                .with_vectors([(EmbeddedField::Combined, similar_vector)])
                .with_metadata(vec![("category", "A"), ("priority", "2")]),
            indexing::Node::new("dissimilar_content")
                .with_vectors([(EmbeddedField::Combined, dissimilar_vector)])
                .with_metadata(vec![("category", "B"), ("priority", "1")]),
        ]
        .into_iter()
        .map(|node| node.to_owned())
        .collect();

        // Store all nodes
        test_context
            .pgv_storage
            .batch_store(nodes)
            .await
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        // Search with base vector
        let mut query = Query::<states::Pending>::new("test_query");
        query.embedding = Some(vec![1.0; 384]);

        let mut search_strategy = SimilaritySingleEmbedding::<()>::default();
        search_strategy.with_top_k(2);

        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query)
            .await
            .unwrap();

        // Verify that similar vectors are retrieved first
        assert_eq!(result.documents().len(), 2);
        let contents = result
            .documents()
            .iter()
            .map(Document::content)
            .collect::<Vec<_>>();
        assert!(contents.contains(&"base_content"));
        assert!(contents.contains(&"similar_content"));
    }

    #[test_case(
        // SingleWithMetadata - No Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_no_meta_1",
                metadata: None,
                vectors: vec![PgVectorTestData::create_test_vector(EmbeddedField::Combined, 1.0)],
                expected_in_results: true,
            },
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_no_meta_2",
                metadata: None,
                vectors: vec![PgVectorTestData::create_test_vector(EmbeddedField::Combined, 1.1)],
                expected_in_results: true,
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
                expected_in_results: true,
            },
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_with_meta_2",
                metadata: Some(vec![
                    ("category", "B"),
                    ("priority", "low")
                ].into()),
                vectors: vec![PgVectorTestData::create_test_vector(EmbeddedField::Combined, 1.3)],
                expected_in_results: true,
            }
        ],
        HashSet::from([EmbeddedField::Combined])
        ; "SingleWithMetadata mode with metadata")]
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

        // Verify storage and retrieval for each test case
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

            // 3. Test vector similarity search
            for (field, vector) in &test_case.vectors {
                let mut query = Query::<states::Pending>::new("test_query");
                query.embedding = Some(vector.clone());

                let mut search_strategy = SimilaritySingleEmbedding::<()>::default();
                search_strategy.with_top_k(nodes.len() as u64);

                let result = test_context
                    .pgv_storage
                    .retrieve(&search_strategy, query)
                    .await
                    .expect("Retrieval should succeed");

                if test_case.expected_in_results {
                    assert!(
                        result
                            .documents()
                            .iter()
                            .map(Document::content)
                            .collect::<Vec<_>>()
                            .contains(&test_case.chunk),
                        "Document should be found in results for field {field}",
                    );
                }
            }
        }
    }
}
