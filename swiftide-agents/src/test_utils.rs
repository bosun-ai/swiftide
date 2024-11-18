use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::chat_completion::{JsonSpec, ToolOutput};

use indoc::indoc;
use swiftide_core::{AgentContext, Tool};

#[macro_export]
macro_rules! chat_request {
    ($($message:expr),+; tools = [$($tool:expr),*]) => {
        ChatCompletionRequest::builder()
            .messages(vec![$($message),*])
            .tools_spec(
                vec![$(Box::new($tool) as Box<dyn Tool>),*]
                    .into_iter()
                    .chain(Agent::<DefaultContext>::default_tools())
                    .map(|tool| tool.json_spec())
                    .collect::<HashSet<_>>(),
            )
            .build()
            .unwrap()
    };
}

#[macro_export]
macro_rules! user {
    ($message:expr) => {
        ChatMessage::User($message.to_string())
    };
}

#[macro_export]
macro_rules! assistant {
    ($message:expr) => {
        ChatMessage::Assistant($message.to_string())
    };
}

#[macro_export]
macro_rules! tool_output {
    ($tool_name:expr, $message:expr) => {{
        ChatMessage::ToolOutput(
            ToolCall::builder()
                .name($tool_name)
                .id("1")
                .build()
                .unwrap(),
            ToolOutput::Text($message.to_string()),
        )
    }};
}

#[macro_export]
macro_rules! chat_response {
    ($message:expr; tool_calls = [$($tool_name:expr),*]) => {{

        let tool_calls = vec![
            $(ToolCall::builder().name($tool_name).id("1").build().unwrap()),*
        ];

        ChatCompletionResponse::builder()
            .message($message)
            .tool_calls(tool_calls)
            .build()
            .unwrap()
    }};
}

type Expectations = Arc<Mutex<Vec<(ToolOutput, Option<&'static str>)>>>;

#[derive(Debug, Clone)]
pub struct MockTool {
    expectations: Expectations,
}

impl MockTool {
    pub fn new() -> Self {
        Self {
            expectations: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn expect_invoke(&self, expected_result: ToolOutput, expected_args: Option<&'static str>) {
        self.expectations
            .lock()
            .unwrap()
            .push((expected_result, expected_args));
    }
}

#[async_trait]
impl Tool for MockTool {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput> {
        let expectation = self
            .expectations
            .lock()
            .unwrap()
            .pop()
            .expect("Unexpected tool call");

        assert_eq!(expectation.1, raw_args);

        Ok(expectation.0)
    }

    fn name(&self) -> &'static str {
        "mock_tool"
    }

    fn json_spec(&self) -> JsonSpec {
        indoc! {r#"
           {
               "name": "mock_tool",
               "description": "A fake tool for testing purposes",
           } 
        "#}
    }
}

impl Drop for MockTool {
    fn drop(&mut self) {
        // Mock still borrowed elsewhere and expectations still be invoked
        if Arc::strong_count(&self.expectations) > 1 {
            return;
        }
        if self.expectations.lock().unwrap().is_empty() {
            tracing::debug!("[MockTool] All expectations were met");
        } else {
            panic!(
                "[MockTool] Not all expectations were met: {:?}",
                *self.expectations.lock().unwrap()
            );
        }
    }
}
