use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use swiftide_core::chat_completion::{errors::ToolError, Tool, ToolOutput, ToolSpec};

use swiftide_core::AgentContext;

use crate::hooks::{
    AfterCompletionFn, AfterToolFn, BeforeAllFn, BeforeCompletionFn, BeforeToolFn, MessageHookFn,
    OnStartFn, OnStopFn,
};
use crate::Agent;

#[macro_export]
macro_rules! chat_request {
    ($($message:expr),+; tools = [$($tool:expr),*]) => {
        ChatCompletionRequest::builder()
            .messages(vec![$($message),*])
            .tools_spec(
                vec![$(Box::new($tool) as Box<dyn Tool>),*]
                    .into_iter()
                    .chain(Agent::default_tools())
                    .map(|tool| tool.tool_spec())
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
macro_rules! system {
    ($message:expr) => {
        ChatMessage::System($message.to_string())
    };
}

#[macro_export]
macro_rules! summary {
    ($message:expr) => {
        ChatMessage::Summary($message.to_string())
    };
}

#[macro_export]
macro_rules! assistant {
    ($message:expr) => {
        ChatMessage::Assistant(Some($message.to_string()), None)
    };
    ($message:expr, [$($tool_call_name:expr),*]) => {{
        let tool_calls = vec![
            $(
            ToolCall::builder()
                .name($tool_call_name)
                .id("1")
                .build()
                .unwrap()
            ),*
        ];

        ChatMessage::Assistant(Some($message.to_string()), Some(tool_calls))
    }};
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
            $message.into(),
        )
    }};
}

#[macro_export]
macro_rules! tool_failed {
    ($tool_name:expr, $message:expr) => {{
        ChatMessage::ToolOutput(
            ToolCall::builder()
                .name($tool_name)
                .id("1")
                .build()
                .unwrap(),
            ToolOutput::Fail($message.to_string()),
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

type Expectations = Arc<Mutex<Vec<(Result<ToolOutput, ToolError>, Option<&'static str>)>>>;

#[derive(Debug, Clone)]
pub struct MockTool {
    expectations: Expectations,
    name: &'static str,
}

impl MockTool {
    pub fn default() -> Self {
        Self::new("mock_tool")
    }
    pub fn new(name: &'static str) -> Self {
        Self {
            expectations: Arc::new(Mutex::new(Vec::new())),
            name,
        }
    }
    pub fn expect_invoke_ok(
        &self,
        expected_result: ToolOutput,
        expected_args: Option<&'static str>,
    ) {
        self.expect_invoke(Ok(expected_result), expected_args);
    }

    pub fn expect_invoke(
        &self,
        expected_result: Result<ToolOutput, ToolError>,
        expected_args: Option<&'static str>,
    ) {
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
    ) -> std::result::Result<ToolOutput, ToolError> {
        tracing::debug!(
            "[MockTool] Invoked `{}` with args: {:?}",
            self.name,
            raw_args
        );
        let expectation = self
            .expectations
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| panic!("[MockTool] No expectations left for `{}`", self.name));

        assert_eq!(expectation.1, raw_args);

        expectation.0
    }

    fn name(&self) -> Cow<'_, str> {
        self.name.into()
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name(self.name().as_ref())
            .description("A fake tool for testing purposes")
            .build()
            .unwrap()
    }
}

impl From<MockTool> for Box<dyn Tool> {
    fn from(val: MockTool) -> Self {
        Box::new(val) as Box<dyn Tool>
    }
}

impl Drop for MockTool {
    fn drop(&mut self) {
        // Mock still borrowed elsewhere and expectations still be invoked
        if Arc::strong_count(&self.expectations) > 1 {
            return;
        }
        if self.expectations.lock().is_err() {
            return;
        }

        let name = self.name;
        if self.expectations.lock().unwrap().is_empty() {
            tracing::debug!("[MockTool] All expectations were met for `{name}`");
        } else {
            panic!(
                "[MockTool] Not all expectations were met for `{name}: {:?}",
                *self.expectations.lock().unwrap()
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockHook {
    name: &'static str,
    called: Arc<Mutex<usize>>,
    expected_calls: usize,
}

impl MockHook {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            called: Arc::new(Mutex::new(0)),
            expected_calls: 0,
        }
    }

    pub fn expect_calls(&mut self, expected_calls: usize) -> &mut Self {
        self.expected_calls = expected_calls;
        self
    }

    pub fn hook_fn(&self) -> impl BeforeAllFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }

    pub fn on_start_fn(&self) -> impl OnStartFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }
    pub fn before_completion_fn(&self) -> impl BeforeCompletionFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent, _| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }

    pub fn after_completion_fn(&self) -> impl AfterCompletionFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent, _| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }

    pub fn after_tool_fn(&self) -> impl AfterToolFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent, _, _| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }

    pub fn before_tool_fn(&self) -> impl BeforeToolFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent, _| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }

    pub fn message_hook_fn(&self) -> impl MessageHookFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent, _| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }

    pub fn stop_hook_fn(&self) -> impl OnStopFn {
        let called = Arc::clone(&self.called);
        move |_: &Agent, _, _| {
            let called = Arc::clone(&called);
            Box::pin(async move {
                let mut called = called.lock().unwrap();
                *called += 1;
                Ok(())
            })
        }
    }
}

impl Drop for MockHook {
    fn drop(&mut self) {
        if Arc::strong_count(&self.called) > 1 {
            return;
        }
        let Ok(called) = self.called.lock() else {
            return;
        };

        if *called == self.expected_calls {
            tracing::debug!(
                "[MockHook] `{}` all expectations met; called {} times",
                self.name,
                *called
            );
        } else {
            panic!(
                "[MockHook] `{}` was called {} times but expected {}",
                self.name, *called, self.expected_calls
            )
        }
    }
}
