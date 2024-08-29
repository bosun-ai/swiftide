use anyhow::Result;
use async_trait::async_trait;

use crate::{
    query::{
        states::{self, Retrieved},
        Query,
    },
    querying::QueryEvaluation,
};

/// Can transform queries before retrieval
#[async_trait]
pub trait TransformQuery: Send + Sync + ToOwned {
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>>;
}

#[async_trait]
impl<F> TransformQuery for F
where
    F: Fn(Query<states::Pending>) -> Result<Query<states::Pending>> + Send + Sync + ToOwned,
{
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        (self)(query)
    }
}

/// A search strategy for the query pipeline
pub trait SearchStrategy: Clone + Send + Sync + Default {}

/// Can retrieve documents given a SearchStrategy
#[async_trait]
pub trait Retrieve<S: SearchStrategy + ?Sized>: Send + Sync + ToOwned {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>>;
}

#[async_trait]
impl<S, F> Retrieve<S> for F
where
    S: SearchStrategy + ?Sized,
    F: Fn(&S, Query<states::Pending>) -> Result<Query<states::Retrieved>> + Send + Sync + ToOwned,
{
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        (self)(search_strategy, query)
    }
}

/// Can transform a response after retrieval
#[async_trait]
pub trait TransformResponse: Send + Sync + ToOwned {
    async fn transform_response(&self, query: Query<Retrieved>)
        -> Result<Query<states::Retrieved>>;
}

#[async_trait]
impl<F> TransformResponse for F
where
    F: Fn(Query<Retrieved>) -> Result<Query<Retrieved>> + Send + Sync + ToOwned,
{
    async fn transform_response(&self, query: Query<Retrieved>) -> Result<Query<Retrieved>> {
        (self)(query)
    }
}

/// Can answer the original query
#[async_trait]
pub trait Answer: Send + Sync + ToOwned {
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>>;
}

#[async_trait]
impl<F> Answer for F
where
    F: Fn(Query<Retrieved>) -> Result<Query<states::Answered>> + Send + Sync + ToOwned,
{
    async fn answer(&self, query: Query<Retrieved>) -> Result<Query<states::Answered>> {
        (self)(query)
    }
}

/// Evaluates a query
///
/// An evaluator needs to be able to respond to each step in the query pipeline
#[async_trait]
pub trait EvaluateQuery: Send + Sync {
    async fn evaluate(&self, evaluation: QueryEvaluation) -> Result<()>;
}
