//! Synthetic Bifrost-shaped fixtures; no live gateway or provider calls.

use std::{fmt::Write as _, time::Duration};

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::chat_completion::{
    ChatCompletion as _, ChatCompletionRequest, ChatMessage, ToolOutput, errors::LanguageModelError,
};
use test_case::test_case;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

use super::{OpenAI, OpenAIConfig};

fn completion(message: &Value) -> Value {
    json!({
        "id": "synthetic", "object": "chat.completion", "created": 1,
        "model": "mistral/mistral-large-4",
        "choices": [{"index": 0, "message": message, "finish_reason": "tool_calls"}],
        "usage": {"prompt_tokens": 20, "completion_tokens": 10, "total_tokens": 30,
            "prompt_tokens_details": {"cached_tokens": 8},
            "completion_tokens_details": {"reasoning_tokens": 6}}
    })
}

fn stream_body(deltas: impl IntoIterator<Item = Value>) -> String {
    let mut sse = String::new();
    for delta in deltas {
        let mut frame = completion(&json!({}));
        frame["object"] = json!("chat.completion.chunk");
        frame["choices"] = json!([{"index": 0, "delta": delta}]);
        frame.as_object_mut().unwrap().remove("usage");
        write!(sse, "data: {frame}\n\n").unwrap();
    }
    let mut usage = completion(&json!({}));
    usage["object"] = json!("chat.completion.chunk");
    usage["choices"] = json!([]);
    write!(sse, "data: {usage}\n\ndata: [DONE]\n\n").unwrap();
    sse
}

fn client(server: &MockServer, stream_full: bool) -> OpenAI {
    OpenAI::builder()
        .client(async_openai::Client::with_config(
            OpenAIConfig::new()
                .with_api_key("test-key")
                .with_api_base(server.uri()),
        ))
        .default_prompt_model("mistral/mistral-large-4")
        .stream_full(stream_full)
        .build()
        .unwrap()
}

#[test_case(None; "unspecified closed")]
#[test_case(Some(false); "open thinking")]
#[test_case(Some(true); "closed thinking")]
#[tokio::test]
async fn gateway_completion_keeps_reasoning_separate_and_replays_tool_history(
    closed: Option<bool>,
) {
    let server = MockServer::start().await;
    let mut detail = json!({"index": 0, "type": "reasoning.text", "text": "Compute 17 × 23.",
        "id": "thought-1", "signature": "opaque-signature", "format": "gateway-v1"});
    if let Some(closed) = closed {
        detail["closed"] = json!(closed);
    }
    let message = json!({"role": "assistant", "content": "391",
        "reasoning": "Compute 17 × 23.", "reasoning_content": "Compute 17 × 23.",
        "reasoning_details": [detail],
        "tool_calls": [{"id": "echo00001", "type": "function",
            "function": {"name": "echo", "arguments": "{\"value\":391}"}}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion(&message)))
        .expect(2)
        .mount(&server)
        .await;
    let client = client(&server, true);
    let mut messages = vec![ChatMessage::new_user("Calculate using echo")];
    let response = client
        .complete(&ChatCompletionRequest::from(messages.clone()))
        .await
        .unwrap();
    assert_eq!(response.message(), Some("391"));
    let reasoning = response
        .reasoning
        .as_ref()
        .expect("gateway reasoning retained");
    assert_eq!(reasoning.len(), 1);
    assert_eq!(reasoning[0].content, Some(vec!["Compute 17 × 23.".into()]));
    assert_eq!(response.usage.as_ref().unwrap().total_tokens, 30);
    let details = response.usage.as_ref().unwrap().details.as_ref().unwrap();
    assert_eq!(
        details
            .prompt_tokens_details
            .as_ref()
            .unwrap()
            .cached_tokens,
        Some(8)
    );
    assert_eq!(
        details
            .completion_tokens_details
            .as_ref()
            .unwrap()
            .reasoning_tokens,
        Some(6)
    );
    // Persistence must retain the replay metadata, not just in-memory response state.
    let saved = serde_json::to_value(reasoning).unwrap();
    let restored: Vec<swiftide_core::chat_completion::ReasoningItem> =
        serde_json::from_value(saved).unwrap();
    messages.extend(restored.into_iter().map(ChatMessage::new_reasoning));
    messages.push(ChatMessage::new_assistant(
        response.message.clone(),
        response.tool_calls.clone(),
    ));
    let call = response.tool_calls.unwrap().remove(0);
    messages.push(ChatMessage::ToolOutput(call, ToolOutput::text("391")));
    client
        .complete(&ChatCompletionRequest::from(messages))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let replay: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(replay["messages"][1], message);
    assert_eq!(replay["messages"][2]["tool_call_id"], "echo00001");
}

#[test_case(false; "delta snapshots")]
#[test_case(true; "full snapshots")]
#[tokio::test]
async fn gateway_stream_accumulates_reasoning_details_and_replays_them(stream_full: bool) {
    let server = MockServer::start().await;
    let deltas = [
        json!({"reasoning": "Compute ", "reasoning_content": "Compute ",
            "reasoning_details": [{"index": 0, "id": "r0", "type": "reasoning.text",
                "text": "Compute ", "closed": false}]}),
        json!({"reasoning": "17 × 23.", "reasoning_content": "17 × 23.", "content": "391",
            "reasoning_details": [{"index": 0, "type": "reasoning.text", "text": "17 × 23.", "closed": true}],
            "tool_calls": [{"index": 0, "id": "echo00001", "type": "function",
                "function": {"name": "echo", "arguments": "{\"value\":"}}]}),
        json!({"reasoning_details": [
            {"index": 0, "type": "reasoning.text", "signature": "sig"},
            {"index": 1, "type": "reasoning.encrypted", "data": "enc", "id": "r1"}],
            "tool_calls": [{"index": 0, "function": {"arguments": "391}"}}]}),
        json!({"reasoning_details": [
            {"index": 0, "type": "reasoning.text", "signature": "nature"},
            {"index": 1, "type": "reasoning.encrypted", "data": "rypted"}]}),
    ];
    let sse = stream_body(deltas);
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server, stream_full);
    let request = ChatCompletionRequest::from(vec![ChatMessage::new_user("Calculate")]);
    let snapshots: Vec<_> = client
        .complete_stream(&request)
        .await
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap();
    let final_response = snapshots.last().unwrap();
    let reasoning = final_response
        .reasoning
        .as_ref()
        .expect("stream reasoning retained");
    assert_eq!(reasoning[0].content, Some(vec!["Compute 17 × 23.".into()]));
    assert_eq!(
        snapshots[1].reasoning.as_ref().unwrap()[0].content,
        Some(vec![
            if stream_full {
                "Compute 17 × 23."
            } else {
                "17 × 23."
            }
            .into()
        ])
    );
    assert_eq!(final_response.message(), Some("391"));
    assert!(final_response.delta.is_none());
    assert_eq!(
        final_response.tool_calls.as_ref().unwrap()[0].args(),
        Some("{\"value\":391}")
    );
    assert_eq!(final_response.usage.as_ref().unwrap().total_tokens, 30);
    let mut messages: Vec<_> = reasoning
        .iter()
        .cloned()
        .map(ChatMessage::new_reasoning)
        .collect();
    messages.push(ChatMessage::new_assistant(
        final_response.message.clone(),
        final_response.tool_calls.clone(),
    ));
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completion(&json!({"role": "assistant", "content": "done"}))),
        )
        .mount(&server)
        .await;
    client
        .complete(&ChatCompletionRequest::from(messages))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let replay: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        replay["messages"][0]["reasoning_details"],
        json!([
            {"index": 0, "id": "r0", "type": "reasoning.text", "text": "Compute 17 × 23.", "closed": true, "signature": "signature"},
            {"index": 1, "id": "r1", "type": "reasoning.encrypted", "data": "encrypted"}
        ])
    );
    assert_eq!(
        replay["messages"][0]["reasoning_content"],
        "Compute 17 × 23."
    );
    assert_eq!(replay["messages"][0]["reasoning"], "Compute 17 × 23.");
}

#[test_case(false; "completion")]
#[test_case(true; "stream")]
#[tokio::test]
async fn invalid_reasoning_schema_is_permanent_and_accounts_for_usage_once(streaming: bool) {
    let server = MockServer::start().await;
    let message =
        json!({"role": "assistant", "content": null, "reasoning_details": "not an array"});
    let mut response = completion(&message);
    let template = if streaming {
        response["object"] = json!("chat.completion.chunk");
        response["choices"] = json!([{"index": 0, "delta": message}]);
        ResponseTemplate::new(200).set_body_raw(
            format!("data: {response}\n\ndata: [DONE]\n\n"),
            "text/event-stream",
        )
    } else {
        ResponseTemplate::new(200).set_body_json(response)
    };
    Mock::given(method("POST"))
        .respond_with(template)
        .expect(1)
        .mount(&server)
        .await;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let client = OpenAI::builder()
        .client(async_openai::Client::with_config(
            OpenAIConfig::new()
                .with_api_key("test-key")
                .with_api_base(server.uri()),
        ))
        .default_prompt_model("mistral/mistral-large-4")
        .on_usage(move |usage| {
            sender.send(usage.clone())?;
            Ok(())
        })
        .build()
        .unwrap();
    let request = ChatCompletionRequest::from(vec![ChatMessage::new_user("hello")]);
    let error = if streaming {
        let mut stream = client.complete_stream(&request).await;
        let error = stream.next().await.unwrap().unwrap_err();
        assert!(stream.next().await.is_none());
        error
    } else {
        client.complete(&request).await.unwrap_err()
    };
    assert!(matches!(error, LanguageModelError::PermanentError(_)));
    drop(client);
    let usage = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(usage.total_tokens, 30);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), receiver.recv())
            .await
            .unwrap()
            .is_none()
    );
}
