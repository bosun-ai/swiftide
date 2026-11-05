//! Reproduce unknown-tool recovery through the real `OpenAI` client and a local HTTP fixture.

use std::{
    borrow::Cow,
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use serde_json::{Value, json};
use swiftide_agents::Agent;
use swiftide_core::{
    AgentContext,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};
use swiftide_integrations::openai::{OpenAI, OpenAIConfig};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate, matchers};

#[derive(Clone, Debug)]
struct ShellCommand(Arc<AtomicUsize>);

#[async_trait]
impl Tool for ShellCommand {
    async fn invoke(&self, _: &dyn AgentContext, _: &ToolCall) -> Result<ToolOutput, ToolError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput::text("command output"))
    }

    fn name(&self) -> Cow<'_, str> {
        "shell_command".into()
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("shell_command")
            .description("A deterministic stand-in for command execution")
            .build()
            .unwrap()
    }
}

/// Enforce complete, ordered tool results on every request, as required by this regression.
fn validate_tool_results(messages: &[Value]) -> Result<(), String> {
    let mut pending = VecDeque::new();
    for message in messages {
        if message["role"] == "tool" {
            let received = message["tool_call_id"].as_str().unwrap();
            let expected = pending.pop_front();
            if expected != Some(received) {
                return Err(format!(
                    "Expected tool result {expected:?}, received {received}"
                ));
            }
        } else {
            if !pending.is_empty() {
                return Err(format!("Missing tool results for {pending:?}"));
            }
            if let Some(calls) = message["tool_calls"].as_array() {
                pending.extend(calls.iter().map(|call| call["id"].as_str().unwrap()));
            }
        }
    }
    if !pending.is_empty() {
        return Err(format!("Missing tool results for {pending:?}"));
    }
    Ok(())
}

struct OpenAiFixture {
    initial_tools: Vec<&'static str>,
    repeat_unknown: bool,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl Respond for OpenAiFixture {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let mut requests = self.requests.lock().unwrap();
        let round = requests.len();
        requests.push(body.clone());
        if let Err(message) = validate_tool_results(body["messages"].as_array().unwrap()) {
            return ResponseTemplate::new(400).set_body_json(json!({
                "error": {"message": message, "type": "invalid_request_error", "code": null}
            }));
        }

        let names = match round {
            0 => self.initial_tools.as_slice(),
            1 if self.repeat_unknown => self.initial_tools.as_slice(),
            1 => &["shell_command"],
            _ => &[],
        };
        let streaming = body["stream"].as_bool().unwrap_or(false);
        let calls = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let mut call = json!({
                    "id": format!("call_{round}_{index}"),
                    "type": "function",
                    "function": {"name": name, "arguments": "{}"}
                });
                if streaming {
                    call["index"] = json!(index);
                }
                call
            })
            .collect::<Vec<_>>();
        let message = if calls.is_empty() {
            json!({"role": "assistant", "content": "Recovered."})
        } else {
            json!({"role": "assistant", "tool_calls": calls})
        };
        let mut choice = json!({
            "index": 0,
            "finish_reason": if calls.is_empty() { "stop" } else { "tool_calls" }
        });
        choice[if streaming { "delta" } else { "message" }] = message;
        let response = json!({
            "id": format!("chatcmpl-{round}"), "created": 1, "model": "test-model",
            "object": if streaming { "chat.completion.chunk" } else { "chat.completion" },
            "choices": [choice]
        });
        if streaming {
            ResponseTemplate::new(200).set_body_raw(
                format!("data: {response}\n\ndata: [DONE]\n\n"),
                "text/event-stream",
            )
        } else {
            ResponseTemplate::new(200).set_body_json(response)
        }
    }
}

fn openai_agent(
    server: &MockServer,
    streaming: bool,
    retries: usize,
    invocations: Arc<AtomicUsize>,
) -> Agent {
    let config = OpenAIConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test-key");
    let openai = OpenAI::builder()
        .client(async_openai::Client::with_config(config))
        .default_prompt_model("test-model")
        .build()
        .unwrap();
    Agent::builder()
        .llm(&openai)
        .add_tool(ShellCommand(invocations))
        .streaming(streaming)
        .tool_retry_limit(retries)
        .limit(3)
        .build()
        .unwrap()
}

async fn recovers_through_openai(streaming: bool) {
    let batches = [
        vec!["bash", "shell_command"],
        vec!["bash"],
        vec!["shell_command", "bash"],
        vec!["shell_command", "bash", "shell_command"],
        vec!["bash", "unavailable", "shell_command", "unavailable"],
    ];
    for names in batches {
        let server = MockServer::start().await;
        let requests = Arc::new(Mutex::new(Vec::new()));
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/chat/completions"))
            .respond_with(OpenAiFixture {
                initial_tools: names.clone(),
                repeat_unknown: false,
                requests: requests.clone(),
            })
            .mount(&server)
            .await;
        let invocations = Arc::new(AtomicUsize::new(0));
        let mut agent = openai_agent(&server, streaming, 3, invocations.clone());

        let result = agent
            .query("Run the command, correcting any unavailable tool.")
            .await;
        let requests = requests.lock().unwrap();
        assert!(
            result.is_ok(),
            "OpenAI client rejected the next agent request: {result:?}\n{}",
            serde_json::to_string_pretty(requests.last().unwrap()).unwrap()
        );
        assert_eq!(requests.len(), 3, "the agent must recover and finish");
        assert_eq!(
            invocations.load(Ordering::SeqCst),
            names
                .iter()
                .filter(|name| **name == "shell_command")
                .count()
                + 1
        );
        let messages = requests[1]["messages"].as_array().unwrap();
        let outputs = messages
            .iter()
            .filter(|m| m["role"] == "tool")
            .collect::<Vec<_>>();
        assert_eq!(outputs.len(), names.len());
        for (index, (name, output)) in names.iter().zip(outputs).enumerate() {
            assert_eq!(output["tool_call_id"], format!("call_0_{index}"));
            if *name != "shell_command" {
                assert_eq!(
                    output["content"],
                    format!("Unknown tool '{name}'. Use an available tool.")
                );
            }
        }
    }
}

#[tokio::test]
async fn unknown_tools_recover_through_openai() {
    recovers_through_openai(false).await;
}

#[tokio::test]
async fn unknown_tools_recover_through_streaming_openai() {
    recovers_through_openai(true).await;
}

#[tokio::test]
async fn repeated_unknown_tools_stop_with_new_ids_through_openai() {
    for streaming in [false, true] {
        let server = MockServer::start().await;
        let requests = Arc::new(Mutex::new(Vec::new()));
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/chat/completions"))
            .respond_with(OpenAiFixture {
                initial_tools: vec!["bash", "shell_command"],
                repeat_unknown: true,
                requests: requests.clone(),
            })
            .mount(&server)
            .await;
        let invocations = Arc::new(AtomicUsize::new(0));
        let mut agent = openai_agent(&server, streaming, 1, invocations.clone());

        let error = agent.query("Run the command.").await.unwrap_err();

        assert!(error.to_string().contains("Unknown tool 'bash'"));
        assert_eq!(requests.lock().unwrap().len(), 2);
        assert_eq!(invocations.load(Ordering::SeqCst), 2);
        let stopped_by = agent
            .stop_reason()
            .unwrap()
            .as_tool_calls_over_limit()
            .unwrap();
        assert_eq!(stopped_by.id(), "call_1_0");
        let history = agent.history().await.unwrap();
        let outputs = history
            .iter()
            .filter_map(|message| match message {
                swiftide_core::chat_completion::ChatMessage::ToolOutput(call, _) => Some(call.id()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(outputs, ["call_0_0", "call_0_1", "call_1_0", "call_1_1"]);
    }
}
