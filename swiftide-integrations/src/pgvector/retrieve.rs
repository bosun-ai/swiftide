use crate::pgvector::{FieldConfig, PgVector, PgVectorBuilder};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use pgvector::Vector;
use sqlx::{prelude::FromRow, types::Uuid, Column, Row};
use std::fmt::Write as _;
use swiftide_core::{
    document::Document,
    indexing::Metadata,
    querying::{
        search_strategies::{CustomStrategy, SimilaritySingleEmbedding},
        states, Query,
    },
    Retrieve,
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct VectorSearchResult {
    id: Uuid,
    chunk: String,
    metadata: Metadata,
}

impl From<VectorSearchResult> for Document {
    fn from(val: VectorSearchResult) -> Self {
        Document::new(val.chunk, Some(val.metadata))
    }
}

impl FromRow<'_, sqlx::postgres::PgRow> for VectorSearchResult {
    fn from_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let mut metadata = Metadata::default();

        // Metadata fields are stored each as prefixed meta_ fields. Perhaps we should add a single
        // metadata field instead of multiple fields.
        for column in row.columns() {
            if column.name().starts_with("meta_") {
                row.try_get::<serde_json::Value, _>(column.name())?
                    .as_object()
                    .and_then(|object| {
                        object.keys().collect::<Vec<_>>().first().map(|key| {
                            metadata.insert(
                                key.to_owned(),
                                object.get(key.as_str()).expect("infallible").clone(),
                            );
                        })
                    });
            }
        }

        Ok(VectorSearchResult {
            id: row.try_get("id")?,
            chunk: row.try_get("chunk")?,
            metadata,
        })
    }
}

#[allow(clippy::redundant_closure_for_method_calls)]
#[async_trait]
impl Retrieve<SimilaritySingleEmbedding<String>> for PgVector {
    #[tracing::instrument]
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding<String>,
        query_state: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let embedding = if let Some(embedding) = query_state.embedding.as_ref() {
            Vector::from(embedding.clone())
        } else {
            return Err(anyhow::Error::msg("Missing embedding in query state"));
        };

        let vector_column_name = self.get_vector_column_name()?;

        let pool = self.pool_get_or_initialize().await?;

        let default_columns: Vec<_> = PgVectorBuilder::default_fields()
            .iter()
            .map(|f| f.field_name().to_string())
            .chain(
                self.fields
                    .iter()
                    .filter(|f| matches!(f, FieldConfig::Metadata(_)))
                    .map(|f| f.field_name().to_string()),
            )
            .collect();

        // Start building the SQL query
        let mut sql = format!(
            "SELECT {} FROM {}",
            default_columns.join(", "),
            self.table_name
        );

        if let Some(filter) = search_strategy.filter() {
            let filter_parts: Vec<&str> = filter.split('=').collect();
            if filter_parts.len() == 2 {
                let key = filter_parts[0].trim();
                let value = filter_parts[1].trim().trim_matches('"');
                tracing::debug!(
                    "Filter being applied: key = {:#?}, value = {:#?}",
                    key,
                    value
                );

                let sql_filter = format!(
                    " WHERE meta_{}->>'{}' = '{}'",
                    PgVector::normalize_field_name(key),
                    key,
                    value
                );
                sql.push_str(&sql_filter);
            } else {
                return Err(anyhow!("Invalid filter format"));
            }
        }

        // Add the ORDER BY clause for vector similarity search
        write!(sql, " ORDER BY {vector_column_name} <=> $1 LIMIT $2")?;

        tracing::debug!("Running retrieve with SQL: {}", sql);

        let top_k = i32::try_from(search_strategy.top_k())
            .map_err(|_| anyhow!("Failed to convert top_k to i32"))?;

        let data: Vec<VectorSearchResult> = sqlx::query_as(&sql)
            .bind(embedding)
            .bind(top_k)
            .fetch_all(pool)
            .await?;

        let docs = data.into_iter().map(Into::into).collect();

        Ok(query_state.retrieved_documents(docs))
    }
}

#[async_trait]
impl Retrieve<SimilaritySingleEmbedding> for PgVector {
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
impl Retrieve<CustomStrategy<sqlx::QueryBuilder<'static, sqlx::Postgres>>> for PgVector {
    async fn retrieve(
        &self,
        search_strategy: &CustomStrategy<sqlx::QueryBuilder<'static, sqlx::Postgres>>,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        // Get the database pool
        let pool = self.get_pool().await?;

        // Build the custom query using both strategy and query state
        let mut query_builder = search_strategy.build_query(&query).await?;

        // Execute the query using the builder's built-in methods
        let results = query_builder
            .build_query_as::<VectorSearchResult>() // Convert to a typed query
            .fetch_all(pool) // Execute and get all results
            .await
            .map_err(|e| anyhow!("Failed to execute search query: {}", e))?;

        // Transform results into documents
        let documents = results.into_iter().map(Into::into).collect();

        // Update query state with retrieved documents
        Ok(query.retrieved_documents(documents))
    }
}

#[cfg(test)]
mod tests {
    use crate::pgvector::fixtures::TestContext;
    use futures_util::TryStreamExt;
    use std::collections::HashSet;
    use swiftide_core::{indexing, indexing::EmbeddedField, Persist};
    use swiftide_core::{
        querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
        Retrieve,
    };

    #[test_log::test(tokio::test)]
    async fn test_retrieve_multiple_docs_and_filter() {
        let test_context = TestContext::setup_with_cfg(
            vec!["filter"].into(),
            HashSet::from([EmbeddedField::Combined]),
        )
        .await
        .expect("Test setup failed");

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

        test_context
            .pgv_storage
            .batch_store(nodes)
            .await
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        let mut query = Query::<states::Pending>::new("test_query");
        query.embedding = Some(vec![1.0; 384]);

        let search_strategy = SimilaritySingleEmbedding::<()>::default();
        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 3);

        let search_strategy =
            SimilaritySingleEmbedding::from_filter("filter = \"true\"".to_string());

        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 2);

        let search_strategy =
            SimilaritySingleEmbedding::from_filter("filter = \"banana\"".to_string());

        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();
        assert_eq!(result.documents().len(), 0);
    }

    #[test_log::test(tokio::test)]
    async fn test_retrieve_docs_with_metadata() {
        let test_context = TestContext::setup_with_cfg(
            vec!["other", "text"].into(),
            HashSet::from([EmbeddedField::Combined]),
        )
        .await
        .expect("Test setup failed");

        let nodes = vec![indexing::Node::new("test_query1")
            .with_metadata([
                ("other", serde_json::Value::from(10)),
                ("text", serde_json::Value::from("some text")),
            ])
            .with_vectors([(EmbeddedField::Combined, vec![1.0; 384])])
            .to_owned()];

        test_context
            .pgv_storage
            .batch_store(nodes)
            .await
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        let mut query = Query::<states::Pending>::new("test_query");
        query.embedding = Some(vec![1.0; 384]);

        let search_strategy = SimilaritySingleEmbedding::<()>::default();
        let result = test_context
            .pgv_storage
            .retrieve(&search_strategy, query.clone())
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 1);

        let doc = result.documents().first().unwrap();
        assert_eq!(
            doc.metadata().get("other"),
            Some(&serde_json::Value::from(10))
        );
        assert_eq!(
            doc.metadata().get("text"),
            Some(&serde_json::Value::from("some text"))
        );
    }
}
