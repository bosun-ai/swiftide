use super::DEFAULT_TOP_K;
use crate::{indexing::EmbeddedField, querying};
// use anyhow::{anyhow, Result};
// use async_trait::async_trait;
use derive_builder::Builder;
use std::sync::Arc;

/// Type definition for a function that generates SQL queries
pub type QueryGenerator = dyn Fn(&str, &[&str], &str) -> String + Send + Sync;

/// A flexible search strategy that allows users to provide custom SQL queries
/// for vector similarity search in `PostgreSQL` with pgvector extension.
#[derive(Clone, Builder)]
pub struct DynamicVectorSearch {
    /// Maximum number of results to return
    #[builder(default)]
    top_k: u64,

    /// Field to use for vector similarity search
    #[builder(default)]
    vector_field: EmbeddedField,

    /// User-provided function that generates the complete SQL query string.
    /// The function receives:
    /// - `table_name`: The name of the table to query
    /// - columns: Vector of column names to select
    /// - `vector_field`: Name of the vector field for similarity search
    #[builder(setter(custom))]
    query_generator: Arc<QueryGenerator>,
}

impl querying::SearchStrategy for DynamicVectorSearch {}

impl Default for DynamicVectorSearch {
    fn default() -> Self {
        Self {
            top_k: DEFAULT_TOP_K,
            vector_field: EmbeddedField::Combined,
            query_generator: Arc::new(|table, columns, vector_field| {
                // Provides a sensible default query implementation
                format!(
                    "SELECT {} FROM {} ORDER BY {} <=> $1::vector LIMIT $2",
                    columns.join(", "),
                    table,
                    vector_field
                )
            }),
        }
    }
}

impl DynamicVectorSearch {
    /// Creates a new `DynamicVectorSearch` with a user-provided query generator
    pub fn new<F>(query_generator: F) -> Self
    where
        F: Fn(&str, &[&str], &str) -> String + Send + Sync + 'static,
    {
        Self {
            top_k: DEFAULT_TOP_K,
            vector_field: EmbeddedField::Combined,
            query_generator: Arc::new(query_generator),
        }
    }

    /// Provides builder-style configuration for `top_k`
    #[must_use]
    pub fn with_top_k(mut self, top_k: u64) -> Self {
        self.top_k = top_k;
        self
    }

    /// Provides builder-style configuration for vector field
    #[must_use]
    pub fn with_vector_field(mut self, vector_field: impl Into<EmbeddedField>) -> Self {
        self.vector_field = vector_field.into();
        self
    }

    // Accessor methods remain unchanged
    pub fn top_k(&self) -> u64 {
        self.top_k
    }
    pub fn vector_field(&self) -> &EmbeddedField {
        &self.vector_field
    }

    /// Generates the SQL query using the table configuration
    pub fn generate_query(&self, table_name: &str, columns: &[&str], vector_field: &str) -> String {
        (self.query_generator)(table_name, columns, vector_field)
    }
}
