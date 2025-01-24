use qdrant_client::qdrant::{self, PrefetchQueryBuilder, ScoredPoint, SearchPointsBuilder};
use swiftide_core::{
    document::Document,
    indexing::{EmbeddedField, Metadata},
    prelude::{Result, *},
    querying::{
        search_strategies::{HybridSearch, SimilaritySingleEmbedding},
        states, Query,
    },
    Retrieve,
};

use super::Qdrant;

/// Implement the `Retrieve` trait for `SimilaritySingleEmbedding` search strategy.
///
/// Can be used in the query pipeline to retrieve documents from Qdrant.
///
/// Supports filters via the `qdrant_client::qdrant::Filter` type.
#[async_trait]
impl Retrieve<SimilaritySingleEmbedding<qdrant::Filter>> for Qdrant {
    #[tracing::instrument]
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding<qdrant::Filter>,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let Some(embedding) = &query.embedding else {
            anyhow::bail!("No embedding for query")
        };
        let mut query_builder = SearchPointsBuilder::new(
            &self.collection_name,
            embedding.to_owned(),
            search_strategy.top_k(),
        )
        .with_payload(true);

        if let Some(filter) = &search_strategy.filter() {
            query_builder = query_builder.filter(filter.to_owned());
        }

        if self.vectors.len() > 1 || !self.sparse_vectors.is_empty() {
            // TODO: Make this configurable
            // It will break if there are multiple vectors and no combined vector
            query_builder = query_builder.vector_name(EmbeddedField::Combined.field_name());
        }

        let result = self
            .client
            .search_points(query_builder.build())
            .await
            .context("Failed to retrieve from qdrant")?
            .result;

        let documents = result
            .into_iter()
            .map(scored_point_into_document)
            .collect::<Result<Vec<_>>>()?;

        Ok(query.retrieved_documents(documents))
    }
}

/// Ensures that the `SimilaritySingleEmbedding` search strategy can be used when no filter is set.
#[async_trait]
impl Retrieve<SimilaritySingleEmbedding> for Qdrant {
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        Retrieve::<SimilaritySingleEmbedding<qdrant::Filter>>::retrieve(
            self,
            &search_strategy.into_concrete_filter::<qdrant::Filter>(),
            query,
        )
        .await
    }
}

/// Implement the `Retrieve` trait for `HybridSearch` search strategy.
///
/// Can be used in the query pipeline to retrieve documents from Qdrant.
///
/// Expects both a dense and sparse embedding to be set on the query.
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
            .map(scored_point_into_document)
            .collect::<Result<Vec<_>>>()?;

        Ok(query.retrieved_documents(documents))
    }
}

fn scored_point_into_document(scored_point: ScoredPoint) -> Result<Document> {
    let content = scored_point
        .payload
        .get("content")
        .context("Expected document in qdrant payload")?
        .to_string();

    let metadata: Metadata = scored_point
        .payload
        .into_iter()
        .filter(|(k, _)| *k != "content")
        .collect::<Vec<(_, _)>>()
        .into();

    Ok(Document::new(content, Some(metadata)))
}

#[cfg(test)]
mod tests {
    use itertools::Itertools as _;
    use swiftide_core::{
        indexing::{self, EmbeddedField},
        Persist as _,
    };

    use super::*;

    async fn setup() -> (
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
        Qdrant,
    ) {
        let (guard, qdrant_url) = swiftide_test_utils::start_qdrant().await;

        let qdrant_client = Qdrant::try_from_url(qdrant_url)
            .unwrap()
            .vector_size(384)
            .with_vector(EmbeddedField::Combined)
            .with_sparse_vector(EmbeddedField::Combined)
            .build()
            .unwrap();

        qdrant_client.setup().await.unwrap();

        let nodes = vec![
            indexing::Node::new("test_query1").with_metadata(("filter", "true")),
            indexing::Node::new("test_query2").with_metadata(("filter", "true")),
            indexing::Node::new("test_query3").with_metadata(("filter", "false")),
        ]
        .into_iter()
        .map(|node| {
            node.with_vectors([(EmbeddedField::Combined, vec![1.0; 384])]);
            node.with_sparse_vectors([(
                EmbeddedField::Combined,
                swiftide_core::SparseEmbedding {
                    indices: vec![0, 1],
                    values: vec![1.0, 1.0],
                },
            )]);
            node.to_owned()
        })
        .collect();

        qdrant_client
            .batch_store(nodes)
            .await
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        (guard, qdrant_client)
    }

    #[test_log::test(tokio::test)]
    async fn test_retrieve_multiple_docs_and_filter() {
        let (_guard, qdrant_client) = setup().await;

        let mut query = Query::<states::Pending>::new("test_query");
        query.embedding = Some(vec![1.0; 384]);

        let search_strategy = SimilaritySingleEmbedding::<()>::default();
        let result = qdrant_client
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 3);
        assert_eq!(
            result
                .documents()
                .iter()
                .sorted()
                .map(Document::content)
                .collect_vec(),
            // FIXME: The extra quotes should be removed by serde (via qdrant::Value), but they are
            // not
            ["\"test_query1\"", "\"test_query2\"", "\"test_query3\""]
                .into_iter()
                .sorted()
                .collect_vec()
        );

        let search_strategy = SimilaritySingleEmbedding::from_filter(qdrant::Filter::must([
            qdrant::Condition::matches("filter", "true".to_string()),
        ]));
        let result = qdrant_client
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 2);
        assert_eq!(
            result
                .documents()
                .iter()
                .sorted()
                .map(Document::content)
                .collect_vec(),
            ["\"test_query1\"", "\"test_query2\""]
                .into_iter()
                .sorted()
                .collect_vec()
        );

        let search_strategy = SimilaritySingleEmbedding::from_filter(qdrant::Filter::must([
            qdrant::Condition::matches("filter", "banana".to_string()),
        ]));
        let result = qdrant_client
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 0);
    }

    #[tokio::test]
    async fn test_hybrid_search() {
        let (_guard, qdrant_client) = setup().await;
        let mut query = Query::<states::Pending>::new("test_query");

        query.embedding = Some(vec![1.0; 384]);
        query.sparse_embedding = Some(swiftide_core::SparseEmbedding {
            indices: vec![0, 1],
            values: vec![1.0, 1.0],
        });
        let search_strategy = HybridSearch::default();
        let result = qdrant_client
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 3);
    }
}
