//! Reassess these probes before replacing Mistral Responses with the upstream adapter.
//! Plaintext replay is fixed upstream; the remaining differences are independent of that fix.

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use swiftide_core::chat_completion::{ChatCompletion, ChatMessage};
use test_case::test_case;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::{client, completion, function_call, message, sse, streamed_completion, tool_request};
use crate::openai::{OpenAI, OpenAIConfig, Options, ReasoningEffort};

pub(super) fn openai_client(server: &MockServer) -> OpenAI {
    OpenAI::builder()
        .client(async_openai::Client::with_config(
            OpenAIConfig::new()
                .with_api_base(format!("{}/v1", server.uri()))
                .with_api_key("fixture-key"),
        ))
        .use_responses_api(true)
        .default_options(
            Options::builder()
                .prompt_model("mistral-large-4")
                .reasoning_effort(ReasoningEffort::High)
                .reasoning_features(false)
                .replay_plaintext_reasoning(true),
        )
        .build()
        .unwrap()
}

fn clients(server: &MockServer) -> [(bool, Box<dyn ChatCompletion>); 2] {
    [
        (true, Box::new(client(server, true, false).0)),
        (false, Box::new(openai_client(server))),
    ]
}

#[test_case(false; "ordinary")]
#[test_case(true; "streaming")]
#[tokio::test]
async fn fractional_timestamps_need_mistral_decoding(streaming: bool) {
    // Mistral's published Responses schema allows number, not just integer timestamps:
    // https://docs.mistral.ai/api/endpoint/responses
    let server = MockServer::start().await;
    let mut response = completion(json!([message("bonjour")]));
    response["created_at"] = json!(123.75);
    let template = if streaming {
        sse(vec![
            json!({"type": "response.completed", "response": response}),
        ])
    } else {
        ResponseTemplate::new(200).set_body_json(response)
    };
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(template)
        .expect(2)
        .mount(&server)
        .await;
    for (is_mistral, client) in clients(&server) {
        let request = tool_request();
        let result = if streaming {
            client.complete_stream(&request).await.next().await.unwrap()
        } else {
            client.complete(&request).await
        };
        if is_mistral {
            assert_eq!(result.unwrap().message(), Some("bonjour"));
        } else {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("floating point"), "{error}");
        }
    }
}

#[tokio::test]
async fn native_tool_call_ids_need_mistral_replay() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(completion(json!([function_call()]))),
        )
        .expect(4)
        .mount(&server)
        .await;
    for (is_mistral, client) in clients(&server) {
        let mut request = tool_request();
        let calls = client.complete(&request).await.unwrap().tool_calls.unwrap();
        assert_eq!(calls[0].id(), "call_fixture");
        request.messages.to_mut().push(ChatMessage::new_assistant(
            None::<String>,
            Some(calls.clone()),
        ));
        request
            .messages
            .to_mut()
            .push(ChatMessage::new_tool_output(calls[0].clone(), "bonjour"));
        client.complete(&request).await.unwrap();
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests.last().unwrap().body).unwrap();
        // Upstream still rewrites call_ to fc_. Native Mistral replay preserves the provider ID.
        let expected = if is_mistral {
            "call_fixture"
        } else {
            "fc_fixture"
        };
        assert_eq!(body["input"][1]["call_id"], expected);
        assert_eq!(body["input"][2]["call_id"], expected);
    }
}

#[tokio::test]
async fn incremental_reasoning_needs_mistral_streaming() {
    let server = MockServer::start().await;
    let mut events = streamed_completion();
    // Isolate the event handling difference from the timestamp decoding difference.
    events[0]["response"]["created_at"] = json!(123);
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(sse(events))
        .expect(2)
        .mount(&server)
        .await;
    for (is_mistral, client) in clients(&server) {
        let request = tool_request();
        let snapshots = client
            .complete_stream(&request)
            .await
            .collect::<Vec<_>>()
            .await;
        let snapshots = snapshots
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let (last, intermediate) = snapshots.split_last().unwrap();
        assert_eq!(last.reasoning.as_ref().unwrap().len(), 2);
        assert_eq!(last.message(), Some("bonjour"));
        // Upstream retains reasoning in the final result, but does not emit reasoning deltas.
        assert_eq!(
            intermediate
                .iter()
                .any(|snapshot| snapshot.reasoning.is_some()),
            is_mistral
        );
    }
}
