//! Generic vector search strategy framework for customizable query generation.
//!
//! Provides core abstractions for vector similarity search:
//! - Generic query type parameter for storage-specific implementations
//! - Configurable vector field selection and result limits
//!
//! This module serves as the foundation for implementing custom vector
//! search strategies across different storage backends, ensuring type
//! safety and consistent behavior while allowing maximum flexibility
//! in query generation.

use crate::{
    indexing::EmbeddedField,
    querying::{self, states, Query},
};
use anyhow::{anyhow, Result};
use std::marker::PhantomData;
use std::sync::Arc;

/// A type alias to simplify the query generation function type
type QueryGenerator<Q, T> = Arc<dyn Fn(&T, &Query<states::Pending>) -> Result<Q> + Send + Sync>;

/// `CustomQuery` provides a flexible way to generate provider-specific search queries.
///
/// # Type Parameters
/// * `Q` - The provider-specific query type (e.g., `sqlx::QueryBuilder` for `PostgreSQL`)
///
/// # Examples
/// ```
/// let strategy = CustomQuery::from_query(|strategy, query_node| {
///     // Query construction logic
///     Ok(provider_specific_query)
/// });
/// ```
pub struct CustomStrategy<Q> {
    /// The query generation function now returns a `Q`
    query: Option<QueryGenerator<Q, Self>>,
    /// Maximum number of results to return
    top_k: u64,
    /// Field to use for vector similarity search
    vector_field: EmbeddedField,
    /// `PhantomData` to handle the generic parameter
    _marker: PhantomData<Q>,
}

impl<Q: Send + Sync + 'static> querying::SearchStrategy for CustomStrategy<Q> {}

impl<Q> Default for CustomStrategy<Q> {
    fn default() -> Self {
        Self {
            query: None,
            top_k: super::DEFAULT_TOP_K,
            vector_field: EmbeddedField::Combined,
            _marker: PhantomData,
        }
    }
}

// Manual Clone implementation instead of derive
impl<Q> Clone for CustomStrategy<Q> {
    fn clone(&self) -> Self {
        Self {
            query: self.query.clone(), // Arc clone is fine
            top_k: self.top_k,
            vector_field: self.vector_field.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Q: Send + Sync + 'static> CustomStrategy<Q> {
    /// Creates a new `CustomQuery` with a query generation function
    pub fn from_query(
        query: impl Fn(&Self, &Query<states::Pending>) -> Result<Q> + Send + Sync + 'static,
    ) -> Self {
        Self {
            query: Some(Arc::new(query)),
            top_k: super::DEFAULT_TOP_K,
            vector_field: EmbeddedField::Combined,
            _marker: PhantomData,
        }
    }

    /// Gets the query builder, which can then be used to build the actual query
    ///
    /// # Errors
    /// This function will return an error if:
    /// - No query function has been set (use `from_query` to set a query function).
    /// - The query function fails while processing the provided `query_node`.
    pub fn build_query(&self, query_node: &Query<states::Pending>) -> Result<Q> {
        match &self.query {
            Some(query_fn) => Ok(query_fn(self, query_node)?),
            None => Err(anyhow!(
                "No query function has been set. Use from_query() to set a query function."
            )),
        }
    }

    /// Sets the maximum number of results to return
    ///
    /// # Panics
    /// This function will panic if:
    /// - `top_k` is greater than the maximum value for a Postgres `bigint` (i.e., `i64::MAX`).
    /// - `top_k` is not positive (i.e., `top_k <= 0`).    
    #[must_use]
    pub fn with_top_k(mut self, top_k: u64) -> Self {
        // Ensure top_k is within Postgres bigint bounds
        assert!(
            i64::try_from(top_k).is_ok(),
            "{}",
            format!(
                "top_k value {top_k} exceeds maximum allowed value {:#?}",
                i64::MAX
            )
        );
        assert!(top_k > 0, "top_k must be positive, got {top_k}");

        self.top_k = top_k;
        self
    }

    /// Sets the vector field to use for similarity search
    #[must_use]
    pub fn with_vector_field(mut self, vector_field: impl Into<EmbeddedField>) -> Self {
        self.vector_field = vector_field.into();
        self
    }

    // Accessor methods
    pub fn top_k(&self) -> u64 {
        self.top_k
    }

    pub fn vector_field(&self) -> &EmbeddedField {
        &self.vector_field
    }
}
