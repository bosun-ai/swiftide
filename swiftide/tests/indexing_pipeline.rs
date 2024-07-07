//! This module contains tests for the indexing pipeline in the Swiftide project.
//! The tests validate the functionality of the pipeline, ensuring it processes data correctly
//! from a temporary file, simulates API responses, and stores data accurately in the Qdrant vector database.

use qdrant_client::qdrant::{SearchPointsBuilder, Value};
use serde_json::json;
use swiftide::{indexing::Pipeline, loaders::FileLoader, *};
use temp_dir::TempDir;
use testcontainers::core::wait::HttpWaitStrategy;
use testcontainers::runners::AsyncRunner;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-3.5-turbo-0125",
            "system_fingerprint": "fp_44709d6fcb",
            "choices": [{
              "index": 0,
              "message": {
                "role": "assistant",
                "content": "\n\nHello there, how may I assist you today?",
              },
              "logprobs": null,
              "finish_reason": "stop"
            }],
            "usage": {
              "prompt_tokens": 9,
              "completion_tokens": 12,
              "total_tokens": 21
            }
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
          "object": "list",
          "data": [
            {
              "object": "embedding",
              "embedding": vec![0; 1536],
              "index": 0
            }
          ],
          "model": "text-embedding-ada-002",
          "usage": {
            "prompt_tokens": 8,
            "total_tokens": 8
        }
        })))
        .mount(&mock_server)
        .await;

    // Setup OpenAI client with the mock server
    let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server.uri());
    let async_openai = async_openai::Client::with_config(config);

    let openai_client = integrations::openai::OpenAI::builder()
        .client(async_openai)
        .default_options(
            integrations::openai::Options::builder()
                .embed_model("text-embedding-3-small")
                .prompt_model("gpt-4o")
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    // Setup Redis container for caching in the test
    let redis = testcontainers::GenericImage::new("redis", "7.2.4")
        .with_exposed_port(6379.into())
        .with_wait_for(testcontainers::core::WaitFor::message_on_stdout(
            "Ready to accept connections",
        ))
        .start()
        .await
        .expect("Redis started");
    let redis_url = format!(
        "redis://{host}:{port}",
        host = redis.get_host().await.unwrap(),
        port = redis.get_host_port_ipv4(6379).await.unwrap()
    );

    let qdrant = testcontainers::GenericImage::new("qdrant/qdrant", "v1.9.2")
        .with_exposed_port(6334.into())
        .with_exposed_port(6333.into())
        .with_wait_for(testcontainers::core::WaitFor::http(
            HttpWaitStrategy::new("/readyz")
                .with_port(6333.into())
                .with_expected_status_code(200_u16),
        ))
        .start()
        .await
        .expect("Qdrant started");
    let qdrant_url = format!(
        "http://127.0.0.1:{port}",
        port = qdrant.get_host_port_ipv4(6334).await.unwrap()
    );

    // Coverage CI runs in container, just accept the double qdrant and use the service instead
    let qdrant_url = std::env::var("QDRANT_URL").unwrap_or(qdrant_url);

    println!("Qdrant URL: {}", qdrant_url);

    let result = Pipeline::from_loader(FileLoader::new(tempdir.path()).with_extensions(&["rs"]))
        .then_chunk(transformers::ChunkCode::try_for_language("rust").unwrap())
        .then(transformers::MetadataQACode::new(openai_client.clone()))
        .filter_cached(integrations::redis::Redis::try_from_url(&redis_url, "prefix").unwrap())
        .then_in_batch(1, transformers::Embed::new(openai_client.clone()))
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
        println!("{}", received_requests);
    };

    result.expect("Indexing pipeline failed");

    let qdrant_client = qdrant_client::Qdrant::from_url(&qdrant_url)
        .build()
        .unwrap();

    let search_request =
        SearchPointsBuilder::new("swiftide-test", vec![0_f32; 1536], 10).with_payload(true);

    let search_response = qdrant_client.search_points(search_request).await.unwrap();

    dbg!(&search_response);

    let first = search_response.result.first().unwrap();

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
            .get("Questions and Answers")
            .unwrap()
            .as_str()
            .unwrap(),
        "\n\nHello there, how may I assist you today?"
    );
}
