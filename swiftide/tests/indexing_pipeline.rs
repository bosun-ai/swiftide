//! This module contains tests for the indexing pipeline in the Swiftide project.
//! The tests validate the functionality of the pipeline, ensuring it processes data correctly
//! from a temporary file, simulates API responses, and stores data accurately in the Qdrant vector
//! database.

use qdrant_client::qdrant::vectors_output::VectorsOptions;
use qdrant_client::qdrant::{ScrollPointsBuilder, SearchPointsBuilder, Value};
use swiftide::indexing::*;
use swiftide::integrations;
use swiftide_test_utils::*;
use temp_dir::TempDir;
use wiremock::MockServer;

/// Tests the indexing pipeline without any mocks.
///
/// This test sets up a temporary directory and file, simulates API responses using mock servers,
/// configures an OpenAI client, and runs the indexing pipeline. It then validates that the data
/// is correctly stored in the Qdrant vector database.
///
/// # Panics
/// Panics if any of the setup steps fail, such as creating the temporary directory or file,
/// starting the mock server, or configuring the OpenAI client.
///
/// # Errors
/// If the indexing pipeline encounters an error, the test will print the received requests
/// for debugging purposes.
#[test_log::test(tokio::test)]
async fn test_indexing_pipeline() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    std::fs::write(&codefile, "fn main() { println!(\"Hello, World!\"); }").unwrap();

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;

    mock_chat_completions(&mock_server).await;

    mock_embeddings(&mock_server, 1).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let (_redis, redis_url) = start_redis().await;

    let (qdrant_container, qdrant_url) = start_qdrant().await;

    // Coverage CI runs in container, just accept the double qdrant and use the service instead
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or(qdrant_url);

    println!("Qdrant URL: {qdrant_url}");

    let result =
        Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
            .with_default_llm_client(openai_client.clone())
            .then_chunk(transformers::ChunkCode::try_for_language("rust").unwrap())
            .then(transformers::MetadataQACode::default())
            .filter_cached(integrations::redis::Redis::try_from_url(&redis_url, "prefix").unwrap())
            .then_in_batch(transformers::Embed::new(openai_client.clone()).with_batch_size(1))
            .log_nodes()
            .then_store_with(
                integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
                    .unwrap()
                    .vector_size(1536)
                    .collection_name("swiftide-test".to_string())
                    .build()
                    .unwrap(),
            )
            .run()
            .await;

    if result.is_err() {
        println!("\n Received the following requests: \n");
        // Just some serde magic to pretty print requests on failure
        let received_requests = mock_server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|req| {
                format!(
                    "- {} {}\n{}",
                    req.method,
                    req.url,
                    serde_json::to_string_pretty(
                        &serde_json::from_slice::<Value>(&req.body).unwrap()
                    )
                    .unwrap()
                )
            })
            .collect::<Vec<String>>()
            .join("\n---\n");
        println!("{received_requests}");
    }

    result.expect("Indexing pipeline failed");

    let qdrant_client = qdrant_client::Qdrant::from_url(&qdrant_url)
        .build()
        .unwrap();

    let stored_node = qdrant_client
        .scroll(
            ScrollPointsBuilder::new("swiftide-test")
                .limit(1)
                .with_payload(true)
                .with_vectors(true),
        )
        .await
        .unwrap();

    dbg!(
        std::str::from_utf8(&qdrant_container.stdout_to_vec().await.unwrap())
            .unwrap()
            .split('\n')
            .collect::<Vec<_>>()
    );
    dbg!(
        std::str::from_utf8(&qdrant_container.stderr_to_vec().await.unwrap())
            .unwrap()
            .split('\n')
            .collect::<Vec<_>>()
    );
    dbg!(stored_node);

    let search_request =
        SearchPointsBuilder::new("swiftide-test", vec![0_f32; 1536], 10).with_payload(true);

    let search_response = qdrant_client.search_points(search_request).await.unwrap();

    dbg!(&search_response);

    let first = search_response.result.first().unwrap();

    dbg!(first);
    assert!(first
        .payload
        .get("path")
        .unwrap()
        .as_str()
        .unwrap()
        .ends_with("main.rs"));
    assert_eq!(
        first.payload.get("content").unwrap().as_str().unwrap(),
        "fn main() { println!(\"Hello, World!\"); }"
    );
    assert_eq!(
        first
            .payload
            .get("Questions and Answers (code)")
            .unwrap()
            .as_str()
            .unwrap(),
        "\n\nHello there, how may I assist you today?"
    );
}

#[test_log::test(tokio::test)]
async fn test_named_vectors() {
    // Setup temporary directory and file for testing
    let tempdir = TempDir::new().unwrap();
    let codefile = tempdir.child("main.rs");
    std::fs::write(&codefile, "fn main() { println!(\"Hello, World!\"); }").unwrap();

    // Setup mock servers to simulate API responses
    let mock_server = MockServer::start().await;

    mock_chat_completions(&mock_server).await;

    mock_embeddings(&mock_server, 2).await;

    let openai_client = openai_client(&mock_server.uri(), "text-embedding-3-small", "gpt-4o");

    let (_redis, redis_url) = start_redis().await;

    let (_qdrant, qdrant_url) = start_qdrant().await;

    // Coverage CI runs in container, just accept the double qdrant and use the service instead
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or(qdrant_url);

    println!("Qdrant URL: {qdrant_url}");

    let result =
        Pipeline::from_loader(loaders::FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
            .with_embed_mode(EmbedMode::PerField)
            .then_chunk(transformers::ChunkCode::try_for_language("rust").unwrap())
            .then(transformers::MetadataQACode::new(openai_client.clone()))
            .filter_cached(integrations::redis::Redis::try_from_url(&redis_url, "prefix").unwrap())
            .then_in_batch(transformers::Embed::new(openai_client.clone()).with_batch_size(10))
            .then_store_with(
                integrations::qdrant::Qdrant::try_from_url(&qdrant_url)
                    .unwrap()
                    .vector_size(1536)
                    .collection_name("named-vectors-test".to_string())
                    .with_vector(EmbeddedField::Chunk)
                    .with_vector(EmbeddedField::Metadata(
                        transformers::metadata_qa_code::NAME.into(),
                    ))
                    .build()
                    .unwrap(),
            )
            .run()
            .await;

    result.expect("Named vectors test indexing pipeline failed");

    let qdrant_client = qdrant_client::Qdrant::from_url(&qdrant_url)
        .build()
        .unwrap();

    let search_request = SearchPointsBuilder::new("named-vectors-test", vec![0_f32; 1536], 10)
        .vector_name(
            EmbeddedField::Metadata(transformers::metadata_qa_code::NAME.into()).to_string(),
        )
        .with_payload(true)
        .with_vectors(true);

    let search_response = qdrant_client.search_points(search_request).await.unwrap();

    let first = search_response.result.into_iter().next().unwrap();

    assert!(first
        .payload
        .get("path")
        .unwrap()
        .as_str()
        .unwrap()
        .ends_with("main.rs"));
    assert_eq!(
        first.payload.get("content").unwrap().as_str().unwrap(),
        "fn main() { println!(\"Hello, World!\"); }"
    );
    assert_eq!(
        first
            .payload
            .get("Questions and Answers (code)")
            .unwrap()
            .as_str()
            .unwrap(),
        "\n\nHello there, how may I assist you today?"
    );

    let vectors = first.vectors.expect("Response has vectors");
    let VectorsOptions::Vectors(named_vectors) = vectors
        .vectors_options
        .expect("Response has vector options")
    else {
        panic!("Expected named vectors");
    };
    let vectors = named_vectors.vectors;

    assert_eq!(vectors.len(), 2);
    assert!(vectors.contains_key(&EmbeddedField::Chunk.to_string()));
    assert!(vectors.contains_key(
        &EmbeddedField::Metadata(transformers::metadata_qa_code::NAME.into()).to_string()
    ));
}
