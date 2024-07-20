use anyhow::Result;
use async_trait::async_trait;

use crate::query::{
    states::{QueryTransformed, ResponseTransformed, Retrieved},
    Query, RetrievableQuery, TransformableQuery, TransformableResponse,
};

#[async_trait]
pub trait TransformQuery: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_query(
        &self,
        query: Query<impl TransformableQuery>,
    ) -> Result<Query<QueryTransformed>>;
}

pub trait SearchStrategyMarker {}

#[async_trait]
pub trait Retrieve<S: SearchStrategyMarker>: Send + Sync + std::fmt::Debug + ToOwned {
    async fn retrieve(&self, query: Query<impl RetrievableQuery>) -> Result<Query<Retrieved>>;
}

#[async_trait]
pub trait TransformResponse: Send + Sync + std::fmt::Debug + ToOwned {
    async fn transform_response(
        &self,
        query: Query<impl TransformableResponse>,
    ) -> Result<Query<ResponseTransformed>>;
}
