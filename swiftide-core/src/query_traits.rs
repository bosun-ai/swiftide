use anyhow::Result;
use async_trait::async_trait;

use crate::{
    query::{
        states::{self, Retrieved},
        Query,
    },
    querying::QueryEvaluation,
};

#[cfg(feature = "test-utils")]
#[doc(hidden)]
use mockall::{automock, predicate::str};

/// Can transform queries before retrieval
#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
pub trait TransformQuery: Send + Sync {
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>>;
}

#[async_trait]
impl<F> TransformQuery for F
where
    F: Fn(Query<states::Pending>) -> Result<Query<states::Pending>> + Send + Sync,
{
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        (self)(query)
    }
}

#[async_trait]
impl TransformQuery for Box<dyn TransformQuery> {
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        self.as_ref().transform_query(query).await
    }
}

/// A search strategy for the query pipeline
pub trait SearchStrategy: Clone + Send + Sync + Default {}

/// Can retrieve documents given a SearchStrategy
#[async_trait]
pub trait Retrieve<S: SearchStrategy>: Send + Sync {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>>;
}

#[async_trait]
impl<S: SearchStrategy> Retrieve<S> for Box<dyn Retrieve<S>> {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        self.as_ref().retrieve(search_strategy, query).await
    }
}

#[async_trait]
impl<S, F> Retrieve<S> for F
where
    S: SearchStrategy,
    F: Fn(&S, Query<states::Pending>) -> Result<Query<states::Retrieved>> + Send + Sync,
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
#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
pub trait TransformResponse: Send + Sync {
    async fn transform_response(&self, query: Query<Retrieved>)
        -> Result<Query<states::Retrieved>>;
}

#[async_trait]
impl<F> TransformResponse for F
where
    F: Fn(Query<Retrieved>) -> Result<Query<Retrieved>> + Send + Sync,
{
    async fn transform_response(&self, query: Query<Retrieved>) -> Result<Query<Retrieved>> {
        (self)(query)
    }
}

#[async_trait]
impl TransformResponse for Box<dyn TransformResponse> {
    async fn transform_response(&self, query: Query<Retrieved>) -> Result<Query<Retrieved>> {
        self.as_ref().transform_response(query).await
    }
}

/// Can answer the original query
#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
pub trait Answer: Send + Sync {
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>>;
}

#[async_trait]
impl<F> Answer for F
where
    F: Fn(Query<Retrieved>) -> Result<Query<states::Answered>> + Send + Sync,
{
    async fn answer(&self, query: Query<Retrieved>) -> Result<Query<states::Answered>> {
        (self)(query)
    }
}

#[async_trait]
impl Answer for Box<dyn Answer> {
    async fn answer(&self, query: Query<Retrieved>) -> Result<Query<states::Answered>> {
        self.as_ref().answer(query).await
    }
}

/// Evaluates a query
///
/// An evaluator needs to be able to respond to each step in the query pipeline
#[cfg_attr(feature = "test-utils", automock)]
#[async_trait]
pub trait EvaluateQuery: Send + Sync {
    async fn evaluate(&self, evaluation: QueryEvaluation) -> Result<()>;
}

#[async_trait]
impl EvaluateQuery for Box<dyn EvaluateQuery> {
    async fn evaluate(&self, evaluation: QueryEvaluation) -> Result<()> {
        self.as_ref().evaluate(evaluation).await
    }
}
