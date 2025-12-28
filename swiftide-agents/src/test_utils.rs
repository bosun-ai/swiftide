use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use swiftide_core::chat_completion::ToolCall;
use swiftide_core::chat_completion::{Tool, ToolOutput, ToolSpec, errors::ToolError};

use swiftide_core::AgentContext;

use crate::Agent;
use crate::hooks::{
    AfterCompletionFn, AfterToolFn, BeforeAllFn, BeforeCompletionFn, BeforeToolFn, MessageHookFn,
    OnStartFn, OnStopFn, OnStreamFn,
};

#[macro_export]
macro_rules! chat_request {
    ($($message:expr),+; tools = [$($tool:expr),*]) => {{
        let mut builder = swiftide_core::chat_completion::ChatCompletionRequest::builder();
        builder.messages(vec![$($message),*]);

        let mut tool_specs = Vec::new();
        $(tool_specs.push({
            let tool = $tool;
            tool.tool_spec()
        });)*

        tool_specs.extend(Agent::default_tools().into_iter().map(|tool| tool.tool_spec()));

        builder.tool_specs(tool_specs);

        builder.build().unwrap()
    }};
    ($($message:expr),+; tool_specs = [$($tool:expr),*]) => {{
        let mut builder = swiftide_core::chat_completion::ChatCompletionRequest::builder();
        builder.messages(vec![$($message),*]);

        let mut tool_specs = Vec::new();
        $(tool_specs.push($tool);)*
        tool_specs.extend(Agent::default_tools().into_iter().map(|tool| tool.tool_spec()));

        builder.tool_specs(tool_specs);

        builder.build().unwrap()
    }}
}

#[macro_export]
macro_rules! user {
    ($message:expr) => {
        swiftide_core::chat_completion::ChatMessage::User($message.to_string())
    };
}

#[macro_export]
macro_rules! system {
    ($message:expr) => {
        swiftide_core::chat_completion::ChatMessage::System($message.to_string())
    };
}

#[macro_export]
macro_rules! summary {
    ($message:expr) => {
        swiftide_core::chat_completion::ChatMessage::Summary($message.to_string())
    };
}

#[macro_export]
macro_rules! assistant {
    ($message:expr) => {
        swiftide_core::chat_completion::ChatMessage::new_assistant(
            Some($message.to_string()),
            None,
        )
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

        ChatMessage::new_assistant(Some($message.to_string()), Some(tool_calls))
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
            ToolOutput::fail($message),
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
    (tool_calls = [$($tool_name:expr),*]) => {{

        let tool_calls = vec![
            $(ToolCall::builder().name($tool_name).id("1").build().unwrap()),*
        ];

        ChatCompletionResponse::builder()
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
    #[allow(clippy::should_implement_trait)]
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

    #[allow(clippy::missing_panics_doc)]
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
        tool_call: &ToolCall,
    ) -> std::result::Result<ToolOutput, ToolError> {
        tracing::debug!(
            "[MockTool] Invoked `{}` with args: {:?}",
            self.name,
            tool_call
        );
        let expectation = self
            .expectations
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| panic!("[MockTool] No expectations left for `{}`", self.name));

        assert_eq!(expectation.1, tool_call.args());

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

    #[allow(clippy::missing_panics_doc)]
    pub fn hook_fn(&self) -> impl BeforeAllFn + use<> {
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

    #[allow(clippy::missing_panics_doc)]
    pub fn on_start_fn(&self) -> impl OnStartFn + use<> {
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
    #[allow(clippy::missing_panics_doc)]
    pub fn before_completion_fn(&self) -> impl BeforeCompletionFn + use<> {
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

    #[allow(clippy::missing_panics_doc)]
    pub fn after_completion_fn(&self) -> impl AfterCompletionFn + use<> {
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

    #[allow(clippy::missing_panics_doc)]
    pub fn after_tool_fn(&self) -> impl AfterToolFn + use<> {
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

    #[allow(clippy::missing_panics_doc)]
    pub fn before_tool_fn(&self) -> impl BeforeToolFn + use<> {
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

    #[allow(clippy::missing_panics_doc)]
    pub fn message_hook_fn(&self) -> impl MessageHookFn + use<> {
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

    #[allow(clippy::missing_panics_doc)]
    pub fn stop_hook_fn(&self) -> impl OnStopFn + use<> {
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

    #[allow(clippy::missing_panics_doc)]
    pub fn on_stream_fn(&self) -> impl OnStreamFn + use<> {
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
