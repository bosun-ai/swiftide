//! Implements a flexible vector search strategy framework using closure-based configuration.
//! Supports both synchronous and asynchronous query generation for different retrieval backends.

use crate::querying::{self, states, Query};
use anyhow::{anyhow, Result};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

// TODO: Should be possible to remove the static bounds and allow Q as borrowed with some fu

// Function type for generating retriever-specific queries
type QueryGenerator<Q> = Arc<dyn Fn(&Query<states::Pending>) -> Result<Q> + Send + Sync>;

// Function type for async query generation
type AsyncQueryGenerator<Q> = Arc<
    dyn Fn(&Query<states::Pending>) -> Pin<Box<dyn Future<Output = Result<Q>> + Send>>
        + Send
        + Sync,
>;

/// Implements the strategy pattern for vector similarity search, allowing retrieval backends
/// to define custom query generation logic through closures.
pub struct CustomStrategy<Q> {
    query: Option<QueryGenerator<Q>>,
    async_query: Option<AsyncQueryGenerator<Q>>,
    _marker: PhantomData<Q>,
}

impl<Q: Send + Sync> querying::SearchStrategy for CustomStrategy<Q> {}

impl<Q> Default for CustomStrategy<Q> {
    fn default() -> Self {
        Self {
            query: None,
            async_query: None,
            _marker: PhantomData,
        }
    }
}

impl<Q> Clone for CustomStrategy<Q> {
    fn clone(&self) -> Self {
        Self {
            query: self.query.clone(),
            async_query: self.async_query.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Q: Send + Sync> CustomStrategy<Q> {
    /// Creates a new strategy with a synchronous query generator.
    pub fn from_query(
        query: impl Fn(&Query<states::Pending>) -> Result<Q> + Send + Sync + 'static,
    ) -> Self {
        Self {
            query: Some(Arc::new(query)),
            async_query: None,
            _marker: PhantomData,
        }
    }

    /// Creates a new strategy with an asynchronous query generator.
    pub fn from_async_query<F>(
        query: impl Fn(&Query<states::Pending>) -> F + Send + Sync + 'static,
    ) -> Self
    where
        F: Future<Output = Result<Q>> + Send + 'static,
    {
        Self {
            query: None,
            async_query: Some(Arc::new(move |q| Box::pin(query(q)))),
            _marker: PhantomData,
        }
    }

    /// Generates a query using either the sync or async generator.
    /// Returns error if no query generator is set.
    ///
    /// # Errors
    /// Returns an error if:
    /// * No query generator has been configured
    /// * The configured query generator fails during query generation
    pub async fn build_query(&self, query_node: &Query<states::Pending>) -> Result<Q> {
        match (&self.query, &self.async_query) {
            (Some(query_fn), _) => query_fn(query_node),
            (_, Some(async_fn)) => async_fn(query_node).await,
            _ => Err(anyhow!("No query function has been set.")),
        }
    }
}
