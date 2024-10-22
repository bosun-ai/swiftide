use crate::pgvector::{PgVector, PgVectorBuilder};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use pgvector::Vector;
use sqlx::{prelude::FromRow, types::Uuid};
use swiftide_core::{
    querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
    Retrieve,
};

#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
struct VectorSearchResult {
    id: Uuid,
    chunk: String,
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
        let embedding = query_state
            .embedding
            .as_ref()
            .ok_or_else(|| anyhow!("No embedding for query"))?;
        let embedding = Vector::from(embedding.clone());

        // let pool = self.connection_pool.get_pool().await?;
        let pool = self.connection_pool.get_pool()?;

        let default_columns: Vec<_> = PgVectorBuilder::default_fields()
            .iter()
            .map(|f| f.field_name().to_string())
            .collect();
        let vector_column_name = self.get_vector_column_name()?;

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
        sql.push_str(&format!(
            " ORDER BY {} <=> $1 LIMIT $2",
            &vector_column_name
        ));

        tracing::debug!("Running retrieve with SQL: {}", sql);

        let top_k = i32::try_from(search_strategy.top_k())
            .map_err(|_| anyhow!("Failed to convert top_k to i32"))?;

        let data: Vec<VectorSearchResult> = sqlx::query_as(&sql)
            .bind(embedding)
            .bind(top_k)
            .fetch_all(&pool)
            .await?;

        let docs = data.into_iter().map(|r| r.chunk).collect();

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

#[cfg(test)]
mod tests {
    use crate::pgvector::PgVector;
    use futures_util::TryStreamExt;
    use swiftide_core::{indexing, indexing::EmbeddedField, Persist};
    use swiftide_core::{
        querying::{search_strategies::SimilaritySingleEmbedding, states, Query},
        Retrieve,
    };
    use temp_dir::TempDir;
    use testcontainers::{ContainerAsync, GenericImage};

    struct TestContext {
        pgv_storage: PgVector,
        _temp_dir: TempDir,
        _pgv_db_container: ContainerAsync<GenericImage>,
    }

    impl TestContext {
        /// Set up the test context, initializing `PostgreSQL` and `PgVector` storage
        async fn setup() -> Result<Self, Box<dyn std::error::Error>> {
            // Start PostgreSQL container and obtain the connection URL
            let (pgv_db_container, pgv_db_url, temp_dir) =
                swiftide_test_utils::start_postgres().await;

            tracing::info!("Postgres database URL: {:#?}", pgv_db_url);

            // Configure and build PgVector storage
            let pgv_storage = PgVector::builder()
                .try_connect_to_pool(pgv_db_url, Some(10))
                .await
                .map_err(|err| {
                    tracing::error!("Failed to connect to Postgres server: {}", err);
                    err
                })?
                .vector_size(384)
                .with_vector(EmbeddedField::Combined)
                .with_metadata("filter")
                .table_name("swiftide_pgvector_test".to_string())
                .build()
                .map_err(|err| {
                    tracing::error!("Failed to build PgVector: {}", err);
                    err
                })?;

            // Set up PgVector storage (create the table if not exists)
            pgv_storage.setup().await.map_err(|err| {
                tracing::error!("PgVector setup failed: {}", err);
                err
            })?;

            Ok(Self {
                pgv_storage,
                _temp_dir: temp_dir,
                _pgv_db_container: pgv_db_container,
            })
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_retrieve_multiple_docs_and_filter() {
        let test_context = TestContext::setup().await.expect("Test setup failed");

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
}
