/// This example demonstrates how to index markdown into PGVector
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
    integrations::{self, fastembed::FastEmbed, pgvector::PgVector},
    query::{self, answers, query_transformers, response_transformers},
    traits::SimplePrompt,
};

async fn ask_query(
    llm_client: impl SimplePrompt + Clone + 'static,
    embed: FastEmbed,
    vector_store: PgVector,
    questions: Vec<String>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // By default the search strategy is SimilaritySingleEmbedding
    // which takes the latest query, embeds it, and does a similarity search
    //
    // Pgvector will return an error if multiple embeddings are set
    //
    // The pipeline generates subquestions to increase semantic coverage, embeds these in a single
    // embedding, retrieves the default top_k documents, summarizes them and uses that as context
    // for the final answer.
    let pipeline = query::Pipeline::default()
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            llm_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(embed))
        .then_retrieve(vector_store.clone())
        .then_transform_response(response_transformers::Summary::from_client(
            llm_client.clone(),
        ))
        .then_answer(answers::Simple::from_client(llm_client.clone()));

    let results: Vec<String> = pipeline
        .query_all(questions)
        .await?
        .iter()
        .map(|result| result.answer().to_string())
        .collect();

    Ok(results)
}

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

    tracing::info!("PgVector Indexing completed successfully");

    let questions: Vec<String> = vec![
            "What is SwiftIDE? Provide a clear, comprehensive summary in under 50 words.".into(),
            "How can I use SwiftIDE to connect with the Ethereum blockchain? Please provide a concise, comprehensive summary in less than 50 words.".into(),
        ];

    ask_query(
        llm_client.clone(),
        fastembed.clone(),
        pgv_storage.clone(),
        questions,
    )
    .await?
    .iter()
    .enumerate()
    .for_each(|(i, result)| {
        tracing::info!("*** Answer Q{} ***", i + 1);
        tracing::info!("{}", result);
        tracing::info!("===X===");
    });

    tracing::info!("PgVector Indexing & retrieval test completed successfully");

    Ok(())
}
