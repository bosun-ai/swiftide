use std::sync::{Arc, Mutex};

use reqwest::Client;
use serde_json::json;
use swiftide_langfuse::{Configuration, LangfuseBatchManager, LangfuseLayer, layer_from_config};
use tokio::task::yield_now;
use tracing::{Level, info, span};
use tracing_subscriber::{Registry, layer::SubscriberExt};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

#[test_log::test(tokio::test)]
async fn integration_tracing_layer_sends_to_langfuse() {
    // Start Wiremock server
    let mock_server = MockServer::start().await;

    // Mock a successful ingestion response
    let response = ResponseTemplate::new(200).set_body_raw(
        r#"{"successes":[{"id":"abc","status":200}],"errors":[]}"#,
        "application/json",
    );

    let body = Arc::new(Mutex::new(None));
    let body_clone = body.clone();

    Mock::given(method("POST"))
        .and(path("/api/public/ingestion"))
        .respond_with(move |req: &wiremock::Request| {
            let body_clone = body_clone.clone();
            let body_str = String::from_utf8_lossy(&req.body).to_string();
            let mut lock = body_clone.lock().unwrap();
            *lock = Some(body_str);
            response.clone()
        })
        .expect(1)
        .mount(&mock_server)
        .await;

    // Prepare Langfuse config to point to the mock server
    let config = Configuration {
        base_path: mock_server.uri(),
        user_agent: Some("integration-test".into()),
        client: Client::new(),
        basic_auth: Some(("PUBLIC".into(), Some("SECRET".into()))),
        ..Default::default()
    };

    // Set up tracing layer
    let batch_manager = LangfuseBatchManager::new(config);
    let layer = LangfuseLayer::from_batch_manager(&batch_manager);

    batch_manager.clone().spawn();

    // Install subscriber and layer
    let subscriber = Registry::default().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let span = span!(
            Level::INFO,
            "test_span",
            "langfuse.input" = "LANGFUSE INPUT",
            "langfuse.output" = "LANGFUSE OUTPUT",
            "langfuse.model" = "LANGFUSE MODEL",
            "otel.name" = "OTEL.OVERWRITE",
            foo = 42
        );
        let _enter = span.enter();
        info!(bar = "baz", "Hello from integration test");
    });

    // Give some time for the async tasks to run
    yield_now().await;
    // Force the flush as the batch manager is not dropped yet
    batch_manager.flush().await.unwrap();

    // Assert request received
    mock_server.verify().await;

    insta::with_settings!({
        filters => vec![
        // UUID v4/v5 pattern
        (r#""[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}""#, r#""<UUID>""#),
        // Improved ISO8601 datetime filter, matching both Z and offsets
        (r#""\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})""#, r#""<TIMESTAMP>""#),
        // Unix timestamp (with optional ms)
        (r#""\d{10,13}""#, r#""<UNIX_TIMESTAMP>""#),
        ]
    }, {
        insta::assert_snapshot!(body.lock().unwrap().as_ref().unwrap())
    });
}
