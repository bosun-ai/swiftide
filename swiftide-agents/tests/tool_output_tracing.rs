//! Tool outputs are traced as they end up in the history, after `AfterTool` hooks have run.
//!
//! Lives in its own test binary because tracing caches callsite interest globally; a capturing
//! subscriber in the shared lib test binary would race with other tests.
use std::sync::{Arc, Mutex};

use serde::ser::Error as _;
use swiftide_agents::test_utils::MockTool;
use swiftide_agents::{Agent, chat_request, chat_response, user};
use swiftide_core::chat_completion::errors::ToolError;
use swiftide_core::chat_completion::{ChatCompletionResponse, Tool as _, ToolCall, ToolOutput};
use swiftide_core::test_utils::MockChatCompletion;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::{Context, SubscriberExt as _};
use tracing_subscriber::registry::LookupSpan;

/// Captures the outputs recorded as tracing fields within `tool` spans
#[derive(Clone, Default)]
struct TracedToolOutputs(Arc<Mutex<Vec<String>>>);

impl TracedToolOutputs {
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

impl<S> tracing_subscriber::Layer<S> for TracedToolOutputs
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        struct OutputField(Option<String>);
        impl Visit for OutputField {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if matches!(field.name(), "output" | "langfuse.output") {
                    self.0 = Some(format!("{value:?}"));
                }
            }
        }

        if ctx
            .event_span(event)
            .is_none_or(|span| span.name() != "tool")
        {
            return;
        }
        let mut visitor = OutputField(None);
        event.record(&mut visitor);
        if let Some(output) = visitor.0 {
            self.0.lock().unwrap().push(output);
        }
    }
}

fn agent_calling(
    mock_tool: MockTool,
    after_tool: Option<fn(&mut Result<ToolOutput, ToolError>)>,
) -> Agent {
    let mock_llm = MockChatCompletion::new();
    mock_llm.expect_complete(
        chat_request! { user!("Hello"); tools = [mock_tool.clone()] },
        Ok(chat_response! { "Calling"; tool_calls = ["mock_tool"] }),
    );

    let mut builder = Agent::builder();
    builder.tools([mock_tool]).llm(&mock_llm).no_system_prompt();
    if let Some(after_tool) = after_tool {
        builder.after_tool(
            move |_: &mut Agent, _: &ToolCall, output: &mut Result<ToolOutput, ToolError>| {
                after_tool(output);
                Box::pin(async { Ok(()) })
            },
        );
    }
    builder.build().unwrap()
}

#[tokio::test]
async fn traces_tool_output_as_it_ends_up_in_history() {
    let traced = TracedToolOutputs::default();
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(traced.clone()))
        .unwrap();

    // An AfterTool hook truncates the output; only the truncated output is traced
    let mock_tool = MockTool::default();
    mock_tool.expect_invoke_ok("huge raw output".into(), None);
    agent_calling(mock_tool, Some(|output| *output = Ok("truncated".into())))
        .query_once("Hello")
        .await
        .unwrap();
    assert_eq!(traced.take(), ["truncated"]);

    // A failed tool call is traced as the failure the LLM gets to see
    let mock_tool = MockTool::default();
    mock_tool.expect_invoke(
        Err(ToolError::WrongArguments(serde_json::Error::custom(
            "missing",
        ))),
        None,
    );
    agent_calling(mock_tool, None)
        .query_once("Hello")
        .await
        .unwrap();
    assert_eq!(
        traced.take(),
        ["Tool call failed: arguments for tool failed to parse: missing"]
    );
}
