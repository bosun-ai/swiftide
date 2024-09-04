use qdrant_client::qdrant::{self, PrefetchQueryBuilder, SearchPointsBuilder};
use swiftide_core::{
    prelude::{Result, *},
    querying::{
        search_strategies::{HybridSearch, SimilaritySingleEmbedding},
        states, Query,
    },
    Retrieve,
};

use super::Qdrant;

#[async_trait]
impl Retrieve<SimilaritySingleEmbedding> for Qdrant {
    #[tracing::instrument]
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let Some(embedding) = &query.embedding else {
            anyhow::bail!("No embedding for query")
        };
        let points = SearchPointsBuilder::new(
            &self.collection_name,
            embedding.to_owned(),
            search_strategy.top_k(),
        )
        .with_payload(true)
        .build();

        let result = self
            .client
            .search_points(points)
            .await
            .context("Failed to retrieve from qdrant")?
            .result;

        let documents = result
            .into_iter()
            .map(|scored_point| {
                Ok(scored_point
                    .payload
                    .get("content")
                    .context("Expected document in qdrant payload")?
                    .to_string())
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(query.retrieved_documents(documents))
    }
}

#[async_trait]
impl Retrieve<HybridSearch> for Qdrant {
    #[tracing::instrument]
    async fn retrieve(
        &self,
        search_strategy: &HybridSearch,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let Some(dense) = &query.embedding else {
            anyhow::bail!("No embedding for query")
        };

        let Some(sparse) = &query.sparse_embedding else {
            anyhow::bail!("No sparse embedding for query")
        };

        // NOTE: Potential improvement to consume the vectors instead of cloning
        let result = self
            .client
            .query(
                qdrant::QueryPointsBuilder::new(&self.collection_name)
                    .with_payload(true)
                    .add_prefetch(
                        PrefetchQueryBuilder::default()
                            .query(qdrant::Query::new_nearest(qdrant::VectorInput::new_sparse(
                                sparse.indices.clone(),
                                sparse.values.clone(),
                            )))
                            .using(search_strategy.sparse_vector_field().sparse_field_name())
                            .limit(search_strategy.top_n()),
                    )
                    .add_prefetch(
                        PrefetchQueryBuilder::default()
                            .query(qdrant::Query::new_nearest(dense.clone()))
                            .using(search_strategy.dense_vector_field().field_name())
                            .limit(search_strategy.top_n()),
                    )
                    .query(qdrant::Query::new_fusion(qdrant::Fusion::Rrf))
                    .limit(search_strategy.top_k()),
            )
            .await?
            .result;

        let documents = result
            .into_iter()
            .map(|scored_point| {
                Ok(scored_point
                    .payload
                    .get("content")
                    .context("Expected document in qdrant payload")?
                    .to_string())
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(query.retrieved_documents(documents))
    }
}
