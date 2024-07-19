use anyhow::Result;
use async_trait::async_trait;

use crate::query::Query;

#[async_trait]
pub trait TransformQuery: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_query(&self, query: Query) -> Result<Query>;
}

pub trait SearchStrategy {}

#[async_trait]
pub trait Retrieve: Send + Sync + std::fmt::Debug + ToOwned {
    async fn retrieve(&self, query: Query) -> Result<Query>;
}

#[async_trait]
pub trait TransformResponse: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_response(&self, query: Query) -> Result<Query>;
}
