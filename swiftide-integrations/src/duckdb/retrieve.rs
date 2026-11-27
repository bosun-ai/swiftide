use anyhow::{Context as _, Result};
use async_trait::async_trait;
use swiftide_core::{
    Retrieve,
    indexing::Chunk,
    querying::{
        Document, Query,
        search_strategies::{CustomStrategy, HybridSearch, SimilaritySingleEmbedding},
        states,
    },
};

use super::Duckdb;

#[async_trait]
impl<T: Chunk> Retrieve<SimilaritySingleEmbedding> for Duckdb<T> {
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
impl<T: Chunk> Retrieve<CustomStrategy<String>> for Duckdb<T> {
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

#[async_trait]
impl<T: Chunk> Retrieve<HybridSearch> for Duckdb<T> {
    async fn retrieve(
        &self,
        search_strategy: &HybridSearch,
        query: Query<states::Pending>,
    ) -> Result<Query<states::Retrieved>> {
        let Some(embedding) = query.embedding.as_ref() else {
            return Err(anyhow::Error::msg("Missing embedding in query state"));
        };

        let sql = self
            .hybrid_query_sql(search_strategy, query.current(), embedding)
            .context("Failed to build query")?;

        tracing::debug!("[duckdb] Executing query: {}", sql);

        let conn = self.connection().lock().unwrap();
        self.refresh_fts_index(&conn)?;
        let mut stmt = conn
            .prepare(&sql)
            .context("Failed to prepare duckdb statement for persist")?;

        tracing::debug!("[duckdb] Prepared statement");

        let documents = stmt
            // DuckDB has issues with using `params!` :(
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
    use indexing::{EmbeddedField, TextNode};
    use swiftide_core::{Persist as _, indexing};

    use super::*;

    #[test_log::test(tokio::test)]
    async fn test_duckdb_retrieving_documents() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let node = TextNode::new("Hello duckdb!")
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

    #[test_log::test(tokio::test)]
    async fn test_duckdb_retrieving_documents_hybrid() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("test".to_string())
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();

        let node = TextNode::new("Hello duckdb!")
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
            .retrieve(&HybridSearch::default(), query)
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

    fn hybrid_query(text: &str) -> Query<states::Pending> {
        Query::<states::Pending>::builder()
            .original(text)
            .embedding(vec![1.0, 0.0, 0.0])
            .build()
            .unwrap()
    }

    fn hybrid_node(text: &str, vector: Vec<f32>) -> TextNode {
        TextNode::new(text)
            .with_vectors([(EmbeddedField::Combined, vector)])
            .to_owned()
    }

    #[tokio::test]
    async fn hybrid_candidates_are_ranked_before_limiting() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("ranked")
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();
        client.setup().await.unwrap();
        client
            .store(hybrid_node("unrelated first row", vec![-1.0, 0.0, 0.0]))
            .await
            .unwrap();
        client
            .store(hybrid_node("relevant later row", vec![1.0, 0.0, 0.0]))
            .await
            .unwrap();
        let strategy = HybridSearch::default().with_top_n(1).with_top_k(1);
        let result = client
            .retrieve(&strategy, hybrid_query("absent"))
            .await
            .unwrap();
        assert_eq!(result.documents()[0].content(), "relevant later row");
    }

    #[tokio::test]
    async fn hybrid_search_without_text_matches_keeps_finite_vector_scores() {
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("vectors")
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();
        client.setup().await.unwrap();
        client
            .store(hybrid_node("opposite direction", vec![-1.0, 0.0, 0.0]))
            .await
            .unwrap();
        let strategy = HybridSearch::default();
        for add_match in [false, true] {
            if add_match {
                client
                    .store(hybrid_node("matching direction", vec![1.0, 0.0, 0.0]))
                    .await
                    .unwrap();
            }
            let result = client
                .retrieve(&strategy, hybrid_query("absent"))
                .await
                .unwrap();
            assert_eq!(
                result.documents()[0].content(),
                if add_match {
                    "matching direction"
                } else {
                    "opposite direction"
                }
            );
            let sql = client
                .hybrid_query_sql(&strategy, "absent", &[1.0, 0.0, 0.0])
                .unwrap();
            let conn = client.connection.lock().unwrap();
            let mut statement = conn.prepare(&sql).unwrap();
            let scores: Vec<f64> = statement
                .query_map([], |row| row.get("score_cc"))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert!(scores.iter().all(|score| score.is_finite()));
        }
    }

    #[tokio::test]
    async fn hybrid_text_search_includes_single_and_batch_writes_through_clones() {
        use futures_util::TryStreamExt as _;
        let client = Duckdb::builder()
            .connection(duckdb::Connection::open_in_memory().unwrap())
            .table_name("fresh")
            .with_vector(EmbeddedField::Combined, 3)
            .build()
            .unwrap();
        client.setup().await.unwrap();
        client
            .store(hybrid_node("initial baseline", vec![1.0, 0.0, 0.0]))
            .await
            .unwrap();
        let strategy = HybridSearch::default().with_top_n(1).with_top_k(1);
        client
            .retrieve(&strategy, hybrid_query("baseline"))
            .await
            .unwrap();
        for (text, batch) in [("telescope astronomy", false), ("microscope biology", true)] {
            let writer = client.clone();
            let node = hybrid_node(text, vec![1.0, 0.0, 0.0]);
            if batch {
                writer
                    .batch_store(vec![node])
                    .await
                    .try_collect::<Vec<_>>()
                    .await
                    .unwrap();
            } else {
                writer.store(node).await.unwrap();
            }
            let result = client
                .retrieve(&strategy, hybrid_query(text))
                .await
                .unwrap();
            assert_eq!(result.documents()[0].content(), text);
            let sql = client
                .hybrid_query_sql(&strategy, text, &[1.0, 0.0, 0.0])
                .unwrap();
            let score: f64 = client
                .connection
                .lock()
                .unwrap()
                .query_row(&sql, [], |row| row.get("raw_fts_score"))
                .unwrap();
            assert!(score > 0.0, "new text must be indexed");
        }
    }
}
