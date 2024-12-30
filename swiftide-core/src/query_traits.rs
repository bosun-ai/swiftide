use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;

use crate::{
    query::{
        states::{self, Retrieved},
        Query,
    },
    querying::QueryEvaluation,
};

#[cfg(feature = "test-utils")]
use mockall::{mock, predicate::str};

/// Can transform queries before retrieval
#[async_trait]
pub trait TransformQuery: Send + Sync + DynClone {
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>>;

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(TransformQuery);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub TransformQuery {}

    #[async_trait]
    impl TransformQuery for TransformQuery {
        async fn transform_query(
            &self,
            query: Query<states::Pending>,
        ) -> Result<Query<states::Pending>>;
        fn name(&self) -> &'static str;
    }

    impl Clone for TransformQuery {
        fn clone(&self) -> Self;
    }
}

#[async_trait]
impl<F> TransformQuery for F
where
    F: Fn(Query<states::Pending>) -> Result<Query<states::Pending>> + Send + Sync + Clone,
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

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl TransformQuery for Arc<dyn TransformQuery> {
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>> {
        self.as_ref().transform_query(query).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

/// A search strategy for the query pipeline
pub trait SearchStrategy: Clone + Send + Sync + Default {}

/// Can retrieve documents given a SearchStrategy
#[async_trait]
pub trait Retrieve<S: SearchStrategy>: Send + Sync + DynClone {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>>;

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(<S> Retrieve<S>);

#[async_trait]
impl<S: SearchStrategy> Retrieve<S> for Box<dyn Retrieve<S>> {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        self.as_ref().retrieve(search_strategy, query).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl<S: SearchStrategy> Retrieve<S> for Arc<dyn Retrieve<S>> {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        self.as_ref().retrieve(search_strategy, query).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl<S, F> Retrieve<S> for F
where
    S: SearchStrategy,
    F: Fn(&S, Query<states::Pending>) -> Result<Query<states::Retrieved>> + Send + Sync + Clone,
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
pub trait TransformResponse: Send + Sync + DynClone {
    async fn transform_response(&self, query: Query<Retrieved>)
        -> Result<Query<states::Retrieved>>;

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(TransformResponse);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub TransformResponse {}

    #[async_trait]
    impl TransformResponse for TransformResponse {
        async fn transform_response(&self, query: Query<Retrieved>)
            -> Result<Query<states::Retrieved>>;
        fn name(&self) -> &'static str;
    }

    impl Clone for TransformResponse {
        fn clone(&self) -> Self;
    }
}
#[async_trait]
impl<F> TransformResponse for F
where
    F: Fn(Query<Retrieved>) -> Result<Query<Retrieved>> + Send + Sync + Clone,
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

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl TransformResponse for Arc<dyn TransformResponse> {
    async fn transform_response(&self, query: Query<Retrieved>) -> Result<Query<Retrieved>> {
        self.as_ref().transform_response(query).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

/// Can answer the original query
#[async_trait]
pub trait Answer: Send + Sync + DynClone {
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>>;

    fn name(&self) -> &'static str {
        let name = std::any::type_name::<Self>();
        name.split("::").last().unwrap_or(name)
    }
}

dyn_clone::clone_trait_object!(Answer);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub Answer {}

    #[async_trait]
    impl Answer for Answer {
        async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>>;
        fn name(&self) -> &'static str;
    }

    impl Clone for Answer {
        fn clone(&self) -> Self;
    }
}
#[async_trait]
impl<F> Answer for F
where
    F: Fn(Query<Retrieved>) -> Result<Query<states::Answered>> + Send + Sync + Clone,
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

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

#[async_trait]
impl Answer for Arc<dyn Answer> {
    async fn answer(&self, query: Query<Retrieved>) -> Result<Query<states::Answered>> {
        self.as_ref().answer(query).await
    }

    fn name(&self) -> &'static str {
        self.as_ref().name()
    }
}

/// Evaluates a query
///
/// An evaluator needs to be able to respond to each step in the query pipeline
#[async_trait]
pub trait EvaluateQuery: Send + Sync + DynClone {
    async fn evaluate(&self, evaluation: QueryEvaluation) -> Result<()>;
}

dyn_clone::clone_trait_object!(EvaluateQuery);

#[cfg(feature = "test-utils")]
mock! {
    #[derive(Debug)]
    pub EvaluateQuery {}

    #[async_trait]
    impl EvaluateQuery for EvaluateQuery {
        async fn evaluate(&self, evaluation: QueryEvaluation) -> Result<()>;
    }

    impl Clone for EvaluateQuery {
        fn clone(&self) -> Self;
    }
}
#[async_trait]
impl EvaluateQuery for Box<dyn EvaluateQuery> {
    async fn evaluate(&self, evaluation: QueryEvaluation) -> Result<()> {
        self.as_ref().evaluate(evaluation).await
    }
}

#[async_trait]
impl EvaluateQuery for Arc<dyn EvaluateQuery> {
    async fn evaluate(&self, evaluation: QueryEvaluation) -> Result<()> {
        self.as_ref().evaluate(evaluation).await
    }
}
