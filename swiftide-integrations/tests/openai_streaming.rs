#![cfg(feature = "openai")]

use std::fmt::Write as _;

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::{
    ChatCompletion, DynStructuredPrompt, SimplePrompt,
    chat_completion::{ChatCompletionRequest, ChatMessage},
};
use swiftide_integrations::openai::OpenAI;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

fn chat_events() -> String {
    let chunks = [
        json!({"choices": [{"index": 0, "delta": {"role": "assistant", "content": "{\"answer\":"}, "finish_reason": null}]}),
        json!({"choices": [{"index": 0, "delta": {"content": "42}"}, "finish_reason": "stop"}]}),
        json!({"choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12, "prompt_tokens_details": {"cached_tokens": 8}, "completion_tokens_details": {"reasoning_tokens": 1}}}),
    ];
    let mut events = String::new();
    for mut chunk in chunks {
        chunk["id"] = "chatcmpl-test".into();
        chunk["object"] = "chat.completion.chunk".into();
        chunk["created"] = 1.into();
        chunk["model"] = "test-model".into();
        write!(events, "data: {chunk}\n\n").unwrap();
    }
    events.push_str("data: [DONE]\n\n");
    events
}

fn responses_events() -> String {
    let response = json!({
        "id": "resp-test", "object": "response", "created_at": 1,
        "model": "test-model", "status": "completed",
        "output": [{"id": "msg-test", "type": "message", "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": "{\"answer\":42}", "annotations": []}]}],
        "usage": {"input_tokens": 10, "output_tokens": 2, "total_tokens": 12,
            "input_tokens_details": {"cached_tokens": 8}, "output_tokens_details": {"reasoning_tokens": 1}}
    });
    let completed =
        json!({"type": "response.completed", "sequence_number": 1, "response": response});
    format!("event: response.completed\ndata: {completed}\n\n")
}

async fn client(responses: bool) -> (MockServer, OpenAI) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(if responses {
                    responses_events()
                } else {
                    chat_events()
                }),
        )
        .mount(&server)
        .await;
    let config = async_openai::config::OpenAIConfig::new().with_api_base(server.uri());
    let client = OpenAI::builder()
        .client(async_openai::Client::with_config(config))
        .default_prompt_model("test-model")
        .use_responses_api(responses)
        .streaming(true)
        .stream_full(false)
        .build()
        .unwrap();
    (server, client)
}

#[tokio::test]
async fn streaming_transport_supports_chat_simple_and_structured_prompts_on_both_apis() {
    for responses in [false, true] {
        let (server, client) = client(responses).await;
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("answer".into())])
            .build()
            .unwrap();
        let response = client.complete(&request).await.unwrap();
        assert_eq!(response.message(), Some("{\"answer\":42}"));
        let usage = response.usage.unwrap().normalized();
        assert_eq!(usage.total_tokens, 12);
        let details = usage.details.unwrap();
        assert_eq!(details.input.cached_tokens, Some(8));
        assert_eq!(details.output.reasoning_tokens, Some(1));

        assert_eq!(
            client.prompt("answer".into()).await.unwrap(),
            "{\"answer\":42}"
        );
        let schema = schemars::schema_for!(Value);
        assert_eq!(
            client
                .structured_prompt_dyn("answer".into(), schema)
                .await
                .unwrap(),
            json!({"answer": 42})
        );

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3);
        for request in &requests {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["stream"], true);
            assert_eq!(
                request.url.path(),
                if responses {
                    "/responses"
                } else {
                    "/chat/completions"
                }
            );
            if !responses {
                assert_eq!(body["stream_options"]["include_usage"], true);
            }
        }
        let structured: Value = serde_json::from_slice(&requests[2].body).unwrap();
        assert_eq!(
            if responses {
                &structured["text"]["format"]["type"]
            } else {
                &structured["response_format"]["type"]
            },
            "json_schema"
        );
    }
}

#[tokio::test]
async fn streamed_chat_preserves_cached_and_reasoning_usage() {
    let (_server, client) = client(false).await;
    let request = ChatCompletionRequest::builder()
        .messages(vec![ChatMessage::User("answer".into())])
        .build()
        .unwrap();
    let mut stream = client.complete_stream(&request).await;
    let mut last = None;
    while let Some(chunk) = stream.next().await {
        last = Some(chunk.unwrap());
    }
    let details = last.unwrap().usage.unwrap().normalized().details.unwrap();
    assert_eq!(details.input.cached_tokens, Some(8));
    assert_eq!(details.output.reasoning_tokens, Some(1));
}

#[tokio::test]
async fn streaming_prompt_errors_are_returned_instead_of_partial_success() {
    for responses in [false, true] {
        let (server, client) = client(responses).await;
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string("data: {invalid-json}\n\n"),
            )
            .mount(&server)
            .await;
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("answer".into())])
            .build()
            .unwrap();
        assert!(client.complete(&request).await.is_err());
        assert!(client.prompt("answer".into()).await.is_err());
        assert!(
            client
                .structured_prompt_dyn("answer".into(), schemars::schema_for!(Value))
                .await
                .is_err()
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }
}
