use anyhow::{Context as _, Result};
use arrow_array::StringArray;
use async_trait::async_trait;
use futures_util::TryStreamExt;
use itertools::Itertools;
use lancedb::query::{ExecutableQuery, QueryBase};
use swiftide_core::{
    querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
    Retrieve,
};

use super::{FieldConfig, LanceDB};

/// Implement the `Retrieve` trait for `SimilaritySingleEmbedding` search strategy.
///
/// Can be used in the query pipeline to retrieve documents from LanceDB.
///
/// Supports filters as strings. Refer to the LanceDB documentation for the format.
#[async_trait]
impl Retrieve<SimilaritySingleEmbedding<String>> for LanceDB {
    #[tracing::instrument]
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding<String>,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let Some(embedding) = &query.embedding else {
            anyhow::bail!("No embedding for query")
        };

        let table = self
            .get_connection()
            .await?
            .open_table(&self.table_name)
            .execute()
            .await?;

        let vector_fields = self
            .fields
            .iter()
            .filter(|field| matches!(field, FieldConfig::Vector(_)))
            .collect_vec();

        if vector_fields.is_empty() || vector_fields.len() > 1 {
            anyhow::bail!("Zero or multiple vector fields configured in schema")
        }

        let column_name = vector_fields.first().map(|v| v.field_name()).unwrap();

        let mut query_builder = table
            .query()
            .nearest_to(embedding.as_slice())?
            .column(&column_name)
            .limit(usize::try_from(search_strategy.top_k())?);

        if let Some(filter) = &search_strategy.filter() {
            query_builder = query_builder.only_if(filter);
        }

        let result = query_builder
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let Some(recordbatch) = result.first() else {
            return Ok(query.retrieved_documents(vec![]));
        };

        let documents: Vec<String> = recordbatch
            .column_by_name("chunk")
            .and_then(|raw_array| raw_array.as_any().downcast_ref::<StringArray>())
            .context("Could not cast documents to strings")?
            .into_iter()
            .flatten()
            .map_into()
            .collect();

        Ok(query.retrieved_documents(documents))
    }
}

#[async_trait]
impl Retrieve<SimilaritySingleEmbedding> for LanceDB {
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        Retrieve::<SimilaritySingleEmbedding<String>>::retrieve(
            self,
            &search_strategy.into_concrete_filter::<String>(),
            query,
        )
        .await
    }
}

#[cfg(test)]
mod test {
    use swiftide_core::{
        indexing::{self, EmbeddedField},
        Persist as _,
    };
    use temp_dir::TempDir;

    use super::*;

    async fn setup() -> (TempDir, LanceDB) {
        let tempdir = TempDir::new().unwrap();
        let lancedb = LanceDB::builder()
            .uri(tempdir.child("lancedb").to_str().unwrap())
            .vector_size(384)
            .with_metadata("filter")
            .with_vector(EmbeddedField::Combined)
            .table_name("swiftide_test")
            .build()
            .unwrap();
        lancedb.setup().await.unwrap();

        (tempdir, lancedb)
    }

    #[tokio::test]
    async fn test_retrieve_multiple_docs_and_filter() {
        let (_guard, lancedb) = setup().await;

        let nodes = vec![
            indexing::Node::new("test_query1").with_metadata(("filter", "true")),
            indexing::Node::new("test_query2").with_metadata(("filter", "true")),
            indexing::Node::new("test_query3").with_metadata(("filter", "false")),
        ]
        .into_iter()
        .map(|node| {
            node.with_vectors([(EmbeddedField::Combined, vec![1.0; 384])]);
            node.to_owned()
        })
        .collect();

        lancedb
            .batch_store(nodes)
            .await
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        let mut query = Query::<states::Pending>::new("test_query");
        query.embedding = Some(vec![1.0; 384]);

        let search_strategy =
            SimilaritySingleEmbedding::from_filter("filter = \"true\"".to_string());
        let result = lancedb
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 2);

        let search_strategy =
            SimilaritySingleEmbedding::from_filter("filter = \"banana\"".to_string());
        let result = lancedb
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 0);

        let search_strategy = SimilaritySingleEmbedding::<()>::default();
        let result = lancedb
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 3);
    }
}
