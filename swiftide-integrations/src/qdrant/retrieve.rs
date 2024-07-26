use qdrant_client::qdrant::SearchPointsBuilder;
use swiftide_core::{
    prelude::{Result, *},
    querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
    Retrieve,
};

use super::Qdrant;

#[async_trait]
impl Retrieve<SimilaritySingleEmbedding> for Qdrant {
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

        let result = self.client.search_points(points).await?.result;

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
