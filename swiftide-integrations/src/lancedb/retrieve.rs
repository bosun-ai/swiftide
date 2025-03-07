use anyhow::Result;
use arrow_array::{RecordBatch, StringArray};
use async_trait::async_trait;
use futures_util::TryStreamExt;
use itertools::Itertools;
use lancedb::query::{ExecutableQuery, QueryBase};
use swiftide_core::{
    document::Document,
    indexing::Metadata,
    querying::{
        search_strategies::{CustomStrategy, SimilaritySingleEmbedding},
        states, Query,
    },
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

        let batches = query_builder
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let documents = Self::retrieve_from_record_batches(batches.as_slice());

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

#[async_trait]
impl<Q: ExecutableQuery + Send + Sync + 'static> Retrieve<CustomStrategy<Q>> for LanceDB {
    /// Implements vector similarity search for LanceDB using a custom query strategy.
    ///
    /// # Type Parameters
    /// * `VectorQuery` - LanceDB's query type for vector similarity search
    async fn retrieve(
        &self,
        search_strategy: &CustomStrategy<Q>,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        // Build the custom query using both strategy and query state
        let query_builder = search_strategy.build_query(&query).await?;

        // Execute the query using the builder's built-in methods
        let batches = query_builder
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;

        let documents = Self::retrieve_from_record_batches(batches.as_slice());

        Ok(query.retrieved_documents(documents))
    }
}

impl LanceDB {
    /// Retrieves documents from Arrow `RecordBatches` by processing each row and extracting content
    /// and metadata fields.
    ///
    /// The function expects a "chunk" field to contain the main document content, while all other
    /// string fields are treated as metadata. Non-string fields are currently skipped    
    fn retrieve_from_record_batches(batches: &[RecordBatch]) -> Vec<Document> {
        let total_rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        let mut documents = Vec::with_capacity(total_rows);

        let process_batch = |batch: &RecordBatch, documents: &mut Vec<Document>| {
            for row_idx in 0..batch.num_rows() {
                let schema = batch.schema();

                let (content, metadata): (String, Option<Metadata>) = {
                    let mut metadata = Metadata::default();
                    let mut content = String::new();

                    for (col_idx, field) in schema.as_ref().fields().iter().enumerate() {
                        if let Some(array) =
                            batch.column(col_idx).as_any().downcast_ref::<StringArray>()
                        {
                            let value = array.value(row_idx).to_string();

                            if field.name() == "chunk" {
                                content = value;
                            } else {
                                metadata.insert(field.name().to_string(), value);
                            }
                        } else {
                            // Handle other array types as necessary
                            // TODO: Can't we just downcast to serde::Value or fail?
                        }
                    }

                    (
                        content,
                        if metadata.is_empty() {
                            None
                        } else {
                            Some(metadata)
                        },
                    )
                };

                documents.push(Document::new(content, metadata));
            }
        };

        batches
            .iter()
            .for_each(|batch| process_batch(batch, &mut documents));

        documents
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
