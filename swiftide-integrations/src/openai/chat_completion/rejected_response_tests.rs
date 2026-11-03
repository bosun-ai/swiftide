//! Exercise rejected responses across the real HTTP / async-openai boundary in the default
//! `OpenAI` format. Native Mistral content arrays require opting into the Mistral format,
//! covered separately by `mistral_tests`.

use std::{fmt::Write as _, time::Duration};

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::chat_completion::{ChatCompletion, ChatCompletionRequest, ChatMessage, Usage};
use test_case::test_case;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::openai::OpenAI;
use swiftide_core::chat_completion::errors::LanguageModelError;

fn completion(content: Value) -> Value {
    // Shape-preserving synthetic fixture from #1212; no private provider output.
    let mut body = json!({
        "id": "synthetic-mistral-response", "object": "chat.completion",
        "created": 1_793_697_017, "model": "mistral-large-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant", "content": null, "refusal": null,
                "tool_calls": [{
                    "id": "call_example", "type": "function",
                    "function": {"name": "inspect_manifest", "arguments": "{}"}
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 12_221, "completion_tokens": 202, "total_tokens": 12_423,
            "prompt_tokens_details": {"cached_tokens": 10_000},
            "completion_tokens_details": {"reasoning_tokens": 180}
        }
    });
    body["choices"][0]["message"]["content"] = content;
    body
}

fn client(server: &MockServer) -> (OpenAI, UnboundedReceiver<Usage>) {
    let (sender, receiver) = unbounded_channel();
    let config = async_openai::config::OpenAIConfig::new()
        .with_api_key("test-key")
        .with_api_base(server.uri());
    let client = OpenAI::builder()
        .client(async_openai::Client::with_config(config))
        .default_prompt_model("mistral/mistral-large-4")
        .on_usage(move |usage| {
            sender.send(usage.clone())?;
            Ok(())
        })
        .build()
        .unwrap();
    (client, receiver)
}

async fn assert_usage_once(receiver: &mut UnboundedReceiver<Usage>, expected: &Usage) {
    let usage = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("usage callback must run")
        .unwrap();
    assert_eq!(&usage, expected);
    assert_no_usage(receiver).await;
}

async fn assert_no_usage(receiver: &mut UnboundedReceiver<Usage>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(50), receiver.recv())
            .await
            .is_err(),
        "must not invent or double-count usage"
    );
}

#[test_case(json!([{"type": "thinking"}]); "thinking only")]
#[test_case(json!([{"type": "thinking"}, {"type": "text", "text": "I will inspect the manifest."}]); "mixed")]
#[test_case(json!([{"type": "text", "text": "Hello"}]); "text array")]
#[test_case(json!([{"type": "unknown"}]); "unknown block")]
#[tokio::test]
async fn unsupported_content_stops_retries_and_reports_consumed_usage(content: Value) {
    let server = MockServer::start().await;
    let body = completion(content);
    let sdk_error =
        serde_json::from_value::<async_openai::types::chat::CreateChatCompletionResponse>(
            body.clone(),
        )
        .unwrap_err();
    assert!(sdk_error.is_data());
    let sdk_usage: async_openai::types::chat::CompletionUsage =
        serde_json::from_value(body["usage"].clone()).unwrap();
    let expected = Usage::from(&sdk_usage);
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server);
    let request = ChatCompletionRequest::from(vec![ChatMessage::User("Inspect".into())]);

    // Model the caller's bounded retry policy. Before the fix this sends three identical paid
    // requests. Swiftide itself does not retry language-model calls inside the agent loop.
    let mut last_error = None;
    for _ in 0..3 {
        let error = client.complete(&request).await.unwrap_err();
        let retryable = matches!(error, LanguageModelError::TransientError(_));
        last_error = Some(error);
        if !retryable {
            break;
        }
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    let error = last_error.unwrap();
    assert!(matches!(error, LanguageModelError::PermanentError(_)));
    assert!(error.to_string().contains("expected a string"));
    assert_usage_once(&mut usage, &expected).await;
}

#[test_case(None; "missing")]
#[test_case(Some(Value::Null); "null")]
#[test_case(Some(json!({"prompt_tokens": 1})); "incomplete")]
#[test_case(Some(json!({"prompt_tokens": -1, "completion_tokens": 2, "total_tokens": 1})); "invalid")]
#[tokio::test]
async fn rejected_responses_do_not_invent_usage(reported_usage: Option<Value>) {
    let server = MockServer::start().await;
    let mut body = completion(json!([{"type": "thinking"}]));
    body.as_object_mut().unwrap().remove("usage");
    if let Some(reported_usage) = reported_usage {
        body["usage"] = reported_usage;
    }
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server);
    let request = ChatCompletionRequest::from(vec![ChatMessage::User("Inspect".into())]);
    assert!(matches!(
        client.complete(&request).await,
        Err(LanguageModelError::PermanentError(_))
    ));
    assert_no_usage(&mut usage).await;
}

#[test_case("<html>Bad gateway</html>"; "non json")]
#[test_case(r#"{"id": [], "choices":"#; "data error before truncated tail")]
#[tokio::test]
async fn malformed_responses_remain_transient_with_a_bounded_caller_policy(body: &str) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .expect(3)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server);
    let request = ChatCompletionRequest::from(vec![ChatMessage::User("Inspect".into())]);
    for _ in 0..3 {
        assert!(matches!(
            client.complete(&request).await,
            Err(LanguageModelError::TransientError(_))
        ));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
    assert_no_usage(&mut usage).await;
}

#[test_case(Some(json!("Visible text")); "text")]
#[test_case(Some(json!("")); "empty text")]
#[test_case(Some(Value::Null); "null content")]
#[test_case(None; "absent content")]
#[tokio::test]
async fn successful_completion_is_accounted_for_once(content: Option<Value>) {
    let server = MockServer::start().await;
    let mut body = completion(content.clone().unwrap_or(Value::Null));
    if content.is_none() {
        body["choices"][0]["message"]
            .as_object_mut()
            .unwrap()
            .remove("content");
    }
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server);
    let request = ChatCompletionRequest::from(vec![ChatMessage::User("Inspect".into())]);
    let response = client.complete(&request).await.unwrap();
    assert_eq!(response.message(), content.as_ref().and_then(Value::as_str));
    let tool = &response.tool_calls.as_ref().unwrap()[0];
    assert_eq!(tool.name(), "inspect_manifest");
    assert_usage_once(&mut usage, response.usage.as_ref().unwrap()).await;
}

#[tokio::test]
async fn in_band_provider_errors_keep_their_classification_and_usage() {
    let server = MockServer::start().await;
    let body = json!({
        "error": {"message": "Service unavailable", "code": 503},
        "usage": {"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12}
    });
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server);
    let request = ChatCompletionRequest::from(vec![ChatMessage::User("Inspect".into())]);
    let error = client.complete(&request).await.unwrap_err();
    assert!(matches!(error, LanguageModelError::TransientError(_)));
    assert!(error.to_string().contains("Service unavailable"));
    assert_usage_once(
        &mut usage,
        &Usage {
            prompt_tokens: 10,
            completion_tokens: 2,
            total_tokens: 12,
            details: None,
        },
    )
    .await;
}

#[test_case(r#"{"choices":[{"delta":{"content":[]}}]}"#, true; "unsupported schema")]
#[test_case(r#"{"choices":[{"delta":{"content":[]"#, false; "truncated schema")]
#[tokio::test]
async fn failed_stream_without_usage_does_not_invent_it(body: &str, permanent: bool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!("data: {body}\n\ndata: [DONE]\n\n"),
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server);
    let request = ChatCompletionRequest::from(vec![ChatMessage::User("Inspect".into())]);
    let mut stream = client.complete_stream(&request).await;
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(
        matches!(error, LanguageModelError::PermanentError(_)),
        permanent
    );
    if !permanent {
        assert!(matches!(error, LanguageModelError::TransientError(_)));
    }
    assert!(stream.next().await.is_none());
    assert_no_usage(&mut usage).await;
}

fn chunk(delta: Value, usage: Value) -> Value {
    let mut body = json!({
        "id": "synthetic-stream", "object": "chat.completion.chunk", "created": 123,
        "model": "mistral-large-4", "usage": null,
        "choices": [{"index": 0, "delta": null, "finish_reason": null}]
    });
    body["choices"][0]["delta"] = delta;
    body["usage"] = usage;
    body
}

#[test_case(false, false; "success with repeated totals")]
#[test_case(true, false; "failure after usage")]
#[test_case(true, true; "failure carries newer usage")]
#[tokio::test]
async fn streaming_usage_is_reported_once_even_when_the_stream_fails(fail: bool, newer: bool) {
    let server = MockServer::start().await;
    let mut final_usage = completion(Value::Null)["usage"].clone();
    let earlier_usage = final_usage.clone();
    if newer {
        final_usage["completion_tokens"] = json!(203);
        final_usage["total_tokens"] = json!(12_424);
    }
    let sdk_usage: async_openai::types::chat::CompletionUsage =
        serde_json::from_value(final_usage.clone()).unwrap();
    let expected = Usage::from(&sdk_usage);
    let final_delta = if fail {
        json!({"content": [{"type": "thinking"}]})
    } else {
        json!({"content": "visible"})
    };
    let frames = [
        chunk(json!({"role": "assistant"}), earlier_usage.clone()),
        chunk(json!({}), earlier_usage),
        chunk(final_delta, if newer { final_usage } else { Value::Null }),
    ];
    let mut body = String::new();
    for frame in frames {
        writeln!(body, "data: {frame}\n").unwrap();
    }
    body.push_str("data: [DONE]\n\n");
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server);
    let request = ChatCompletionRequest::from(vec![ChatMessage::User("Inspect".into())]);
    let mut stream = client.complete_stream(&request).await;
    let mut errors = 0;
    let mut final_response = None;
    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => final_response = Some(response),
            Err(LanguageModelError::PermanentError(_)) if fail => errors += 1,
            other => panic!("unexpected stream result: {other:?}"),
        }
    }
    assert_eq!(errors, usize::from(fail));
    if !fail {
        let response = final_response.unwrap();
        assert_eq!(response.message(), Some("visible"));
        assert_eq!(response.usage.as_ref(), Some(&expected));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_usage_once(&mut usage, &expected).await;
}
