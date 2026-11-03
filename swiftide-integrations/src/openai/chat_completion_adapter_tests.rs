//! Both wire formats must use the same prompt, stream, and accounting behavior.

use std::{fmt::Write as _, time::Duration};

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::chat_completion::{
    ChatCompletion as _, ChatCompletionRequest, ChatMessage, Usage, errors::LanguageModelError,
};
use swiftide_core::{DynStructuredPrompt as _, SimplePrompt as _};
use test_case::test_case;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::{ChatCompletionFormat, OpenAI, OpenAIConfig};

fn client(server: &MockServer, format: ChatCompletionFormat) -> (OpenAI, UnboundedReceiver<Usage>) {
    let (sender, receiver) = unbounded_channel();
    let client = OpenAI::builder()
        .client(async_openai::Client::with_config(
            OpenAIConfig::new()
                .with_api_key("test-key")
                .with_api_base(server.uri()),
        ))
        .default_prompt_model("test-model")
        .chat_completion_format(format)
        .on_usage(move |usage| {
            sender.send(usage.clone())?;
            Ok(())
        })
        .build()
        .unwrap();
    (client, receiver)
}

fn usage() -> Value {
    json!({
        "prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120,
        "prompt_tokens_details": {"cached_tokens": 64},
        "completion_tokens_details": {"reasoning_tokens": 10}
    })
}

fn completion(content: &Value) -> Value {
    json!({
        "id": "completion", "object": "chat.completion", "created": 1, "model": "test-model",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"}],
        "usage": usage()
    })
}

async fn assert_usage_once(mut receiver: UnboundedReceiver<Usage>) {
    let expected =
        serde_json::from_value::<async_openai::types::chat::CompletionUsage>(usage()).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), receiver.recv())
            .await
            .unwrap()
            .unwrap(),
        Usage::from(&expected),
    );
    // Call after dropping the client: all callback senders must be gone after this one report.
    assert!(
        tokio::time::timeout(Duration::from_secs(1), receiver.recv())
            .await
            .unwrap()
            .is_none()
    );
}

#[test_case(ChatCompletionFormat::OpenAI; "openai")]
#[test_case(ChatCompletionFormat::Mistral; "mistral")]
#[tokio::test]
async fn prompt_frontends_share_options_and_keep_schema_request_local(
    format: ChatCompletionFormat,
) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completion(&json!(r#"{"answer":"bonjour"}"#))),
        )
        .expect(2)
        .mount(&server)
        .await;
    let (mut client, mut reports) = client(&server, format);
    client
        .options_mut()
        .extra_body
        .insert("prompt_cache_key".into(), json!("test-session"));
    client
        .options_mut()
        .extra_body
        .insert("response_format".into(), json!({"type": "text"}));
    let original = client.options().extra_body.clone();
    let schema = schemars::schema_for!(Value);
    let result = client
        .structured_prompt_dyn("JSON please".into(), schema.clone())
        .await
        .unwrap();
    assert_eq!(result, json!({"answer": "bonjour"}));
    assert_eq!(client.options().extra_body, original);
    assert_eq!(
        client.prompt("Text please".into()).await.unwrap(),
        r#"{"answer":"bonjour"}"#
    );

    let requests = server.received_requests().await.unwrap();
    let structured: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let simple: Value = serde_json::from_slice(&requests[1].body).unwrap();
    for body in [&structured, &simple] {
        assert_eq!(body["prompt_cache_key"], "test-session");
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["messages"][0]["role"], "user");
    }
    assert_eq!(structured["response_format"]["type"], "json_schema");
    assert_eq!(
        structured["response_format"]["json_schema"]["schema"],
        Value::from(schema)
    );
    assert_eq!(structured["response_format"]["json_schema"]["strict"], true);
    assert_eq!(simple["response_format"], json!({"type": "text"}));
    drop(client);
    // Exactly one accounting callback per request, with the complete usage breakdown.
    let first = tokio::time::timeout(Duration::from_secs(1), reports.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.total_tokens, 120);
    assert_usage_once(reports).await;
}

#[test_case(ChatCompletionFormat::OpenAI, false; "openai simple")]
#[test_case(ChatCompletionFormat::OpenAI, true; "openai structured")]
#[test_case(ChatCompletionFormat::Mistral, false; "mistral simple")]
#[test_case(ChatCompletionFormat::Mistral, true; "mistral structured")]
#[tokio::test]
async fn rejected_prompt_responses_are_permanent_and_accounted_once(
    format: ChatCompletionFormat,
    structured: bool,
) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completion(&json!([{"type": "unsupported"}]))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (client, reports) = client(&server, format);
    let error = if structured {
        client
            .structured_prompt_dyn("Hello".into(), schemars::schema_for!(Value))
            .await
            .unwrap_err()
    } else {
        client.prompt("Hello".into()).await.unwrap_err()
    };
    assert!(matches!(error, LanguageModelError::PermanentError(_)));
    drop(client);
    assert_usage_once(reports).await;
}

#[test_case(ChatCompletionFormat::OpenAI, false; "openai deltas")]
#[test_case(ChatCompletionFormat::OpenAI, true; "openai full")]
#[test_case(ChatCompletionFormat::Mistral, false; "mistral deltas")]
#[test_case(ChatCompletionFormat::Mistral, true; "mistral full")]
#[tokio::test]
async fn stream_accumulation_does_not_repeat_text_on_tool_or_usage_frames(
    format: ChatCompletionFormat,
    stream_full: bool,
) {
    let server = MockServer::start().await;
    let deltas = [
        json!({"content": "bonjour"}),
        json!({"tool_calls": [{"index": 0, "id": "call1", "type": "function",
            "function": {"name": "echo", "arguments": "{\"text\":"}}]}),
        json!({"tool_calls": [{"index": 0, "function": {"arguments": "\"bonjour\"}"}}]}),
    ];
    let mut frames: Vec<Value> = deltas.into_iter().map(|delta| json!({
        "id": "completion", "object": "chat.completion.chunk", "created": 1, "model": "test-model",
        "choices": [{"index": 0, "delta": delta, "finish_reason": null}]
    })).collect();
    let usage_frame = json!({
        "id": "completion", "object": "chat.completion.chunk", "created": 1, "model": "test-model",
        "choices": [], "usage": usage()
    });
    frames.extend([usage_frame.clone(), usage_frame]);
    let mut sse = String::new();
    for frame in frames {
        write!(sse, "data: {frame}\n\n").unwrap();
    }
    sse.push_str("data: [DONE]\n\n");
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    let (mut client, reports) = client(&server, format);
    client.stream_full = stream_full;
    let request = ChatCompletionRequest::from(vec![ChatMessage::new_user("Hello")]);
    let mut stream = client.complete_stream(&request).await;
    let mut snapshots = Vec::new();
    while let Some(item) = stream.next().await {
        snapshots.push(item.unwrap());
    }
    assert_eq!(snapshots.len(), 6);
    assert_eq!(
        snapshots[0]
            .delta
            .as_ref()
            .unwrap()
            .message_chunk
            .as_deref(),
        Some("bonjour")
    );
    for snapshot in &snapshots[1..5] {
        assert!(snapshot.delta.as_ref().unwrap().message_chunk.is_none());
        assert_eq!(
            snapshot.message.as_deref(),
            stream_full.then_some("bonjour")
        );
    }
    let final_response = snapshots.last().unwrap();
    assert!(final_response.delta.is_none());
    assert_eq!(final_response.message.as_deref(), Some("bonjour"));
    let calls = final_response.tool_calls.as_ref().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name(), "echo");
    assert_eq!(calls[0].args(), Some(r#"{"text":"bonjour"}"#));
    assert_eq!(final_response.usage.as_ref().unwrap().total_tokens, 120);
    drop(stream);
    drop(client);
    assert_usage_once(reports).await;
}

#[tokio::test]
async fn prompt_frontends_validate_the_backend_before_sending() {
    let server = MockServer::start().await;
    let (mut client, _) = client(&server, ChatCompletionFormat::Mistral);
    client.use_responses_api = true;
    assert!(matches!(
        client.prompt("Hello".into()).await,
        Err(LanguageModelError::PermanentError(_))
    ));
    assert!(matches!(
        client
            .structured_prompt_dyn("Hello".into(), schemars::schema_for!(Value))
            .await,
        Err(LanguageModelError::PermanentError(_))
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}
