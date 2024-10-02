use swiftide::indexing::{self, *};
use swiftide::query::search_strategies::HybridSearch;
use swiftide::{integrations, query};
use swiftide_integrations::fastembed::FastEmbed;
use swiftide_query::{answers, query_transformers, response_transformers};
use swiftide_test_utils::*;
use temp_dir::TempDir;
use wiremock::MockServer;

#[test_log::test(tokio::test)]
async fn test_query_pipeline() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    std::fs::write(&codefile, "fn main() { println!(\"Hello, World!\"); }").unwrap();

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;

    mock_chat_completions(&mock_server).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let (_qdrant, qdrant_url) = start_qdrant().await;

    let qdrant_client = integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
        .unwrap()
        .vector_size(384)
        .collection_name("swiftide-test".to_string())
        .build()
        .unwrap();

    let fastembed = integrations::fastembed::FastEmbed::try_default().unwrap();

    println!("Qdrant URL: {qdrant_url}");

    indexing::Pipeline::from_loader(
        loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]),
    )
    .then_chunk(transformers::ChunkCode::try_for_language("rust").unwrap())
    .then_in_batch(transformers::Embed::new(fastembed.clone()).with_batch_size(1))
    .then_store_with(qdrant_client.clone())
    .run()
    .await
    .unwrap();

    let query_pipeline = query::Pipeline::default()
        .then_transform_query(query_transformers::GenerateSubquestions::from_client(
            openai_client.clone(),
        ))
        .then_transform_query(query_transformers::Embed::from_client(fastembed.clone()))
        .then_retrieve(qdrant_client.clone())
        .then_transform_response(response_transformers::Summary::from_client(
            openai_client.clone(),
        ))
        .then_answer(answers::Simple::from_client(openai_client.clone()));

    let result = query_pipeline.query("What is swiftide?").await.unwrap();

    assert!(result.embedding.is_some());
    assert!(!result.answer().is_empty());
}

#[test_log::test(tokio::test)]
async fn test_hybrid_search_qdrant() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    std::fs::write(&codefile, "fn main() { println!(\"Hello, World!\"); }").unwrap();

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;

    mock_chat_completions(&mock_server).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let (_qdrant, qdrant_url) = start_qdrant().await;

    let batch_size = 10;

    let qdrant_client = integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
        .unwrap()
        .vector_size(384)
        .batch_size(batch_size)
        .with_vector(EmbeddedField::Combined)
        .with_sparse_vector(EmbeddedField::Combined)
        .collection_name("swiftide-hybrid")
        .build()
        .unwrap();

    let fastembed_sparse = FastEmbed::try_default_sparse().unwrap().clone();
    let fastembed = FastEmbed::try_default().unwrap().clone();

    println!("Qdrant URL: {qdrant_url}");

    indexing::Pipeline::from_loader(
        loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]),
    )
    .then_chunk(transformers::ChunkCode::try_for_language("rust").unwrap())
    .then_in_batch(transformers::Embed::new(fastembed.clone()).with_batch_size(batch_size))
    .then_in_batch(
        transformers::SparseEmbed::new(fastembed_sparse.clone()).with_batch_size(batch_size),
    )
    .then_store_with(qdrant_client.clone())
    .run()
    .await
    .unwrap();

    let collection = qdrant_client
        .client()
        .collection_info("swiftide-hybrid")
        .await
        .unwrap();

    dbg!(collection);

    let query_pipeline = query::Pipeline::from_search_strategy(HybridSearch::default())
        .then_transform_query(query_transformers::Embed::from_client(fastembed.clone()))
        .then_transform_query(query_transformers::SparseEmbed::from_client(
            fastembed_sparse.clone(),
        ))
        .then_retrieve(qdrant_client.clone())
        .then_answer(answers::Simple::from_client(openai_client.clone()));

    let result = query_pipeline.query("What is swiftide?").await.unwrap();

    assert!(result.embedding.is_some());
    assert!(!result.answer().is_empty());
}
