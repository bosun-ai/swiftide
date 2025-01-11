//! Generic vector search strategy framework for customizable query generation.
//!
//! Provides core abstractions for vector similarity search:
//! - Generic query type parameter for retriever-specific implementations
//! - Flexible query generation through closure-based configuration
//!
//! This module implements a strategy pattern for vector similarity search,
//! allowing different retrieval backends to provide their own query generation
//! logic while maintaining a consistent interface. The framework emphasizes
//! composition over inheritance, enabling configuration through closures
//! rather than struct fields.

use crate::querying::{self, states, Query};
use anyhow::{anyhow, Result};
use std::marker::PhantomData;
use std::sync::Arc;

/// A type alias for query generation functions.
///
/// The query generator takes a pending query state and produces a
/// retriever-specific query type. All configuration parameters should
/// be captured in the closure's environment.
type QueryGenerator<Q> = Arc<dyn Fn(&Query<states::Pending>) -> Result<Q> + Send + Sync>;

/// `CustomStrategy` provides a flexible way to generate retriever-specific search queries.
///
/// This struct implements a strategy pattern for vector similarity search, allowing
/// different retrieval backends to provide their own query generation logic. Configuration
/// is managed through the query generation closure, promoting a more flexible and
/// composable design.
///
/// # Type Parameters
/// * `Q` - The retriever-specific query type (e.g., `sqlx::QueryBuilder` for `PostgreSQL`)
///
/// # Examples
/// ```ignore
/// // Define search configuration
/// const MAX_SEARCH_RESULTS: i64 = 5;
///
/// // Create a custom search strategy
/// let strategy = CustomStrategy::from_query(|query_node| {
///     let mut builder = QueryBuilder::new();
///     
///     // Configure search parameters within the closure
///     builder.push(" LIMIT ");
///     builder.push_bind(MAX_SEARCH_RESULTS);
///     
///     Ok(builder)
/// });
/// ```
///
/// # Implementation Notes
/// - Search configuration (like result limits and vector fields) should be defined
///   in the closure's scope
/// - Implementers are responsible for validating configuration values
/// - The query generator has access to the full query state for maximum flexibility
pub struct CustomStrategy<Q> {
    /// The query generation function now returns a `Q`
    query: Option<QueryGenerator<Q>>,

    /// `PhantomData` to handle the generic parameter
    _marker: PhantomData<Q>,
}

impl<Q: Send + Sync + 'static> querying::SearchStrategy for CustomStrategy<Q> {}

impl<Q> Default for CustomStrategy<Q> {
    fn default() -> Self {
        Self {
            query: None,
            _marker: PhantomData,
        }
    }
}

// Manual Clone implementation instead of derive
impl<Q> Clone for CustomStrategy<Q> {
    fn clone(&self) -> Self {
        Self {
            query: self.query.clone(), // Arc clone is fine
            _marker: PhantomData,
        }
    }
}

impl<Q: Send + Sync + 'static> CustomStrategy<Q> {
    /// Creates a new `CustomStrategy` with a query generation function.
    ///
    /// The provided closure should contain all necessary configuration for
    /// query generation. This design allows for more flexible configuration
    /// management compared to struct-level fields.
    ///
    /// # Parameters
    /// * `query` - A closure that generates retriever-specific queries
    pub fn from_query(
        query: impl Fn(&Query<states::Pending>) -> Result<Q> + Send + Sync + 'static,
    ) -> Self {
        Self {
            query: Some(Arc::new(query)),
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
            Some(query_fn) => Ok(query_fn(query_node)?),
            None => Err(anyhow!(
                "No query function has been set. Use from_query() to set a query function."
            )),
        }
    }
}
