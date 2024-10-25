use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::chat_completion::{
    ChatCompletionRequest, ChatCompletionResponse, JsonSpec, ToolOutput,
};
use tracing::error;

use crate::{AgentContext, Tool};
use indoc::indoc;

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
        let Ok(expectations) = self.expectations.try_lock() else {
            return;
        };
        if !expectations.is_empty() {
            tracing::warn!("Not all expectations were met: {:?}", expectations);
        }
    }
}
