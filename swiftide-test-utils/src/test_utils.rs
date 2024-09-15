#![allow(missing_docs)]
#![allow(clippy::missing_panics_doc)]

use serde_json::json;
use testcontainers::{
    core::wait::HttpWaitStrategy, runners::AsyncRunner as _, ContainerAsync, GenericImage,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use swiftide_integrations as integrations;

pub fn openai_client(
    mock_server_uri: &str,
    embed_model: &str,
    prompt_model: &str,
) -> integrations::openai::OpenAI {
    let config = async_openai::config::OpenAIConfig::new().with_api_base(mock_server_uri);
    let async_openai = async_openai::Client::with_config(config);
    integrations::openai::OpenAI::builder()
        .client(async_openai)
        .default_options(
            integrations::openai::Options::builder()
                .embed_model(embed_model)
                .prompt_model(prompt_model)
                .build()
                .unwrap(),
        )
        .build()
        .expect("Can create OpenAI client.")
}

/// Setup Qdrant container.
/// Returns container server and `server_url`.
pub async fn start_qdrant() -> (ContainerAsync<GenericImage>, String) {
    let qdrant = testcontainers::GenericImage::new("qdrant/qdrant", "v1.11.3")
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
    (qdrant, qdrant_url)
}

/// Setup Redis container for caching in the test.
/// Returns container server and `server_url`.
pub async fn start_redis() -> (ContainerAsync<GenericImage>, String) {
    let redis = testcontainers::GenericImage::new("redis", "7-alpine")
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
    (redis, redis_url)
}

/// Mock embeddings creation endpoint.
/// `embeddings_count` controls number of returned embedding vectors.
pub async fn mock_embeddings(mock_server: &MockServer, embeddings_count: u8) {
    let data = (0..embeddings_count)
        .map(|i| {
            json!( {
              "object": "embedding",
              "embedding": vec![0; 1536],
              "index": i
            })
        })
        .collect::<Vec<serde_json::Value>>();
    let data: serde_json::Value = serde_json::Value::Array(data);
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
          "object": "list",
          "data": data,
          "model": "text-embedding-ada-002",
          "usage": {
            "prompt_tokens": 8,
            "total_tokens": 8
        }
        })))
        .mount(mock_server)
        .await;
}

pub async fn mock_chat_completions(mock_server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1_677_652_288,
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
        .mount(mock_server)
        .await;
}
