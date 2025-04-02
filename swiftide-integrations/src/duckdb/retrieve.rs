use anyhow::{Context as _, Result};
use async_trait::async_trait;
use swiftide_core::{
    querying::{
        search_strategies::{CustomStrategy, SimilaritySingleEmbedding},
        states, Document, Query,
    },
    Retrieve,
};

use super::Duckdb;

#[async_trait]
impl Retrieve<SimilaritySingleEmbedding> for Duckdb {
    async fn retrieve(
        &self,
        search_strategy: &SimilaritySingleEmbedding,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let Some(embedding) = query.embedding.as_ref() else {
            return Err(anyhow::Error::msg("Missing embedding in query state"));
        };

        let table_name = &self.table_name;

        // Silently ignores multiple vector fields
        let (field_name, embedding_size) = self
            .vectors
            .iter()
            .next()
            .context("No vectors configured")?;

        let limit = search_strategy.top_k();

        // Ideally it should be a prepared statement, where only the new parameters lead to extra
        // allocations. This is possible in 1.2.1, but that version is still broken for VSS via
        // Rust.
        let sql = format!(
            "SELECT uuid, chunk, path FROM {table_name}\n
            ORDER BY array_distance({field_name}, ARRAY[{}]::FLOAT[{embedding_size}])\n
            LIMIT {limit}",
            embedding
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );

        tracing::trace!("[duckdb] Executing query: {}", sql);

        let conn = self.connection().lock().unwrap();

        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare duckdb statement for persist")?;

        tracing::trace!("[duckdb] Retrieving documents");

        let documents = stmt
            .query_map([], |row| {
                Ok(Document::builder()
                    .metadata([("id", row.get::<_, String>(0)?), ("path", row.get(2)?)])
                    .content(row.get::<_, String>(1)?)
                    .build()
                    .expect("Failed to build document; should never happen"))
            })
            .context("failed to query for documents")?
            .collect::<Result<Vec<Document>, _>>()
            .context("failed to build documents")?;

        tracing::debug!("[duckdb] Retrieved documents");
        Ok(query.retrieved_documents(documents))
    }
}

#[async_trait]
impl Retrieve<CustomStrategy<String>> for Duckdb {
    async fn retrieve(
        &self,
        search_strategy: &CustomStrategy<String>,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let sql = search_strategy
            .build_query(&query)
            .await
            .context("Failed to build query")?;

        tracing::debug!("[duckdb] Executing query: {}", sql);

        let conn = self.connection().lock().unwrap();
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare duckdb statement for persist")?;

        tracing::debug!("[duckdb] Prepared statement");

        let documents = stmt
            .query_map([], |row| {
                Ok(Document::builder()
                    .metadata([("id", row.get::<_, String>(0)?), ("path", row.get(2)?)])
                    .content(row.get::<_, String>(1)?)
                    .build()
                    .expect("Failed to build document; should never happen"))
            })
            .context("failed to query for documents")?
            .collect::<Result<Vec<Document>, _>>()
            .context("failed to build documents")?;

        tracing::debug!("[duckdb] Retrieved documents");

        Ok(query.retrieved_documents(documents))
    }
}

#[cfg(test)]
mod tests {
    use indexing::{EmbeddedField, Node};
    use swiftide_core::{indexing, Persist as _};

    use super::*;

    #[test_log::test(tokio::test)]
    async fn test_duckdb_retrieving_documents() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let node = Node::new("Hello duckdb!")
            .with_vectors([(EmbeddedField::Combined, vec![1.0, 2.0, 3.0])])
            .to_owned();

        client.setup().await.unwrap();
        client.store(node.clone()).await.unwrap();

        tracing::info!("Stored node");

        let query = Query::<states::Pending>::builder()
            .embedding(vec![1.0, 2.0, 3.0])
            .original("Some query")
            .build()
            .unwrap();

        let result = client
            .retrieve(&SimilaritySingleEmbedding::default(), query)
            .await
            .unwrap();

        assert_eq!(result.documents().len(), 1);
        let document = result.documents().first().unwrap();

        assert_eq!(document.content(), "Hello duckdb!");
        assert_eq!(
            document.metadata().get("id").unwrap().as_str(),
            Some(node.id().to_string().as_str())
        );
    }
}
