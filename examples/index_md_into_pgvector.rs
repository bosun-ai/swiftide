/**
* This example demonstrates how to use the Pgvector integration with Swiftide
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
    let test_dataset_path = PathBuf::from(manifest_dir).join("test_dataset");
    tracing::info!("Test Dataset path: {:?}", test_dataset_path);

    let pgv_db_url = std::env::var("DATABASE_URL")
        .as_deref()
        .unwrap_or("postgresql://myuser:mypassword@localhost:5432/mydatabase")
        .to_owned();

    let ollama_client = integrations::ollama::Ollama::default()
        .with_default_prompt_model("llama3.2:latest")
        .to_owned();

    let fastembed =
        integrations::fastembed::FastEmbed::try_default().expect("Could not create FastEmbed");

    // Configure Pgvector with a default vector size, a single embedding
    // and in addition to embedding the text metadata, also store it in a field
    let pgv_storage = PgVector::builder()
        .try_from_url(pgv_db_url, Some(10))
        .await
        .expect("Failed to connect to postgres server")
        .vector_size(384)
        .with_vector(EmbeddedField::Combined)
        .with_metadata(METADATA_QA_TEXT_NAME)
        .table_name("swiftide_pgvector_test".to_string())
        .build()
        .unwrap();

    // Drop the existing test table before running the test
    tracing::info!("Dropping existing test table if it exists");
    let drop_table_sql = "DROP TABLE IF EXISTS swiftide_pgvector_test";

    if let Some(pool) = pgv_storage.get_pool() {
        sqlx::query(drop_table_sql).execute(pool).await?;
    } else {
        return Err("Failed to get database connection pool".into());
    }

    tracing::info!("Starting indexing pipeline");
    indexing::Pipeline::from_loader(FileLoader::new(test_dataset_path).with_extensions(&["md"]))
        .then_chunk(ChunkMarkdown::from_chunk_range(10..2048))
        .then(MetadataQAText::new(ollama_client.clone()))
        .then_in_batch(Embed::new(fastembed).with_batch_size(100))
        .then_store_with(pgv_storage.clone())
        .run()
        .await?;

    tracing::info!("Indexing test completed successfully");
    Ok(())
}
