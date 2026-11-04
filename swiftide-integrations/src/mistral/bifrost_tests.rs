//! Bifrost's normalized Mistral contract; no live gateway or provider calls.
use super::{ChatFormat, Mistral, OpenAIConfig, Options, ReasoningEffort};
use futures_util::StreamExt as _;
use serde_json::{Value, json};
use std::fmt::Write as _;
use swiftide_core::chat_completion::{
    ChatCompletion, ChatCompletionRequest, ChatMessage, ReasoningStatus,
};
use test_case::test_case;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

fn detail(text: &str, closed: Option<bool>) -> Value {
    let mut detail = json!({"index": 0, "type": "reasoning.text", "text": text});
    if let Some(closed) = closed {
        detail["closed"] = json!(closed);
    }
    detail
}

#[test_case(false, None; "ordinary unspecified")]
#[test_case(false, Some(false); "ordinary open")]
#[test_case(false, Some(true); "ordinary closed")]
#[test_case(true, None; "stream unspecified")]
#[test_case(true, Some(false); "stream open")]
#[test_case(true, Some(true); "stream closed")]
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn reasoning_tools_usage_and_persisted_history(streaming: bool, closed: Option<bool>) {
    let server = MockServer::start().await;
    let message = json!({"role":"assistant", "content":"Hello",
        "reasoning":"Plan then act", "reasoning_content":"Plan then act",
        "reasoning_details":[detail("Plan then act", closed)],
        "tool_calls":[{"id":"echo00001", "type":"function", "function":{"name":"echo", "arguments":"{\"text\":\"Hello\"}"}}]
    });
    let usage = json!({"prompt_tokens":40, "completion_tokens":10, "total_tokens":50,
        "prompt_tokens_details":{"cached_tokens":30},"completion_tokens_details":{"reasoning_tokens":8}});
    let response = json!({"choices":[{"message":message.clone()}],"usage":usage.clone()});
    let mut sse = String::new();
    for delta in [
        json!({"reasoning":"Plan ","reasoning_content":"Plan ","reasoning_details":[detail("Plan ", Some(false))]}),
        json!({"reasoning":"then act","reasoning_content":"then act","reasoning_details":[detail("then act", closed)],"content":"Hello"}),
        json!({"tool_calls":[{"index":0,"id":"echo00001","type":"function","function":{"name":"echo","arguments":"{\"text\":"}}]}),
        json!({"tool_calls":[{"index":0,"function":{"arguments":"\"Hello\"}"}}]}),
    ] {
        writeln!(sse, "data: {}\n", json!({"choices":[{"delta":delta}]})).unwrap();
    }
    writeln!(
        sse,
        "data: {}\n\ndata: [DONE]\n",
        json!({"choices":[],"usage":usage})
    )
    .unwrap();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["model"], "mistral/mistral-large-4");
            assert_eq!(body["max_completion_tokens"], 4096);
            assert_eq!(body["seed"], 42);
            assert_eq!(body["reasoning_effort"], "high");
            assert_eq!(body["prompt_cache_key"], "mistral-gateway-test");
            assert!(body.get("max_tokens").is_none());
            if body["messages"].as_array().unwrap().len() > 1 {
                assert_eq!(body["messages"][1]["content"], "Hello");
                assert_eq!(body["messages"][1]["reasoning_content"], "Plan then act");
                assert_eq!(
                    body["messages"][1]["reasoning_details"],
                    json!([detail("Plan then act", closed)])
                );
                assert_eq!(body["messages"][1]["tool_calls"], message["tool_calls"]);
                assert_eq!(body["messages"][2]["role"], "tool");
                assert_eq!(body["messages"][2]["tool_call_id"], "echo00001");
                assert_eq!(body["messages"][2]["content"], "Hello");
                ResponseTemplate::new(200)
                    .set_body_json(json!({"choices":[{"message":{"content":"done"}}]}))
            } else if streaming {
                ResponseTemplate::new(200).set_body_raw(sse.clone(), "text/event-stream")
            } else {
                ResponseTemplate::new(200).set_body_json(response.clone())
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let client = Mistral::builder()
        .client(async_openai::Client::with_config(
            OpenAIConfig::new()
                .with_api_base(format!("{}/v1", server.uri()))
                .with_api_key("test-key"),
        ))
        .chat_format(ChatFormat::Bifrost)
        .stream_full(false)
        .default_prompt_model("mistral/mistral-large-4")
        .default_options(
            Options::builder()
                .max_tokens(4096)
                .random_seed(42)
                .reasoning_effort(ReasoningEffort::High)
                .extra_body(serde_json::Map::from_iter([(
                    "prompt_cache_key".into(),
                    json!("mistral-gateway-test"),
                )])),
        )
        .on_usage(move |usage| {
            tx.send(usage.clone())?;
            Ok(())
        })
        .build()
        .unwrap();
    let mut request = super::tests::tool_request();
    let response = if streaming {
        let mut stream = client.complete_stream(&request).await;
        let mut last = None;
        let mut visible = String::new();
        while let Some(response) = stream.next().await {
            let response = response.unwrap();
            if let Some(text) = response
                .delta
                .as_ref()
                .and_then(|delta| delta.message_chunk.as_deref())
            {
                visible.push_str(text);
            }
            last = Some(response);
        }
        assert_eq!(visible, "Hello");
        let final_response = last.unwrap();
        assert!(final_response.delta.is_none());
        final_response
    } else {
        client.complete(&request).await.unwrap()
    };
    assert_eq!(response.message(), Some("Hello"));
    let reasoning = response.reasoning.unwrap();
    assert_eq!(reasoning.len(), 1);
    assert_eq!(
        reasoning[0].content.as_ref().unwrap().concat(),
        "Plan then act"
    );
    assert_eq!(
        reasoning[0].status,
        closed.map(|closed| if closed {
            ReasoningStatus::Completed
        } else {
            ReasoningStatus::InProgress
        })
    );
    let usage = response.usage.unwrap();
    assert_eq!(
        usage
            .details
            .as_ref()
            .unwrap()
            .prompt_tokens_details
            .as_ref()
            .unwrap()
            .cached_tokens,
        Some(30)
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
        Some(8)
    );
    request
        .messages
        .to_mut()
        .extend(reasoning.into_iter().map(ChatMessage::new_reasoning));
    let calls = response.tool_calls.unwrap();
    request.messages.to_mut().push(ChatMessage::new_assistant(
        response.message,
        Some(calls.clone()),
    ));
    request
        .messages
        .to_mut()
        .push(ChatMessage::new_tool_output(calls[0].clone(), "Hello"));
    let history: Vec<ChatMessage> =
        serde_json::from_value(serde_json::to_value(request.messages()).unwrap()).unwrap();
    assert_eq!(
        client
            .complete(&ChatCompletionRequest::from(history))
            .await
            .unwrap()
            .message(),
        Some("done")
    );
    drop(client);
    assert_eq!(rx.recv().await.unwrap().total_tokens, 50);
    assert!(rx.recv().await.is_none());
}
