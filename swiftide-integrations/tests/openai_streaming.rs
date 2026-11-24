#![cfg(feature = "openai")]

use std::fmt::Write as _;

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::{
    ChatCompletion, DynStructuredPrompt, SimplePrompt,
    chat_completion::errors::LanguageModelError,
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

async fn assert_unfinished_stream_fails(responses: bool, events: &str) {
    let (server, client) = client(responses).await;
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(events),
        )
        .mount(&server)
        .await;
    let request = ChatCompletionRequest::builder()
        .messages(vec![ChatMessage::User("answer".into())])
        .build()
        .unwrap();
    assert!(matches!(
        client.complete(&request).await,
        Err(LanguageModelError::TransientError(_))
    ));
    assert!(matches!(
        client.prompt("answer".into()).await,
        Err(LanguageModelError::TransientError(_))
    ));
    assert!(matches!(
        client
            .structured_prompt_dyn("answer".into(), schemars::schema_for!(Value))
            .await,
        Err(LanguageModelError::TransientError(_))
    ));
    let items = client
        .complete_stream(&request)
        .await
        .collect::<Vec<_>>()
        .await;
    assert!(matches!(
        items.last(),
        Some(Err(LanguageModelError::TransientError(_)))
    ));
}

#[tokio::test]
async fn chat_partial_content_followed_by_eof_is_not_a_successful_completion() {
    let partial = json!({
        "id": "chatcmpl-test", "object": "chat.completion.chunk", "created": 1,
        "model": "test-model", "choices": [{"index": 0,
            "delta": {"role": "assistant", "content": "{\"answer\":42}"}, "finish_reason": null}]
    });
    assert_unfinished_stream_fails(false, &format!("data: {partial}\n\n")).await;
    assert_unfinished_stream_fails(false, &format!("data: {partial}\n\ndata: [DONE]\n\n")).await;
}

#[tokio::test]
async fn responses_partial_content_followed_by_eof_is_not_a_successful_completion() {
    let partial = json!({"type": "response.output_text.delta", "sequence_number": 0,
        "item_id": "msg-test", "output_index": 0, "content_index": 0, "delta": "{\"answer\":42}"});
    assert_unfinished_stream_fails(true, &format!("data: {partial}\n\n")).await;
}

#[tokio::test]
async fn empty_streams_are_transient_errors_on_both_apis() {
    for responses in [false, true] {
        assert_unfinished_stream_fails(responses, "").await;
    }
}

#[tokio::test]
async fn a_chat_tool_only_finish_is_a_valid_collected_completion() {
    let (server, client) = client(false).await;
    server.reset().await;
    let chunk = json!({
        "id": "chatcmpl-test", "object": "chat.completion.chunk", "created": 1,
        "model": "test-model", "choices": [{"index": 0, "delta": {"role": "assistant",
            "tool_calls": [{"index": 0, "id": "call-test", "type": "function",
                "function": {"name": "lookup", "arguments": "{\"q\":\"rust\"}"}}]},
            "finish_reason": "tool_calls"}]
    });
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(format!("data: {chunk}\n\ndata: [DONE]\n\n")),
        )
        .mount(&server)
        .await;
    let request = ChatCompletionRequest::builder()
        .messages(vec![ChatMessage::User("lookup rust".into())])
        .build()
        .unwrap();
    let response = client.complete(&request).await.unwrap();
    assert!(response.message().is_none());
    let calls = response.tool_calls().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name(), "lookup");
    assert_eq!(calls[0].args(), Some("{\"q\":\"rust\"}"));
}

#[tokio::test]
async fn responses_tool_arguments_are_not_duplicated_after_reasoning() {
    for output_index in [0, 1] {
        let (server, client) = client(true).await;
        server.reset().await;
        let function = json!({"type": "function_call", "id": "fc-test", "call_id": "call-test",
            "name": "lookup", "arguments": "{\"q\":\"rust\"}", "status": "completed"});
        let reasoning = json!({"type": "reasoning", "id": "rs-test", "summary": []});
        let mut events = vec![];
        let mut output = vec![];
        if output_index == 1 {
            events.push(
                json!({"type": "response.output_item.added", "output_index": 0, "item": reasoning}),
            );
            output.push(reasoning);
        }
        let mut added = function.clone();
        added["arguments"] = "".into();
        added["status"] = "in_progress".into();
        events.extend([
            json!({"type": "response.output_item.added", "output_index": output_index, "item": added}),
            json!({"type": "response.function_call_arguments.delta", "output_index": output_index,
                "item_id": "fc-test", "delta": "{\"q\":"}),
            json!({"type": "response.function_call_arguments.delta", "output_index": output_index,
                "item_id": "fc-test", "delta": "\"rust\"}"}),
            json!({"type": "response.function_call_arguments.done", "output_index": output_index,
                "item_id": "fc-test", "name": "lookup", "arguments": "{\"q\":\"rust\"}"}),
            json!({"type": "response.output_item.done", "output_index": output_index, "item": function}),
        ]);
        output.push(function);
        events.push(json!({"type": "response.completed", "response": {
            "id": "resp-test", "object": "response", "created_at": 1, "model": "test-model",
            "status": "completed", "output": output
        }}));
        let mut body = String::new();
        for (sequence, mut event) in events.into_iter().enumerate() {
            event["sequence_number"] = sequence.into();
            write!(body, "data: {event}\n\n").unwrap();
        }
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;
        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("lookup rust".into())])
            .build()
            .unwrap();
        let result = client.complete(&request).await.unwrap();
        assert!(result.message().is_none());
        let calls = result.tool_calls().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name(), "lookup");
        assert_eq!(
            calls[0].args(),
            Some("{\"q\":\"rust\"}"),
            "output index {output_index}"
        );
    }
}
