use anyhow::Result;
use async_trait::async_trait;

use crate::query::{
    states::{self, Retrieved},
    Query,
};

/// Can transform queries before retrieval
#[async_trait]
pub trait TransformQuery: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>>;
}

pub trait SearchStrategy: Clone + Send + Sync + Default {}

/// Can retrieve documents given a SearchStrategy
#[async_trait]
pub trait Retrieve<S: SearchStrategy + ?Sized>: Send + Sync + std::fmt::Debug + ToOwned {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>>;
}

/// Can transform a response after retrieval
#[async_trait]
pub trait TransformResponse: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_response(&self, query: Query<Retrieved>)
        -> Result<Query<states::Retrieved>>;
}

/// Can answer the original query
#[async_trait]
pub trait Answer: Send + Sync + std::fmt::Debug + ToOwned {
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>>;
}
