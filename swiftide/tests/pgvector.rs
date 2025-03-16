//! This module contains tests for the `PgVector` indexing pipeline in the Swiftide project.
//! The tests validate the functionality of the pipeline, ensuring that data is correctly indexed
//! and processed from temporary files, database configurations, and simulated environments.

use swiftide_core::document::Document;
use swiftide_integrations::treesitter::metadata_qa_code;
use temp_dir::TempDir;

use anyhow::{anyhow, Result};
use sqlx::{prelude::FromRow, types::Uuid};
use swiftide::{
    indexing::{
        self, loaders,
        transformers::{
            self, metadata_qa_code::NAME as METADATA_QA_CODE_NAME, ChunkCode, MetadataQACode,
        },
        EmbeddedField, Pipeline,
    },
    integrations::{
        self,
        pgvector::{FieldConfig, PgVector, PgVectorBuilder, VectorConfig},
    },
    query::{self, answers, query_transformers, response_transformers, states, Query},
};
use swiftide_test_utils::{mock_chat_completions, openai_client};
use wiremock::MockServer;

#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
struct VectorSearchResult {
    id: Uuid,
    chunk: String,
}

/// Test case for verifying the PgVector indexing pipeline functionality.
///
/// This test:
/// - Sets up a temporary file and Postgres database for testing.
/// - Configures a PgVector instance with a vector size of 384.
/// - Executes an indexing pipeline for Rust code chunks with embedded vector metadata.
/// - Performs a similarity-based vector search on the database and validates the retrieved results.
///
/// Ensures correctness of end-to-end data flow, including table management, vector storage, and
/// query execution.
#[test_log::test(tokio::test)]
async fn test_pgvector_indexing() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    let code = "fn main() { println!(\"Hello, World!\"); }";
    std::fs::write(&codefile, code).unwrap();

    let (_pgv_db_container, pgv_db_url) = swiftide_test_utils::start_postgres().await;

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;

    // Configure Pgvector with a default vector size, a single embedding
    // and in addition to embedding the text metadata, also store it in a field
    let pgv_storage = PgVector::builder()
        .db_url(pgv_db_url)
        .vector_size(384)
        .with_vector(EmbeddedField::Combined)
        .table_name("swiftide_test")
        .build()
        .unwrap();

    // Drop the existing test table before running the test
    println!("Dropping existing test table & index if it exists");
    let drop_table_sql = "DROP TABLE IF EXISTS swiftide_test";
    let drop_index_sql = "DROP INDEX IF EXISTS swiftide_test_embedding_idx";

    if let Ok(pool) = pgv_storage.get_pool().await {
        sqlx::query(drop_table_sql)
            .execute(pool)
            .await
            .expect("Failed to execute SQL query for dropping the table");
        sqlx::query(drop_index_sql)
            .execute(pool)
            .await
            .expect("Failed to execute SQL query for dropping the index");
    } else {
        panic!("Unable to acquire database connection pool");
    }

    let result =
        Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
            .then_chunk(ChunkCode::try_for_language("rust").unwrap())
            .then(|mut node: indexing::Node| {
                node.with_vectors([(EmbeddedField::Combined, vec![1.0; 384])]);
                Ok(node)
            })
            .then_store_with(pgv_storage.clone())
            .run()
            .await;

    result.expect("PgVector Named vectors test indexing pipeline failed");

    let pool = pgv_storage
        .get_pool()
        .await
        .expect("Unable to acquire database connection pool");

    // Start building the SQL query
    let sql_vector_query =
        "SELECT id, chunk FROM swiftide_test ORDER BY vector_combined <=> $1::VECTOR LIMIT $2";

    println!("Running retrieve with SQL: {sql_vector_query}");

    let top_k: i32 = 10;
    let embedding = vec![1.0; 384];

    let data: Vec<VectorSearchResult> = sqlx::query_as(sql_vector_query)
        .bind(embedding)
        .bind(top_k)
        .fetch_all(pool)
        .await
        .expect("Sql named vector query failed");

    let docs: Vec<_> = data.into_iter().map(|r| r.chunk).collect();

    println!("Retrieved documents for debugging: {docs:#?}");

    assert_eq!(docs[0], "fn main() { println!(\"Hello, World!\"); }");
}

/// Test the retrieval functionality of `PgVector` integration.
///
/// This test verifies that a Rust code snippet can be embedded,
/// stored in a PostgreSQL database using `PgVector`, and accurately
/// retrieved using a single similarity-based query pipeline. It sets up
/// a mock OpenAI client, configures `PgVector`, and executes a query
/// to ensure the pipeline retrieves the correct data and generates
/// an expected response.
#[test_log::test(tokio::test)]
async fn test_pgvector_retrieve() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    let code = "fn main() { println!(\"Hello, World!\"); }";
    std::fs::write(&codefile, code).unwrap();

    let (_pgv_db_container, pgv_db_url) = swiftide_test_utils::start_postgres().await;

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let fastembed =
        integrations::fastembed::FastEmbed::try_default().expect("Could not create FastEmbed");

    // Configure Pgvector with a default vector size, a single embedding
    // and in addition to embedding the text metadata, also store it in a field
    let pgv_storage = PgVector::builder()
        .db_url(pgv_db_url)
        .vector_size(384)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(METADATA_QA_CODE_NAME)
        .with_metadata("filter")
        .table_name("swiftide_test")
        .build()
        .unwrap();

    // Drop the existing test table before running the test
    println!("Dropping existing test table & index if it exists");
    let drop_table_sql = "DROP TABLE IF EXISTS swiftide_test";
    let drop_index_sql = "DROP INDEX IF EXISTS swiftide_test_embedding_idx";

    if let Ok(pool) = pgv_storage.get_pool().await {
        sqlx::query(drop_table_sql)
            .execute(pool)
            .await
            .expect("Failed to execute SQL query for dropping the table");
        sqlx::query(drop_index_sql)
            .execute(pool)
            .await
            .expect("Failed to execute SQL query for dropping the index");
    } else {
        panic!("Unable to acquire database connection pool");
    }

    Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(ChunkCode::try_for_language("rust").unwrap())
        .then(MetadataQACode::new(openai_client.clone()))
        .then(|mut node: indexing::Node| {
            node.metadata
                .insert("filter".to_string(), "true".to_string());
            Ok(node)
        })
        .then_in_batch(transformers::Embed::new(fastembed.clone()).with_batch_size(20))
        .log_nodes()
        .then_store_with(pgv_storage.clone())
        .run()
        .await
        .unwrap();

    let strategy = query::search_strategies::SimilaritySingleEmbedding::from_filter(
        "filter = \"true\"".to_string(),
    );

    let query_pipeline = query::Pipeline::from_search_strategy(strategy)
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(fastembed.clone()))
        .then_retrieve(pgv_storage.clone())
        .then_transform_response(response_transformers::Summary::from_client(
            openai_client.clone(),
        ))
        .then_answer(answers::Simple::from_client(openai_client.clone()));

    let result: Query<states::Answered> = query_pipeline.query("What is swiftide?").await.unwrap();

    assert_eq!(
        result.answer(),
        "\n\nHello there, how may I assist you today?"
    );

    let first_document = result.documents().first().unwrap();

    let expected = Document::builder()
        .content("fn main() { println!(\"Hello, World!\"); }")
        .metadata([
            (
                metadata_qa_code::NAME,
                "\n\nHello there, how may I assist you today?",
            ),
            ("filter", "true"),
        ])
        .build()
        .unwrap();
    assert_eq!(first_document, &expected);
}

/// Tests the dynamic vector similarity search functionality using PostgreSQL.
///
/// This integration test verifies the complete workflow of vector similarity search:
/// 1. Sets up a temporary test environment with a sample Rust code file
/// 2. Configures PostgreSQL with pgvector extension for vector operations
/// 3. Creates and populates test data using a processing pipeline:
///    - Loads source code files
///    - Chunks code into processable segments
///    - Generates metadata using OpenAI
///    - Embeds text using FastEmbed
///    - Stores processed data in PostgreSQL
/// 4. Implements a custom search strategy that:
///    - Filters results based on metadata
///    - Orders results by vector similarity
///    - Limits the number of returned results
/// 5. Executes a query pipeline that:
///    - Generates and embeds the search query
///    - Retrieves similar documents
///    - Transforms results into a meaningful summary
///    - Produces a final answer
///
/// # Configuration Pattern
/// The test demonstrates the recommended configuration approach:
/// - Define search parameters as constants in the implementation scope
/// - Pass configuration through the query generator closure
/// - Keep the strategy struct minimal and focused on query generation
#[test_log::test(tokio::test)]
async fn test_pgvector_retrieve_dynamic_search() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    let code = "fn main() { println!(\"Hello, World!\"); }";
    std::fs::write(&codefile, code).unwrap();

    let (_pgv_db_container, pgv_db_url) = swiftide_test_utils::start_postgres().await;

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;
    mock_chat_completions(&mock_server).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let fastembed =
        integrations::fastembed::FastEmbed::try_default().expect("Could not create FastEmbed");

    // Configure Pgvector with a default vector size, a single embedding
    // and in addition to embedding the text metadata, also store it in a field
    let pgv_storage = PgVector::builder()
        .db_url(pgv_db_url)
        .vector_size(384)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(METADATA_QA_CODE_NAME)
        .with_metadata("filter")
        .table_name("swiftide_test")
        .build()
        .unwrap();

    // Drop the existing test table before running the test
    println!("Dropping existing test table & index if it exists");
    let drop_table_sql = "DROP TABLE IF EXISTS swiftide_test";
    let drop_index_sql = "DROP INDEX IF EXISTS swiftide_test_embedding_idx";

    if let Ok(pool) = pgv_storage.get_pool().await {
        sqlx::query(drop_table_sql)
            .execute(pool)
            .await
            .expect("Failed to execute SQL query for dropping the table");
        sqlx::query(drop_index_sql)
            .execute(pool)
            .await
            .expect("Failed to execute SQL query for dropping the index");
    } else {
        panic!("Unable to acquire database connection pool");
    }

    Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(ChunkCode::try_for_language("rust").unwrap())
        .then(MetadataQACode::new(openai_client.clone()))
        .then(|mut node: indexing::Node| {
            node.metadata
                .insert("filter".to_string(), "true".to_string());
            Ok(node)
        })
        .then_in_batch(transformers::Embed::new(fastembed.clone()).with_batch_size(20))
        .log_nodes()
        .then_store_with(pgv_storage.clone())
        .run()
        .await
        .unwrap();

    // First, we'll clone pgv_storage before using it in the closure
    let pgv_storage_for_closure = pgv_storage.clone();

    // Configure search strategy
    // Create a custom query generator with metadata filtering
    let custom_strategy = query::search_strategies::CustomStrategy::from_query(
        move |query_node| -> Result<sqlx::QueryBuilder<'static, sqlx::Postgres>> {
            const CUSTOM_STRATEGY_MAX_RESULTS: i64 = 5;
            let mut builder = sqlx::QueryBuilder::new("");
            let table: &str = pgv_storage_for_closure.get_table_name();

            // Get column definitions
            let default_fields: Vec<_> = PgVectorBuilder::default_fields();
            let default_columns: Vec<&str> =
                default_fields.iter().map(FieldConfig::field_name).collect();

            // Start building the query properly
            builder.push("SELECT ");
            builder.push(default_columns.join(", "));
            builder.push(" FROM ");
            builder.push(table);

            // Add metadata filter
            builder.push(" WHERE meta_");
            builder.push(PgVector::normalize_field_name("filter"));
            builder.push(" @> ");
            builder.push("'{\"filter\": \"true\"}'::jsonb");

            // Add vector similarity ordering
            let vector_field = VectorConfig::from(EmbeddedField::Combined).field;
            builder.push(" ORDER BY ");
            builder.push(vector_field);
            builder.push(" <=> ");
            // Let QueryBuilder handle the parameter placeholders
            builder.push_bind(
                query_node
                    .embedding
                    .as_ref()
                    .ok_or_else(|| anyhow!("Missing embedding in query state"))?
                    .clone(),
            );
            builder.push("::vector");

            // Add LIMIT clause
            builder.push(" LIMIT ");

            builder.push_bind(CUSTOM_STRATEGY_MAX_RESULTS);

            Ok(builder)
        },
    );

    let query_pipeline = query::Pipeline::from_search_strategy(custom_strategy)
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(fastembed.clone()))
        .then_retrieve(pgv_storage.clone())
        .then_transform_response(response_transformers::Summary::from_client(
            openai_client.clone(),
        ))
        .then_answer(answers::Simple::from_client(openai_client.clone()));

    let result: Query<states::Answered> = query_pipeline.query("What is swiftide?").await.unwrap();

    assert_eq!(
        result.answer(),
        "\n\nHello there, how may I assist you today?"
    );

    let first_document = result.documents().first().unwrap();

    // The custom query explicitly skipped metadata
    let expected = Document::builder()
        .content("fn main() { println!(\"Hello, World!\"); }")
        .build()
        .unwrap();
    assert_eq!(first_document, &expected);
}
