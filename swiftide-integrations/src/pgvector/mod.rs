//! This module integrates with the pgvector database, providing functionalities to create and manage vector collections,
//! store data, and optimize indexing for efficient searches.
//!
//! pgvector is utilized in both the `indexing::Pipeline` and `query::Pipeline` modules.
mod persist;
mod pgv_table_types;
mod retrieve;
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

#[cfg(test)]
mod tests {
    use crate::pgvector::PgVector;
    use futures_util::TryStreamExt;
    use swiftide_core::{indexing, indexing::EmbeddedField, Persist};
    use swiftide_core::{
        indexing::EmbedMode,
        querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
        Retrieve,
    };
    use test_case::test_case;
    use testcontainers::{ContainerAsync, GenericImage};

    struct TestContext {
        pgv_storage: PgVector,
        _pgv_db_container: ContainerAsync<GenericImage>,
    }

    impl TestContext {
        /// Set up the test context, initializing `PostgreSQL` and `PgVector` storage
        /// with configurable metadata fields
        async fn setup_with_cfg(
            metadata_fields: Option<Vec<&str>>,
            embedded_field: indexing::EmbeddedField,
        ) -> Result<Self, Box<dyn std::error::Error>> {
            // Start `PostgreSQL` container and obtain the connection URL
            let (pgv_db_container, pgv_db_url) = swiftide_test_utils::start_postgres().await;
            tracing::info!("Postgres database URL: {:#?}", pgv_db_url);

            // Initialize the connection pool outside of the builder chain
            let mut connection_pool = PgVector::builder()
                .try_connect_to_pool(pgv_db_url, Some(10))
                .await
                .map_err(|err| {
                    tracing::error!("Failed to connect to Postgres server: {}", err);
                    err
                })?;

            // Configure PgVector storage
            let mut builder = connection_pool
                .vector_size(384)
                .with_vector(embedded_field)
                .table_name("swiftide_pgvector_test".to_string());

            // Add all metadata fields
            if let Some(metadata_fields_inner) = metadata_fields {
                for field in metadata_fields_inner {
                    builder = builder.with_metadata(field);
                }
            };

            let pgv_storage = builder.build().map_err(|err| {
                tracing::error!("Failed to build PgVector: {}", err);
                err
            })?;

            // Set up PgVector storage (create the table if not exists)
            pgv_storage.setup().await.map_err(|err| {
                tracing::error!("PgVector setup failed: {}", err);
                err
            })?;

            Ok(Self {
                pgv_storage,
                _pgv_db_container: pgv_db_container,
            })
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_metadata_filter_with_vector_search() {
        let test_context = TestContext::setup_with_cfg(
            vec!["category", "priority"].into(),
            EmbeddedField::Combined,
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

        // Search with category filter
        let search_strategy =
            SimilaritySingleEmbedding::from_filter("category = \"A\"".to_string());
        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 2);
        assert!(result.documents().contains(&"content1".to_string()));
        assert!(result.documents().contains(&"content2".to_string()));

        // Additional test with priority filter
        let search_strategy =
            SimilaritySingleEmbedding::from_filter("priority = \"1\"".to_string());
        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query)
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 2);
        assert!(result.documents().contains(&"content1".to_string()));
        assert!(result.documents().contains(&"content3".to_string()));
    }

    #[test_log::test(tokio::test)]
    async fn test_vector_similarity_search_accuracy() {
        let test_context = TestContext::setup_with_cfg(None, EmbeddedField::Combined)
            .await
            .expect("Test setup failed");

        // Create nodes with known vector relationships
        let base_vector = vec![1.0; 384];
        let similar_vector = base_vector.iter().map(|x| x + 0.1).collect::<Vec<_>>();
        let dissimilar_vector = vec![-1.0; 384];

        let nodes = vec![
            indexing::Node::new("base_content")
                .with_vectors([(EmbeddedField::Combined, base_vector)]),
            indexing::Node::new("similar_content")
                .with_vectors([(EmbeddedField::Combined, similar_vector)]),
            indexing::Node::new("dissimilar_content")
                .with_vectors([(EmbeddedField::Combined, dissimilar_vector)]),
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
        assert!(result.documents().contains(&"base_content".to_string()));
        assert!(result.documents().contains(&"similar_content".to_string()));
    }

    #[derive(Clone)]
    struct PgVectorTestData<'a> {
        pub embed_mode: indexing::EmbedMode,
        pub chunk: &'a str,
        pub metadata: Option<indexing::Metadata>,
        pub vectors: Vec<(indexing::EmbeddedField, Vec<f32>)>,
        pub expected_in_results: bool,
    }

    impl<'a> PgVectorTestData<'a> {
        fn to_node(&self) -> indexing::Node {
            // Create the initial builder
            let mut base_builder = indexing::Node::builder();

            // Set the required fields
            let mut builder = base_builder.chunk(self.chunk).embed_mode(self.embed_mode);

            // Add metadata if it exists
            if let Some(metadata) = &self.metadata {
                builder = builder.metadata(metadata.clone());
            }

            // Build the node and add vectors
            let mut node = builder.build().unwrap();
            node.vectors = Some(self.vectors.clone().into_iter().collect());
            node
        }
    }

    fn create_test_vector(field: EmbeddedField, base_value: f32) -> (EmbeddedField, Vec<f32>) {
        (field, vec![base_value; 384])
    }

    #[test_case(
        // SingleWithMetadata - No Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_no_meta_1",
                metadata: None,
                vectors: vec![create_test_vector(EmbeddedField::Combined, 1.0)],
                expected_in_results: true,
            },
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_no_meta_2",
                metadata: None,
                vectors: vec![create_test_vector(EmbeddedField::Combined, 1.1)],
                expected_in_results: true,
            }
        ]
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
                vectors: vec![create_test_vector(EmbeddedField::Combined, 1.2)],
                expected_in_results: true,
            },
            PgVectorTestData {
                embed_mode: EmbedMode::SingleWithMetadata,
                chunk: "single_with_meta_2",
                metadata: Some(vec![
                    ("category", "B"),
                    ("priority", "low")
                ].into()),
                vectors: vec![create_test_vector(EmbeddedField::Combined, 1.3)],
                expected_in_results: true,
            }
        ]
        ; "SingleWithMetadata mode with metadata")]
    #[test_case(
        // Both - No Metadata
        vec![
            PgVectorTestData {
                embed_mode: EmbedMode::Both,
                chunk: "both_no_meta_1",
                metadata: None,
                vectors: vec![
                    create_test_vector(EmbeddedField::Combined, 3.0),
                    create_test_vector(EmbeddedField::Chunk, 3.1)
                ],
                expected_in_results: true,
            },
            PgVectorTestData {
                embed_mode: EmbedMode::Both,
                chunk: "both_no_meta_2",
                metadata: None,
                vectors: vec![
                    create_test_vector(EmbeddedField::Combined, 3.2),
                    create_test_vector(EmbeddedField::Chunk, 3.3)
                ],
                expected_in_results: true,
            }
        ]
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
                    create_test_vector(EmbeddedField::Combined, 3.4),
                    create_test_vector(EmbeddedField::Chunk, 3.5),
                    create_test_vector(EmbeddedField::Metadata("category".into()), 3.6),
                    create_test_vector(EmbeddedField::Metadata("priority".into()), 3.7),
                    create_test_vector(EmbeddedField::Metadata("tag".into()), 3.8)
                ],
                expected_in_results: true,
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
                    create_test_vector(EmbeddedField::Combined, 3.9),
                    create_test_vector(EmbeddedField::Chunk, 4.0),
                    create_test_vector(EmbeddedField::Metadata("category".into()), 4.1),
                    create_test_vector(EmbeddedField::Metadata("priority".into()), 4.2),
                    create_test_vector(EmbeddedField::Metadata("tag".into()), 4.3)
                ],
                expected_in_results: true,
            }
        ]
        ; "Both mode with metadata")]
    #[test_log::test(tokio::test)]
    async fn test_persist_and_retrieve_nodes(test_cases: Vec<PgVectorTestData<'_>>) {
        // Extract all possible metadata fields from test cases
        let metadata_fields: Vec<&str> = test_cases
            .iter()
            .filter_map(|case| case.metadata.as_ref())
            .flat_map(|metadata| metadata.iter().map(|(key, _)| key.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Initialize test context with all required metadata fields
        let test_context =
            TestContext::setup_with_cfg(Some(metadata_fields), EmbeddedField::Combined)
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
                    .retrieve(&search_strategy, query.clone())
                    .await
                    .expect("Retrieval should succeed");

                if test_case.expected_in_results {
                    assert!(
                        result.documents().contains(&test_case.chunk.to_string()),
                        "Document should be found in results for field {field}",
                    );
                }
            }

            // 4. Test metadata filtering if present
            if let Some(metadata) = &test_case.metadata {
                for (key, value) in metadata {
                    let filter_query = format!("{key} = \"{value}\"");
                    let search_strategy = SimilaritySingleEmbedding::from_filter(filter_query);

                    let mut query = Query::<states::Pending>::new("test_query");
                    query.embedding = Some(test_case.vectors[0].1.clone());

                    let result = test_context
                        .pgv_storage
                        .retrieve(&search_strategy, query)
                        .await
                        .expect("Filtered retrieval should succeed");

                    if test_case.expected_in_results {
                        assert!(
                            result.documents().contains(&test_case.chunk.to_string()),
                            "Document should be found when filtering by metadata {key}={value}"
                        );
                    }
                }
            }
        }
    }
}
