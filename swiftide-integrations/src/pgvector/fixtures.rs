//! This module implements common types and helper utilities for unit tests related to the pgvector
use crate::pgvector::PgVector;
use std::collections::HashSet;
use swiftide_core::{
    indexing::{self, EmbeddedField},
    Persist,
};
use testcontainers::{ContainerAsync, GenericImage};

#[derive(Clone)]
pub(crate) struct PgVectorTestData<'a> {
    pub embed_mode: indexing::EmbedMode,
    pub chunk: &'a str,
    pub metadata: Option<indexing::Metadata>,
    pub vectors: Vec<(indexing::EmbeddedField, Vec<f32>)>,
}

impl<'a> PgVectorTestData<'a> {
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

pub(crate) struct TestContext {
    pub(crate) pgv_storage: PgVector,
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
