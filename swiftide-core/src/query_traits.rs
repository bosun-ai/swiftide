use anyhow::Result;
use async_trait::async_trait;

use crate::query::{
    states::{self, Retrieved},
    Query,
};

#[async_trait]
pub trait TransformQuery: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_query(
        &self,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Pending>>;
}

pub trait SearchStrategy: Clone + Send + Sync {}

#[async_trait]
pub trait Retrieve<S: SearchStrategy + ?Sized>: Send + Sync + std::fmt::Debug + ToOwned {
    async fn retrieve(
        &self,
        search_strategy: &S,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>>;
}

#[async_trait]
pub trait TransformResponse: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_response(&self, query: Query<Retrieved>)
        -> Result<Query<states::Retrieved>>;
}

// If we do roleup, answer could also take all queries in the stream instead
#[async_trait]
pub trait Answer: Send + Sync + std::fmt::Debug + ToOwned {
    async fn answer(&self, query: Query<states::Retrieved>) -> Result<Query<states::Answered>>;
}
