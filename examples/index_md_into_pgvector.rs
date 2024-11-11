/**
* This example demonstrates how to index markdown into PGVector
*/
use std::path::PathBuf;
use swiftide::{
    indexing::{
        self,
        loaders::FileLoader,
        transformers::{
            metadata_qa_text::NAME as METADATA_QA_TEXT_NAME, ChunkMarkdown, Embed, MetadataQAText,
        },
        EmbeddedField,
    },
    integrations::{self, pgvector::PgVector},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting PgVector indexing test");

    // Get the manifest directory path
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    // Create a PathBuf to test dataset from the manifest directory
    let test_dataset_path = PathBuf::from(manifest_dir).join("../README.md");

    tracing::info!("Test Dataset path: {:?}", test_dataset_path);

    let (_pgv_db_container, pgv_db_url) = swiftide_test_utils::start_postgres().await;

    tracing::info!("pgv_db_url :: {:#?}", pgv_db_url);

    let llm_client = integrations::ollama::Ollama::default()
        .with_default_prompt_model("llama3.2:latest")
        .to_owned();

    let fastembed =
        integrations::fastembed::FastEmbed::try_default().expect("Could not create FastEmbed");

    // Configure Pgvector with a default vector size, a single embedding
    // and in addition to embedding the text metadata, also store it in a field
    let pgv_storage = PgVector::builder()
        .db_url(pgv_db_url)
        .vector_size(384)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(METADATA_QA_TEXT_NAME)
        .table_name("swiftide_pgvector_test".to_string())
        .build()
        .unwrap();

    // Drop the existing test table before running the test
    tracing::info!("Dropping existing test table & index if it exists");
    let drop_table_sql = "DROP TABLE IF EXISTS swiftide_pgvector_test";
    let drop_index_sql = "DROP INDEX IF EXISTS swiftide_pgvector_test_embedding_idx";

    if let Ok(pool) = pgv_storage.get_pool().await {
        sqlx::query(drop_table_sql).execute(pool).await?;
        sqlx::query(drop_index_sql).execute(pool).await?;
    } else {
        return Err("Failed to get database connection pool".into());
    }

    tracing::info!("Starting indexing pipeline");
    indexing::Pipeline::from_loader(FileLoader::new(test_dataset_path).with_extensions(&["md"]))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(llm_client.clone()))
        .then_in_batch(Embed::new(fastembed.clone()).with_batch_size(100))
        .then_store_with(pgv_storage.clone())
        .run()
        .await?;

    tracing::info!("PgVector Indexing test completed successfully");
    Ok(())
}
