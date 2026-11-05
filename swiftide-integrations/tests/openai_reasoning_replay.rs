#![cfg(feature = "openai")]

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ToolOutput,
    ToolSpec,
};
use swiftide_integrations::openai::{OpenAI, OpenAIConfig, Options, ReasoningEffort};
use test_case::test_case;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

#[derive(schemars::JsonSchema)]
struct EchoArgs {}

async fn complete(
    client: &OpenAI,
    request: &ChatCompletionRequest<'_>,
    stream_full: Option<bool>,
) -> ChatCompletionResponse {
    if stream_full.is_none() {
        return client.complete(request).await.unwrap();
    }
    let mut stream = client.complete_stream(request).await;
    let mut last = None;
    while let Some(response) = stream.next().await {
        last = Some(response.unwrap());
    }
    last.expect("stream completed")
}

fn response(output: &Value) -> Value {
    json!({
        "id": "resp_fixture", "object": "response", "created_at": 123,
        "model": "provider/plaintext-fixture", "status": "completed",
        "parallel_tool_calls": false, "tool_choice": "auto", "tools": [],
        "output": output,
        "usage": {"input_tokens": 10, "input_tokens_details": {"cached_tokens": 0},
                  "output_tokens": 5, "output_tokens_details": {"reasoning_tokens": 3},
                  "total_tokens": 15}
    })
}

#[test_case(None; "without_encrypted_content")]
#[test_case(Some(""); "with_empty_encrypted_content")]
#[test_case(Some("opaque-fixture"); "alongside_encrypted_content")]
#[tokio::test]
async fn plaintext_reasoning_survives_tool_rounds(encrypted: Option<&str>) {
    for stream_full in [None, Some(true), Some(false)] {
        for next_effort in [
            None,
            Some(ReasoningEffort::Low),
            Some(ReasoningEffort::High),
            Some(ReasoningEffort::None),
        ] {
            replay_tool_round(encrypted, stream_full, next_effort).await;
        }
    }
}

fn reasoning_items(encrypted: Option<&str>) -> Vec<Value> {
    ["rs_first", "rs_second"]
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let mut item = json!({
                "type": "reasoning", "id": id,
                "summary": [{"type": "summary_text", "text": format!("Summary {index}")}],
                "status": if index == 0 { "completed" } else { "incomplete" },
                "content": [{"type": "reasoning_text", "text": format!("Reasoning {index}: call echo.")}]
            });
            if let Some(encrypted) = encrypted {
                item["encrypted_content"] = json!(encrypted);
            }
            item
        })
        .collect()
}

async fn mock_server(reasoning: &[Value]) -> MockServer {
    let server = MockServer::start().await;
    let call = json!({"type": "function_call", "id": "fc_fixture", "call_id": "fc_call",
                      "name": "echo", "arguments": "{}", "status": "completed"});
    let first = response(&json!([reasoning[0], reasoning[1], call]));
    let second = response(&json!([{
        "type": "message", "id": "msg_fixture", "role": "assistant", "status": "completed",
        "content": [{"type": "output_text", "text": "ok", "annotations": []}]
    }]));
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let is_second = body["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "function_call_output");
            let result = if is_second { &second } else { &first };
            if body["stream"] == true {
                let completed = json!({"type": "response.completed", "sequence_number": 0,
                                       "response": result});
                ResponseTemplate::new(200).set_body_raw(
                    format!("data: {completed}\n\ndata: [DONE]\n\n"),
                    "text/event-stream",
                )
            } else {
                ResponseTemplate::new(200).set_body_json(result)
            }
        })
        .expect(2)
        .mount(&server)
        .await;

    server
}

async fn replay_tool_round(
    encrypted: Option<&str>,
    stream_full: Option<bool>,
    next_effort: Option<ReasoningEffort>,
) {
    let reasoning = reasoning_items(encrypted);
    let server = mock_server(&reasoning).await;

    let mut client = OpenAI::builder()
        .client(async_openai::Client::with_config(
            OpenAIConfig::new()
                .with_api_base(server.uri())
                .with_api_key("fixture-key"),
        ))
        .use_responses_api(true)
        .stream_full(stream_full.unwrap_or(true))
        .default_options(
            Options::builder()
                .prompt_model("provider/plaintext-fixture")
                .reasoning_effort(ReasoningEffort::High)
                .reasoning_features(false)
                .replay_plaintext_reasoning(true),
        )
        .build()
        .unwrap();
    let tools = [ToolSpec::builder()
        .name("echo")
        .description("Return ok")
        .parameters_schema(schemars::schema_for!(EchoArgs))
        .build()
        .unwrap()];
    let request = ChatCompletionRequest::builder()
        .messages(vec![ChatMessage::new_user("Use echo.")])
        .tool_specs(tools.clone())
        .build()
        .unwrap();
    let result = complete(&client, &request, stream_full).await;
    assert!(result.message.is_none());
    let items = result.reasoning.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].content.as_ref().unwrap()[0],
        "Reasoning 0: call echo."
    );
    let calls = result.tool_calls.unwrap();
    let mut history = request.messages().to_vec();
    history.extend(items.into_iter().map(ChatMessage::Reasoning));
    history.push(ChatMessage::new_assistant(
        None::<String>,
        Some(calls.clone()),
    ));
    history.push(ChatMessage::ToolOutput(
        calls[0].clone(),
        ToolOutput::text("ok"),
    ));
    let restored: Vec<ChatMessage> =
        serde_json::from_value(serde_json::to_value(&history).unwrap()).unwrap();
    assert_eq!(restored, history);

    client.options_mut().reasoning_effort = next_effort.clone();
    let request = ChatCompletionRequest::builder()
        .messages(restored)
        .tool_specs(tools)
        .build()
        .unwrap();
    assert_eq!(
        complete(&client, &request, stream_full)
            .await
            .message
            .as_deref(),
        Some("ok")
    );
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let input = body["input"].as_array().unwrap();
    let replayed: Vec<_> = input
        .iter()
        .filter(|item| item["type"] == "reasoning")
        .cloned()
        .collect();
    assert_eq!(replayed, reasoning);
    assert_eq!(body["store"], false);
    assert_eq!(body["reasoning"]["effort"], json!(next_effort));
    assert!(body.get("include").is_none_or(Value::is_null));
    assert_eq!(input[3]["type"], "function_call");
    assert_eq!(input[3]["call_id"], "fc_call");
    assert_eq!(input[4]["type"], "function_call_output");
    assert_eq!(input[4]["call_id"], "fc_call");
    assert_eq!(input[4]["output"], "ok");
}
