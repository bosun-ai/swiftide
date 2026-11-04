//! Direct-provider contract tests: no Bifrost envelopes, model prefixes, or normalization.
//! Fixtures are synthetic, based on <https://docs.mistral.ai/studio/conversations/function-calling>
//! and <https://docs.mistral.ai/studio/conversations/reasoning>, not recorded live responses.

use std::{fmt::Write as _, time::Duration};

use futures_util::StreamExt as _;
use serde::Deserialize;
use serde_json::{Value, json};
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatMessage, ToolOutput, ToolSpec, Usage,
    errors::LanguageModelError,
};
use test_case::test_case;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::{Mistral, OpenAIConfig, Options, ReasoningEffort};

fn direct_client(server: &MockServer, reasoning: bool) -> (Mistral, UnboundedReceiver<Usage>) {
    let config = OpenAIConfig::new()
        .with_api_base(format!("{}/v1", server.uri()))
        .with_api_key("mistral-test-key");
    let mut options = Options::builder();
    options.max_tokens(4096).random_seed(42);
    options.extra_body(serde_json::Map::from_iter([(
        "prompt_cache_key".into(),
        json!("session-native-test"),
    )]));
    if reasoning {
        options.reasoning_effort(ReasoningEffort::High);
    }
    let (sender, receiver) = unbounded_channel();
    let client = Mistral::builder()
        .client(async_openai::Client::with_config(config))
        .default_prompt_model(if reasoning {
            "mistral-large-4"
        } else {
            "mistral-small-latest"
        })
        .default_options(options)
        .on_usage(move |usage| {
            sender.send(usage.clone())?;
            Ok(())
        })
        .build()
        .unwrap();
    (client, receiver)
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct EchoArgs {
    text: String,
}

pub(super) fn tool_request() -> ChatCompletionRequest<'static> {
    let spec = ToolSpec::builder()
        .name("echo")
        .description("Echo the supplied text")
        .parameters_schema(schemars::schema_for!(EchoArgs))
        .build()
        .unwrap();
    ChatCompletionRequest::builder()
        .messages(vec![ChatMessage::new_user("Echo bonjour using the tool.")])
        .tool_specs([spec])
        .build()
        .unwrap()
}

fn assert_native_request(body: &Value, reasoning: bool) {
    assert_eq!(
        body["model"],
        if reasoning {
            "mistral-large-4"
        } else {
            "mistral-small-latest"
        }
    );
    assert_eq!(body["max_tokens"], 4096);
    assert_eq!(body["random_seed"], 42);
    assert_eq!(body["prompt_cache_key"], "session-native-test");
    assert!(body.get("seed").is_none());
    assert!(body.get("user").is_none());
    assert!(body.get("max_completion_tokens").is_none());
    if reasoning {
        assert_eq!(body["reasoning_effort"], "high");
    } else {
        assert!(body.get("reasoning_effort").is_none());
    }
    for message in body["messages"].as_array().unwrap() {
        assert!(message.get("reasoning_content").is_none());
    }
}

#[tokio::test]
async fn direct_text_and_tool_history_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer mistral-test-key"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_native_request(&body, false);
            let messages = body["messages"].as_array().unwrap();
            let (message, finish_reason) = if messages.len() == 1 {
                assert_eq!(body["tools"][0]["function"]["name"], "echo");
                (
                    json!({
                        "role": "assistant", "content": null,
                        "tool_calls": [{"id": "call12345", "type": "function", "function": {
                            "name": "echo", "arguments": "{\"text\":\"bonjour\"}"
                        }}]
                    }),
                    "tool_calls",
                )
            } else {
                assert_eq!(messages.len(), 3);
                assert_eq!(messages[1]["role"], "assistant");
                assert_eq!(messages[1]["tool_calls"][0]["id"], "call12345");
                assert_eq!(
                    messages[1]["tool_calls"][0]["function"]["arguments"],
                    "{\"text\":\"bonjour\"}"
                );
                assert_eq!(
                    messages[2],
                    json!({"role": "tool", "tool_call_id": "call12345", "content": "bonjour"})
                );
                (json!({"role": "assistant", "content": "bonjour"}), "stop")
            };
            ResponseTemplate::new(200).set_body_json(json!({
                "id": "direct-mistral", "object": "chat.completion", "created": 123,
                "model": "mistral-small-latest",
                "choices": [{"index": 0, "message": message, "finish_reason": finish_reason}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12}
            }))
        })
        .expect(2)
        .mount(&server)
        .await;

    let (client, mut usage) = direct_client(&server, false);
    let mut request = tool_request();
    let response = client.complete(&request).await.unwrap();
    assert!(response.message.is_none());
    assert!(response.reasoning.is_none());
    let calls = response.tool_calls.unwrap();
    assert_eq!(calls.len(), 1);
    let call = calls[0].clone();
    assert_eq!(call.name(), "echo");
    let args: EchoArgs = serde_json::from_str(call.args().unwrap()).unwrap();
    // Execute the fixture's echo tool, then replay the exact ID and arguments through Swiftide.
    let output = ToolOutput::text(args.text);
    request
        .messages
        .to_mut()
        .push(ChatMessage::new_assistant(None::<String>, Some(calls)));
    request
        .messages
        .to_mut()
        .push(ChatMessage::ToolOutput(call, output));
    let response = client.complete(&request).await.unwrap();
    assert_eq!(response.message(), Some("bonjour"));
    drop(client);
    let mut accounted = Vec::new();
    while let Some(usage) = usage.recv().await {
        accounted.push(usage.total_tokens);
    }
    assert_eq!(accounted, [12, 12]);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[test_case(false; "ordinary")]
#[test_case(true; "streaming")]
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn direct_high_reasoning_and_tools_replay_with_usage(streaming: bool) {
    let server = MockServer::start().await;
    // Unlike the old Bifrost fixture, native Mistral includes the nested thinking text.
    let content = json!([
        {"type": "thinking", "thinking": [{"type": "text", "text": "Synthetic reasoning, not user-visible."}], "closed": true},
        {"type": "text", "text": "I will echo the text."}
    ]);
    let mut body = json!({
        "id": "direct-mistral-reasoning", "created": 123, "model": "mistral-large-4",
        "object": if streaming { "chat.completion.chunk" } else { "chat.completion" },
        "choices": [{"index": 0, "finish_reason": "tool_calls"}],
        "usage": {"prompt_tokens": 128, "completion_tokens": 5, "total_tokens": 133,
            "prompt_tokens_details": {"cached_tokens": 64},
            "completion_tokens_details": {"reasoning_tokens": 3}}
    });
    body["choices"][0][if streaming { "delta" } else { "message" }] = json!({
        "role": "assistant", "content": content.clone(),
        "tool_calls": [{"index": 0, "id": "call12345", "type": "function", "function": {
            "name": "echo", "arguments": "{\"text\":\"bonjour\"}"
        }}]
    });
    let template = if streaming {
        ResponseTemplate::new(200).set_body_raw(
            format!("data: {body}\n\ndata: [DONE]\n\n"),
            "text/event-stream",
        )
    } else {
        ResponseTemplate::new(200).set_body_json(body)
    };
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer mistral-test-key"))
        .respond_with(move |request: &Request| {
            let request: Value = serde_json::from_slice(&request.body).unwrap();
            if request["messages"].as_array().unwrap().len() == 1 {
                template.clone()
            } else {
                assert_native_request(&request, true);
                assert_eq!(request["messages"].as_array().unwrap().len(), 3);
                assert_eq!(request["messages"][1]["content"], content);
                assert_eq!(request["messages"][1]["tool_calls"][0]["id"], "call12345");
                assert_eq!(request["messages"][2]["content"], "bonjour");
                ResponseTemplate::new(200).set_body_json(json!({
                    "choices": [{"message": {"content": "bonjour"}}],
                    "usage": {"prompt_tokens": 128, "completion_tokens": 2, "total_tokens": 130}
                }))
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let (client, mut usage) = direct_client(&server, true);
    let mut request = tool_request();
    let response = if streaming {
        let mut stream = client.complete_stream(&request).await;
        let mut last = None;
        while let Some(response) = stream.next().await {
            last = Some(response.unwrap());
        }
        last.unwrap()
    } else {
        client.complete(&request).await.unwrap()
    };
    assert_eq!(response.message(), Some("I will echo the text."));
    let reasoning = response.reasoning.unwrap();
    assert_eq!(
        reasoning[0].content.as_deref(),
        Some(["Synthetic reasoning, not user-visible.".to_owned()].as_slice())
    );
    request
        .messages
        .to_mut()
        .extend(reasoning.into_iter().map(ChatMessage::new_reasoning));
    let calls = response.tool_calls.unwrap();
    let args: EchoArgs = serde_json::from_str(calls[0].args().unwrap()).unwrap();
    request.messages.to_mut().push(ChatMessage::new_assistant(
        response.message,
        Some(calls.clone()),
    ));
    request
        .messages
        .to_mut()
        .push(ChatMessage::new_tool_output(calls[0].clone(), args.text));
    let final_response = client.complete(&request).await.unwrap();
    assert_eq!(final_response.message(), Some("bonjour"));
    let usage = tokio::time::timeout(Duration::from_secs(1), usage.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(usage.total_tokens, 133);
    assert_eq!(
        usage
            .details
            .as_ref()
            .unwrap()
            .prompt_tokens_details
            .as_ref()
            .unwrap()
            .cached_tokens,
        Some(64)
    );
    assert_eq!(
        usage
            .details
            .as_ref()
            .unwrap()
            .completion_tokens_details
            .as_ref()
            .unwrap()
            .reasoning_tokens,
        Some(3)
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let request: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_native_request(&request, true);
    if streaming {
        assert_eq!(request["stream_options"]["include_usage"], true);
    }
}

#[tokio::test]
async fn direct_stream_reassembles_tool_arguments_and_retains_usage() {
    let server = MockServer::start().await;
    let deltas = [
        json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "call12345", "type": "function", "function": {"name": "echo", "arguments": "{\"text\":"}}]}),
        json!({"tool_calls": [{"index": 0, "function": {"arguments": "\"bonjour\"}"}}]}),
    ];
    let mut sse = String::new();
    for delta in deltas {
        let frame = json!({
            "id": "native-stream", "object": "chat.completion.chunk", "created": 123,
            "model": "mistral-small-latest",
            "choices": [{"index": 0, "delta": delta, "finish_reason": null}]
        });
        writeln!(sse, "data: {frame}\n").unwrap();
    }
    let final_frame = json!({
        "id": "native-stream", "object": "chat.completion.chunk", "created": 123,
        "model": "mistral-small-latest",
        "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12}
    });
    writeln!(sse, "data: {final_frame}\n\ndata: [DONE]\n").unwrap();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer mistral-test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(sse, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = direct_client(&server, false);
    let request = tool_request();
    let responses: Vec<_> = client.complete_stream(&request).await.collect().await;
    assert!(responses.iter().all(Result::is_ok));
    let response = responses.last().unwrap().as_ref().unwrap();
    assert!(response.message.is_none());
    let calls = response.tool_calls.as_ref().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id(), "call12345");
    assert_eq!(calls[0].name(), "echo");
    assert_eq!(calls[0].args(), Some("{\"text\":\"bonjour\"}"));
    assert_eq!(response.usage.as_ref().unwrap().total_tokens, 12);
    drop(client);
    assert_eq!(usage.recv().await.unwrap().total_tokens, 12);
    assert!(usage.recv().await.is_none());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_native_request(&body, false);
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
}

#[test_case(true; "accumulated")]
#[test_case(false; "delta only")]
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn direct_reasoning_stream_transitions_and_replays(stream_full: bool) {
    let server = MockServer::start().await;
    let mut sse = String::new();
    let deltas = [
        json!({"role": "assistant"}),
        json!({"content": [{"type": "thinking", "thinking": [
            {"type": "text", "text": "Plan "}, {"type": "text", "text": "et"}
        ], "closed": false}]}),
        json!({"content": [
            {"type": "thinking", "thinking": [{"type": "text", "text": " voilà"}], "closed": true},
            {"type": "text", "text": "bon"}, {"type": "text", "text": "j"}
        ]}),
        json!({"content": "our"}),
    ];
    for delta in deltas {
        writeln!(sse, "data: {}\n", json!({"choices": [{"delta": delta}]})).unwrap();
    }
    let usage_frame = json!({"choices": [], "usage": {
        "prompt_tokens": 128, "completion_tokens": 10, "total_tokens": 138,
        "prompt_tokens_details": {"cached_tokens": 64},
        "completion_tokens_details": {"reasoning_tokens": 6}
    }});
    // Mistral may report cumulative totals more than once; they are not deltas.
    writeln!(
        sse,
        "data: {usage_frame}\n\ndata: {usage_frame}\n\ndata: [DONE]\n"
    )
    .unwrap();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_native_request(&body, true);
            if body["stream"] == true {
                ResponseTemplate::new(200).set_body_raw(sse.clone(), "text/event-stream")
            } else {
                assert_eq!(body["messages"][1], json!({
                    "role": "assistant", "content": [
                        {"type": "thinking", "thinking": [
                            {"type": "text", "text": "Plan "}, {"type": "text", "text": "et voilà"}
                        ], "closed": true},
                        {"type": "text", "text": "bonjour"}
                    ]
                }));
                ResponseTemplate::new(200).set_body_json(json!({"choices": [{"message": {"content": "done"}}]}))
            }
        })
        .expect(2).mount(&server).await;
    let (mut client, mut usage) = direct_client(&server, true);
    client.stream_full = stream_full;
    let mut request = ChatCompletionRequest::from(vec![ChatMessage::new_user("Greet me")]);
    let mut stream = client.complete_stream(&request).await;
    let mut snapshots = Vec::new();
    while let Some(snapshot) = stream.next().await {
        snapshots.push(snapshot.unwrap());
    }
    drop(stream);
    assert_eq!(snapshots.len(), 7);
    assert!(snapshots[1].message.is_none());
    assert_eq!(
        snapshots[1].reasoning.as_ref().unwrap()[0]
            .content
            .as_ref()
            .unwrap(),
        &["Plan ", "et"]
    );
    assert_eq!(
        snapshots[2]
            .delta
            .as_ref()
            .unwrap()
            .message_chunk
            .as_deref(),
        Some("bonj")
    );
    assert_eq!(
        snapshots[3]
            .delta
            .as_ref()
            .unwrap()
            .message_chunk
            .as_deref(),
        Some("our")
    );
    assert!(snapshots[4].delta.as_ref().unwrap().message_chunk.is_none());
    if !stream_full {
        assert!(
            snapshots[..6]
                .iter()
                .all(|snapshot| snapshot.message.is_none())
        );
        assert_eq!(
            snapshots[2].reasoning.as_ref().unwrap()[0]
                .content
                .as_ref()
                .unwrap(),
            &[" voilà"]
        );
    }
    let response = snapshots.pop().unwrap();
    assert_eq!(response.message(), Some("bonjour"));
    assert!(response.delta.is_none());
    assert_eq!(response.usage.as_ref().unwrap().total_tokens, 138);
    request.messages.to_mut().extend(
        response
            .reasoning
            .unwrap()
            .into_iter()
            .map(ChatMessage::new_reasoning),
    );
    request
        .messages
        .to_mut()
        .push(ChatMessage::new_assistant(response.message, None));
    request
        .messages
        .to_mut()
        .push(ChatMessage::new_user("Continue"));
    assert_eq!(
        client.complete(&request).await.unwrap().message(),
        Some("done")
    );
    drop(client);
    let accounted = usage.recv().await.unwrap();
    assert_eq!(accounted.total_tokens, 138);
    assert_eq!(
        accounted
            .details
            .unwrap()
            .prompt_tokens_details
            .unwrap()
            .cached_tokens,
        Some(64)
    );
    assert!(usage.recv().await.is_none());
}

#[test_case(false; "ordinary")]
#[test_case(true; "streaming")]
#[tokio::test]
async fn native_unknown_content_is_permanent_and_accounted_once(streaming: bool) {
    let server = MockServer::start().await;
    let mut body = json!({"choices": [{}], "usage": {
        "prompt_tokens": 128, "completion_tokens": 10, "total_tokens": 138
    }});
    body["choices"][0][if streaming { "delta" } else { "message" }] =
        json!({"content": [{"type": "unknown"}]});
    let response = if streaming {
        let previous = json!({"choices": [], "usage": {"prompt_tokens": 128, "completion_tokens": 5, "total_tokens": 133}});
        ResponseTemplate::new(200).set_body_raw(
            format!("data: {previous}\n\ndata: {body}\n\ndata: [DONE]\n\n"),
            "text/event-stream",
        )
    } else {
        ResponseTemplate::new(200).set_body_json(body)
    };
    Mock::given(method("POST"))
        .respond_with(response)
        .expect(1)
        .mount(&server)
        .await;
    let (client, mut usage) = direct_client(&server, true);
    let request = tool_request();
    let error = if streaming {
        let mut stream = client.complete_stream(&request).await;
        assert!(stream.next().await.unwrap().is_ok());
        let error = stream.next().await.unwrap().unwrap_err();
        assert!(stream.next().await.is_none());
        error
    } else {
        client.complete(&request).await.unwrap_err()
    };
    assert!(matches!(error, LanguageModelError::PermanentError(_)));
    drop(client);
    assert_eq!(usage.recv().await.unwrap().total_tokens, 138);
    assert!(usage.recv().await.is_none());
}

#[tokio::test]
async fn native_simple_and_structured_prompts_keep_reasoning_out_of_answers() {
    use swiftide_core::{DynStructuredPrompt as _, SimplePrompt as _, prompt::Prompt};
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_native_request(&body, true);
            let text = if body.get("response_format").is_some() {
                assert_eq!(body["response_format"]["type"], "json_schema");
                r#"{"text":"bonjour"}"#
            } else {
                "bonjour"
            };
            ResponseTemplate::new(200).set_body_json(json!({"choices": [{"message": {"content": [
                {"type": "thinking", "thinking": [{"type": "text", "text": "Not the answer"}]},
                {"type": "text", "text": text}
            ]}}]}))
        })
        .expect(2)
        .mount(&server)
        .await;
    let (client, _) = direct_client(&server, true);
    assert_eq!(
        client.prompt(Prompt::from("Greet me")).await.unwrap(),
        "bonjour"
    );
    assert_eq!(
        client
            .structured_prompt_dyn(
                Prompt::from("Greet me as JSON"),
                schemars::schema_for!(EchoArgs)
            )
            .await
            .unwrap(),
        json!({"text": "bonjour"})
    );
}

#[tokio::test]
async fn native_rejects_encrypted_history_before_sending() {
    use swiftide_core::chat_completion::ReasoningItem;
    let server = MockServer::start().await;
    let (client, _) = direct_client(&server, true);
    let request = ChatCompletionRequest::from(vec![ChatMessage::new_reasoning(ReasoningItem {
        encrypted_content: Some("opaque".into()),
        ..Default::default()
    })]);
    assert!(matches!(
        client.complete(&request).await,
        Err(LanguageModelError::PermanentError(_))
    ));
    let mut stream = client.complete_stream(&request).await;
    assert!(matches!(
        stream.next().await.unwrap(),
        Err(LanguageModelError::PermanentError(_))
    ));
    assert!(stream.next().await.is_none());
    assert!(server.received_requests().await.unwrap().is_empty());
}
