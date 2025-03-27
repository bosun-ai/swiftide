//! Test fixtures and utilities for pgvector integration testing.
//!
//! Provides test infrastructure and helper types to verify vector storage and retrieval:
//! - Mock data generation for different embedding modes
//! - Test containers for `PostgreSQL` with pgvector extension
//! - Common test scenarios and assertions
//!
//! # Examples
//!
//! ```rust
//! use swiftide_integrations::pgvector::fixtures::{TestContext, PgVectorTestData};
//! use swiftide_core::indexing::{EmbedMode, EmbeddedField};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize test context with PostgreSQL container
//! let context = TestContext::setup_with_cfg(
//!     Some(vec!["category", "priority"]),
//!     vec![EmbeddedField::Combined].into_iter().collect()
//! ).await?;
//!
//! // Create test data for different embedding modes
//! let test_data = PgVectorTestData {
//!     embed_mode: EmbedMode::SingleWithMetadata,
//!     chunk: "test content",
//!     metadata: None,
//!     vectors: vec![PgVectorTestData::create_test_vector(
//!         EmbeddedField::Combined,
//!         1.0
//!     )],
//! };
//! # Ok(())
//! # }
//! ```
//!
//! The module supports testing for:
//! - Single embedding with/without metadata
//! - Per-field embeddings
//! - Combined embedding modes
//! - Different vector configurations
//! - Various metadata scenarios
use crate::pgvector::PgVector;
use std::collections::HashSet;
use swiftide_core::{
    indexing::{self, EmbeddedField},
    Persist,
};
use testcontainers::{ContainerAsync, GenericImage};

/// Test data structure for pgvector integration testing.
///
/// Provides a flexible structure to test different embedding modes and configurations,
/// including metadata handling and vector generation.
///
/// # Examples
///
/// ```rust
/// use swiftide_integrations::pgvector::fixtures::PgVectorTestData;
/// use swiftide_core::indexing::{EmbedMode, EmbeddedField};
///
/// let test_data = PgVectorTestData {
///     embed_mode: EmbedMode::SingleWithMetadata,
///     chunk: "test content",
///     metadata: None,
///     vectors: vec![PgVectorTestData::create_test_vector(
///         EmbeddedField::Combined,
///         1.0
///     )],
/// };
/// ```
#[derive(Clone)]
pub(crate) struct PgVectorTestData<'a> {
    /// Embedding mode for the test case
    pub embed_mode: indexing::EmbedMode,
    /// Test content chunk
    pub chunk: &'a str,
    /// Optional metadata for testing metadata handling
    pub metadata: Option<indexing::Metadata>,
    /// Vector embeddings with their corresponding fields
    pub vectors: Vec<(indexing::EmbeddedField, Vec<f32>)>,
    pub expected_in_results: bool,
}

impl PgVectorTestData<'_> {
    pub(crate) fn to_node(&self) -> indexing::Node {
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

    pub(crate) fn create_test_vector(
        field: EmbeddedField,
        base_value: f32,
    ) -> (EmbeddedField, Vec<f32>) {
        (field, vec![base_value; 384])
    }
}

/// Test context managing `PostgreSQL` container and pgvector storage.
///
/// Handles the lifecycle of test containers and provides configured storage
/// instances for testing.
///
/// # Examples
///
/// ```rust
/// # use swiftide_integrations::pgvector::fixtures::TestContext;
/// # use swiftide_core::indexing::EmbeddedField;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Setup test context with specific configuration
/// let context = TestContext::setup_with_cfg(
///     Some(vec!["category"]),
///     vec![EmbeddedField::Combined].into_iter().collect()
/// ).await?;
///
/// // Use context for testing
/// context.pgv_storage.setup().await?;
/// # Ok(())
/// # }
/// ```
pub(crate) struct TestContext {
    /// Configured pgvector storage instance
    pub(crate) pgv_storage: PgVector,
    /// Container instance running `PostgreSQL` with pgvector
    _pgv_db_container: ContainerAsync<GenericImage>,
}

impl TestContext {
    /// Set up the test context, initializing `PostgreSQL` and `PgVector` storage
    /// with configurable metadata fields
    pub(crate) async fn setup_with_cfg(
        metadata_fields: Option<Vec<&str>>,
        vector_fields: HashSet<EmbeddedField>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Start `PostgreSQL` container and obtain the connection URL
        let (pgv_db_container, pgv_db_url) = swiftide_test_utils::start_postgres().await;
        tracing::info!("Postgres database URL: {:#?}", pgv_db_url);

        // Initialize the connection pool outside of the builder chain
        let mut connection_pool = PgVector::builder();

        // Configure PgVector storage
        let mut builder = connection_pool
            .db_url(pgv_db_url)
            .vector_size(384)
            .table_name("swiftide_pgvector_test".to_string());

        // Add all vector fields
        for vector_field in vector_fields {
            builder = builder.with_vector(vector_field);
        }

        // Add all metadata fields
        if let Some(metadata_fields_inner) = metadata_fields {
            for field in metadata_fields_inner {
                builder = builder.with_metadata(field);
            }
        }

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
