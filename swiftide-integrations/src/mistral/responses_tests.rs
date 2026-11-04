//! Synthetic contract fixtures based on <https://docs.mistral.ai/openapi.yaml> (Responses preview).
//! No live provider calls. The replay test was first run against the unchanged `OpenAI` client.

use std::fmt::Write as _;

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::chat_completion::{
    ChatCompletion, ChatMessage, ToolOutput, Usage, errors::LanguageModelError,
};
use test_case::test_case;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::{OpenAIConfig, ReasoningEffort, tests::tool_request};

fn client(
    server: &MockServer,
    stream_full: bool,
    bifrost: bool,
) -> (super::Mistral, UnboundedReceiver<Usage>) {
    let config = OpenAIConfig::new()
        .with_api_base(format!("{}/v1", server.uri()))
        .with_api_key("fixture-key");
    let (sender, receiver) = unbounded_channel();
    let client = super::Mistral::builder()
        .client(async_openai::Client::with_config(config))
        .use_responses_api(true)
        .stream_full(stream_full)
        .chat_format(if bifrost {
            super::ChatFormat::Bifrost
        } else {
            super::ChatFormat::Native
        })
        .default_prompt_model(if bifrost {
            "mistral/mistral-large-4"
        } else {
            "mistral-large-4"
        })
        .default_options(super::Options::builder().reasoning_effort(ReasoningEffort::High))
        .on_usage(move |usage| {
            sender.send(usage.clone())?;
            Ok(())
        })
        .build()
        .unwrap();
    (client, receiver)
}

fn message(text: &str) -> Value {
    json!({"type": "message", "id": "msg_fixture", "role": "assistant", "status": "completed",
        "content": [{"type": "output_text", "text": text, "annotations": []}]})
}

fn sse(events: Vec<Value>) -> ResponseTemplate {
    let mut frames = String::new();
    for (index, mut event) in events.into_iter().enumerate() {
        event["sequence_number"] = json!(index);
        write!(
            frames,
            "event: {}\ndata: {event}\n\n",
            event["type"].as_str().unwrap()
        )
        .unwrap();
    }
    frames.push_str("data: [DONE]\n\n");
    ResponseTemplate::new(200).set_body_raw(frames, "text/event-stream")
}

fn streamed_completion() -> Vec<Value> {
    let mut initial = completion(json!([]));
    initial["status"] = json!("in_progress");
    initial["usage"] = Value::Null;
    initial["created_at"] = json!(123.75); // Mistral's OpenAPI type is number, not integer.
    let mut second_reasoning = reasoning();
    second_reasoning["id"] = json!("rs_second");
    second_reasoning["content"][0]["text"] = json!("Second reasoning block.");
    let mut second_call = function_call();
    second_call["id"] = json!("fc_second");
    second_call["call_id"] = json!("call_second");
    let output = json!([
        reasoning(),
        message("bonjour"),
        function_call(),
        second_reasoning,
        second_call
    ]);
    vec![
        json!({"type": "response.created", "response": initial}),
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "reasoning", "id": "rs_fixture", "status": "in_progress", "summary": [], "content": []}}),
        json!({"type": "response.content_part.added", "output_index": 0, "item_id": "rs_fixture", "content_index": 0,
            "part": {"type": "reasoning_text", "text": ""}}),
        json!({"type": "response.reasoning_text.delta", "output_index": 0, "item_id": "rs_fixture", "content_index": 0, "delta": "Synthetic private "}),
        json!({"type": "response.reasoning_text.delta", "output_index": 0, "item_id": "rs_fixture", "content_index": 0, "delta": "reasoning."}),
        json!({"type": "response.reasoning_text.done", "output_index": 0, "item_id": "rs_fixture", "content_index": 0, "text": "Synthetic private reasoning."}),
        json!({"type": "response.content_part.done", "output_index": 0, "item_id": "rs_fixture", "content_index": 0,
            "part": {"type": "reasoning_text", "text": "Synthetic private reasoning."}}),
        json!({"type": "response.output_item.done", "output_index": 0, "item": reasoning()}),
        json!({"type": "response.output_item.added", "output_index": 1,
            "item": {"type": "message", "id": "msg_fixture", "role": "assistant", "status": "in_progress", "content": []}}),
        json!({"type": "response.content_part.added", "output_index": 1, "item_id": "msg_fixture", "content_index": 0,
            "part": {"type": "output_text", "text": "", "annotations": []}}),
        json!({"type": "response.output_text.delta", "output_index": 1, "item_id": "msg_fixture", "content_index": 0, "delta": "bon"}),
        json!({"type": "response.output_text.delta", "output_index": 1, "item_id": "msg_fixture", "content_index": 0, "delta": "jour"}),
        json!({"type": "response.output_text.done", "output_index": 1, "item_id": "msg_fixture", "content_index": 0, "text": "bonjour"}),
        json!({"type": "response.output_item.done", "output_index": 1, "item": message("bonjour")}),
        json!({"type": "response.output_item.added", "output_index": 2,
            "item": {"type": "function_call", "id": "fc_fixture", "call_id": "call_fixture", "name": "echo", "arguments": "", "status": "in_progress"}}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 2, "item_id": "fc_fixture", "delta": "{\"text\":"}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 2, "item_id": "fc_fixture", "delta": "\"bonjour\"}"}),
        json!({"type": "response.function_call_arguments.done", "output_index": 2, "item_id": "fc_fixture", "name": "echo", "arguments": "{\"text\":\"bonjour\"}"}),
        json!({"type": "response.output_item.done", "output_index": 2, "item": function_call()}),
        json!({"type": "response.output_item.added", "output_index": 3, "item": output[3]}),
        json!({"type": "response.output_item.done", "output_index": 3, "item": output[3]}),
        json!({"type": "response.output_item.added", "output_index": 4, "item": output[4]}),
        json!({"type": "response.output_item.done", "output_index": 4, "item": output[4]}),
        json!({"type": "response.completed", "response": completion(output)}),
    ]
}

fn completion(output: Value) -> Value {
    let mut response = json!({
        "id": "resp_fixture", "object": "response", "created_at": 123,
        "model": "mistral-large-4", "status": "completed",
        "parallel_tool_calls": true, "tool_choice": "auto", "tools": [],
        "usage": {"input_tokens": 128, "input_tokens_details": {"cached_tokens": 64},
            "output_tokens": 5, "output_tokens_details": {"reasoning_tokens": 3}, "total_tokens": 133}
    });
    response["output"] = output;
    response
}

fn reasoning() -> Value {
    json!({"type": "reasoning", "id": "rs_fixture", "summary": [], "status": "completed",
        "content": [{"type": "reasoning_text", "text": "Synthetic private reasoning."}]})
}

fn function_call() -> Value {
    json!({"type": "function_call", "id": "fc_fixture", "call_id": "call_fixture",
        "name": "echo", "arguments": "{\"text\":\"bonjour\"}", "status": "completed"})
}

#[tokio::test]
async fn responses_plaintext_reasoning_survives_persisted_tool_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(completion(json!([reasoning(), function_call()]))),
        )
        .expect(2)
        .mount(&server)
        .await;
    let config = OpenAIConfig::new()
        .with_api_base(format!("{}/v1", server.uri()))
        .with_api_key("fixture-key");
    let client = super::Mistral::builder()
        .client(async_openai::Client::with_config(config))
        .use_responses_api(true)
        .default_options(
            super::Options::builder()
                .prompt_model("mistral-large-4")
                .reasoning_effort(ReasoningEffort::High),
        )
        .build()
        .unwrap();
    let mut request = tool_request();
    let response = client.complete(&request).await.unwrap();
    assert!(response.message.is_none());
    let items = response.reasoning.unwrap();
    assert_eq!(
        items[0].content.as_ref().unwrap(),
        &["Synthetic private reasoning."]
    );
    let calls = response.tool_calls.unwrap();
    let call = calls[0].clone();
    request
        .messages
        .to_mut()
        .extend(items.into_iter().map(ChatMessage::Reasoning));
    request
        .messages
        .to_mut()
        .push(ChatMessage::new_assistant(None::<String>, Some(calls)));
    request
        .messages
        .to_mut()
        .push(ChatMessage::ToolOutput(call, ToolOutput::text("bonjour")));
    // Simulate storing and restoring an agent's history between turns.
    request.messages = serde_json::from_value::<Vec<ChatMessage>>(
        serde_json::to_value(request.messages()).unwrap(),
    )
    .unwrap()
    .into();
    client.complete(&request).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let input = body["input"].as_array().unwrap();
    assert_eq!(
        input.iter().find(|item| item["type"] == "reasoning"),
        Some(&reasoning())
    );
    assert_eq!(input.last().unwrap()["call_id"], "call_fixture");
}

#[test_case(true, false; "native_full")]
#[test_case(false, false; "native_deltas")]
#[test_case(true, true; "gateway_full")]
#[test_case(false, true; "gateway_deltas")]
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn responses_stream_and_replay(stream_full: bool, bifrost: bool) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(|req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            if body["stream"] == true {
                sse(streamed_completion())
            } else {
                ResponseTemplate::new(200).set_body_json(completion(json!([message("done")])))
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let (mut client, mut usage) = client(&server, stream_full, bifrost);
    let mut request = tool_request();
    let mut stream = client.complete_stream(&request).await;
    let mut snapshots = Vec::new();
    while let Some(response) = stream.next().await {
        snapshots.push(response.unwrap());
    }
    drop(stream);
    let last = snapshots.last().unwrap();
    assert_eq!(last.message(), Some("bonjour"));
    assert!(last.delta.is_none());
    assert!(snapshots.iter().all(|snapshot| snapshot.id == last.id));
    let visible: String = snapshots
        .iter()
        .filter_map(|r| r.delta.as_ref()?.message_chunk.as_deref())
        .collect();
    assert_eq!(
        visible, "bonjour",
        "control/tool events must not repeat the previous text delta"
    );
    assert_eq!(last.reasoning.as_ref().unwrap().len(), 2);
    assert_eq!(last.tool_calls.as_ref().unwrap().len(), 2);
    assert_eq!(last.usage.as_ref().unwrap().total_tokens, 133);
    let details = last.usage.as_ref().unwrap().normalized().details.unwrap();
    assert_eq!(details.input.cached_tokens, Some(64));
    assert_eq!(details.output.reasoning_tokens, Some(3));
    if !stream_full {
        let reasoning: String = snapshots[..snapshots.len() - 1]
            .iter()
            .flat_map(|s| s.reasoning.iter().flatten())
            .flat_map(|item| item.content.iter().flatten())
            .cloned()
            .collect();
        assert_eq!(reasoning, "Synthetic private reasoning.");
        assert!(
            snapshots[..snapshots.len() - 1]
                .iter()
                .all(|s| s.message.is_none() && s.tool_calls.is_none())
        );
    }
    request.messages.to_mut().extend(
        last.reasoning
            .clone()
            .unwrap()
            .into_iter()
            .map(ChatMessage::Reasoning),
    );
    let calls = last.tool_calls.clone().unwrap();
    request.messages.to_mut().push(ChatMessage::new_assistant(
        last.message.clone(),
        Some(calls.clone()),
    ));
    for call in calls {
        request
            .messages
            .to_mut()
            .push(ChatMessage::new_tool_output(call, "bonjour"));
    }
    request.messages = serde_json::from_value::<Vec<ChatMessage>>(
        serde_json::to_value(request.messages()).unwrap(),
    )
    .unwrap()
    .into();
    // Reasoning replay must not depend on the current turn's effort setting.
    client.options_mut().reasoning_effort = None;
    assert_eq!(
        client.complete(&request).await.unwrap().message(),
        Some("done")
    );
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(body["input"][1], reasoning());
    assert_eq!(body["input"][2]["id"], "rs_second");
    assert_eq!(body["input"][4]["call_id"], "call_fixture");
    assert_eq!(body["input"][5]["call_id"], "call_second");
    assert_eq!(body["input"][6]["call_id"], "call_fixture");
    assert_eq!(body["input"][7]["call_id"], "call_second");
    assert_eq!(body["store"], false);
    assert!(body.get("include").is_none());
    assert!(body.get("reasoning").is_none());
    assert_eq!(
        body["model"],
        if bifrost {
            "mistral/mistral-large-4"
        } else {
            "mistral-large-4"
        }
    );
    drop(client);
    let mut totals = Vec::new();
    while let Some(usage) = usage.recv().await {
        totals.push(usage.total_tokens);
    }
    assert_eq!(totals, [133, 133]);
}

#[test_case(false, "incomplete", true; "ordinary_incomplete")]
#[test_case(true, "incomplete", true; "stream_incomplete")]
#[test_case(false, "invalid_prompt", true; "ordinary_invalid_prompt")]
#[test_case(true, "invalid_prompt", true; "stream_invalid_prompt")]
#[test_case(false, "invalid_image", true; "ordinary_invalid_image")]
#[test_case(false, "server_error", false; "ordinary_server_error")]
#[test_case(true, "rate_limit_exceeded", false; "stream_rate_limit")]
#[test_case(false, "unknown_output", true; "ordinary_unknown_output")]
#[test_case(true, "unknown_output", true; "stream_unknown_output")]
#[test_case(true, "bad_event", true; "stream_bad_event")]
#[test_case(true, "early_eof", false; "stream_early_eof")]
#[test_case(true, "error_event", false; "stream_error_event")]
#[tokio::test]
async fn responses_errors_account_once(streaming: bool, failure: &str, permanent: bool) {
    let server = MockServer::start().await;
    let mut response = completion(json!([message("partial")]));
    let kind = match failure {
        "incomplete" => {
            response["status"] = json!("incomplete");
            response["incomplete_details"] = json!({"reason": "max_output_tokens"});
            "response.incomplete"
        }
        "unknown_output" => {
            response["output"] = json!([{"type": "unsupported_output"}]);
            "response.completed"
        }
        "bad_event" | "early_eof" | "error_event" => "response.in_progress",
        code => {
            response["status"] = json!("failed");
            response["error"] = json!({"code": code, "message": "fixture failure"});
            "response.failed"
        }
    };
    let template = if streaming {
        let mut earlier = completion(json!([]));
        earlier["status"] = json!("in_progress");
        earlier["usage"]["output_tokens"] = json!(0);
        earlier["usage"]["total_tokens"] = json!(128);
        let mut events = vec![
            json!({"type": "response.created", "response": earlier}),
            json!({"type": kind, "response": response}),
        ];
        if failure == "bad_event" {
            events.push(json!({"type": "response.output_text.delta", "item_id": "missing", "output_index": 0, "content_index": 0, "delta": "bad"}));
        }
        if failure == "error_event" {
            events.push(
                json!({"type": "error", "code": "server_error", "message": "fixture failure"}),
            );
        }
        sse(events)
    } else {
        ResponseTemplate::new(200).set_body_json(response)
    };
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(template)
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server, true, false);
    let request = tool_request();
    let error = if streaming {
        let results: Vec<_> = client.complete_stream(&request).await.collect().await;
        assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
        results.into_iter().last().unwrap().unwrap_err()
    } else {
        client.complete(&request).await.unwrap_err()
    };
    assert_eq!(
        matches!(error, LanguageModelError::PermanentError(_)),
        permanent,
        "{error:?}"
    );
    drop(client);
    assert_eq!(usage.recv().await.unwrap().total_tokens, 133);
    assert!(usage.recv().await.is_none());
}

#[tokio::test]
async fn responses_request_options_and_multimodal_inputs() {
    use swiftide_core::chat_completion::{
        ChatMessageContentPart as Part, ChatMessageContentSource as Source,
    };
    let server = MockServer::start().await;
    let (mut client, _) = client(&server, true, false);
    client.options_mut().max_tokens = Some(4096);
    client.options_mut().temperature = Some(0.2);
    client.options_mut().top_p = Some(0.9);
    client.options_mut().parallel_tool_calls = Some(false);
    client
        .options_mut()
        .extra_body
        .insert("prompt_cache_key".into(), json!("cache-fixture"));
    let mut request = tool_request();
    request
        .messages
        .to_mut()
        .push(ChatMessage::UserWithParts(vec![
            Part::Image {
                source: Source::Url {
                    url: "https://example.test/image.png".into(),
                },
                format: None,
            },
            Part::Document {
                source: Source::Bytes {
                    data: b"fixture".to_vec(),
                    media_type: Some("application/pdf".into()),
                },
                name: Some("fixture.pdf".into()),
                format: Some("pdf".into()),
            },
        ]));
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion(json!([message("ok")]))))
        .expect(1)
        .mount(&server)
        .await;
    client.complete(&request).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["max_output_tokens"], 4096);
    assert_eq!(body["parallel_tool_calls"], false);
    assert_eq!(body["prompt_cache_key"], "cache-fixture");
    assert_eq!(body["reasoning"], json!({"effort": "high"}));
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["name"], "echo");
    assert_eq!(body["input"][1]["content"][0]["type"], "input_image");
    assert_eq!(body["input"][1]["content"][1]["type"], "input_file");
    assert_eq!(body["input"][1]["content"][1]["file_data"], "Zml4dHVyZQ==");
    assert_eq!(body["input"][1]["content"][1]["filename"], "fixture.pdf");
    for forbidden in [
        "stream_options",
        "max_tokens",
        "random_seed",
        "messages",
        "include",
    ] {
        assert!(body.get(forbidden).is_none());
    }
}

#[test_case("store", json!(true); "storage")]
#[test_case("background", json!(true); "background")]
#[test_case("previous_response_id", json!("resp_previous"); "server_state")]
#[test_case("truncation", json!("auto"); "truncation")]
#[test_case("stream", json!(true); "wrong_stream_mode")]
#[tokio::test]
async fn responses_reject_unsupported_state_overrides(key: &str, value: Value) {
    let server = MockServer::start().await;
    let (mut client, _) = client(&server, true, false);
    client.options_mut().extra_body.insert(key.into(), value);
    assert!(matches!(
        client.complete(&tool_request()).await,
        Err(LanguageModelError::PermanentError(_))
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn responses_simple_and_structured_prompts() {
    use swiftide_core::{DynStructuredPrompt as _, SimplePrompt as _, prompt::Prompt};
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(|req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            assert!(body.get("response_format").is_none());
            assert_eq!(body["store"], false);
            let text = if body.get("text").is_some() {
                assert_eq!(body["text"]["format"]["type"], "json_schema");
                assert_eq!(body["text"]["format"]["name"], "structured_prompt");
                assert_eq!(body["text"]["format"]["schema"]["type"], "string");
                "\"bonjour\""
            } else {
                "bonjour"
            };
            let mut response = completion(json!([reasoning(), message(text)]));
            response["created_at"] = json!(123.5);
            ResponseTemplate::new(200).set_body_json(response)
        })
        .expect(2)
        .mount(&server)
        .await;
    let (mut client, _) = client(&server, true, false);
    assert_eq!(
        client.prompt(Prompt::from("Greet me")).await.unwrap(),
        "bonjour"
    );
    // A per-request schema takes precedence over an extra-body text default.
    client
        .options_mut()
        .extra_body
        .insert("text".into(), json!({"format": {"type": "text"}}));
    assert_eq!(
        client
            .structured_prompt_dyn(
                Prompt::from("Greet me as JSON"),
                schemars::schema_for!(String)
            )
            .await
            .unwrap(),
        json!("bonjour")
    );
}

#[tokio::test]
async fn responses_malformed_stream_accounts_previous_usage() {
    let server = MockServer::start().await;
    let mut initial = completion(json!([]));
    initial["status"] = json!("in_progress");
    let initial =
        json!({"type": "response.in_progress", "sequence_number": 0, "response": initial});
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!("data: {initial}\n\ndata: {{malformed\n\ndata: [DONE]\n\n"),
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = client(&server, false, false);
    let results: Vec<_> = client
        .complete_stream(&tool_request())
        .await
        .collect()
        .await;
    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert!(matches!(
        &results[1],
        Err(LanguageModelError::TransientError(_))
    ));
    drop(client);
    assert_eq!(usage.recv().await.unwrap().total_tokens, 133);
    assert!(usage.recv().await.is_none());
}

#[tokio::test]
async fn responses_refusals_are_visible_and_reasoning_is_separate() {
    let server = MockServer::start().await;
    let mut refusal = message("");
    refusal["content"] = json!([{"type": "refusal", "refusal": "I cannot help with that."}]);
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completion(json!([reasoning(), refusal]))),
        )
        .mount(&server)
        .await;
    let (client, _) = client(&server, false, false);
    let response = client.complete(&tool_request()).await.unwrap();
    assert_eq!(response.message(), Some("I cannot help with that."));
    assert_eq!(
        response.reasoning.unwrap()[0].id.as_deref(),
        Some("rs_fixture")
    );
}

#[tokio::test]
async fn responses_reject_chat_only_options_before_sending() {
    let server = MockServer::start().await;
    let (mut client, _) = client(&server, false, false);
    client.options_mut().random_seed = Some(42);
    assert!(matches!(
        client.complete(&tool_request()).await,
        Err(LanguageModelError::PermanentError(_))
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}
