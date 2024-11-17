#![allow(clippy::missing_panics_doc)]
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::chat_completion::{ChatCompletion, ChatCompletionRequest, ChatCompletionResponse};
use anyhow::Result;
use pretty_assertions::assert_eq;

#[macro_export]
macro_rules! assert_default_prompt_snapshot {
    ($node:expr, $($key:expr => $value:expr),*) => {
        #[tokio::test]
        async fn test_default_prompt() {
        let template = default_prompt();
        let mut prompt = template.to_prompt().with_node(&Node::new($node));
        $(
            prompt = prompt.with_context_value($key, $value);
        )*
        insta::assert_snapshot!(prompt.render().await.unwrap());
        }
    };

    ($($key:expr => $value:expr),*) => {
        #[tokio::test]
        async fn test_default_prompt() {
            let template = default_prompt();
            let mut prompt = template.to_prompt();
            $(
                prompt = prompt.with_context_value($key, $value);
            )*
            insta::assert_snapshot!(prompt.render().await.unwrap());
        }
    };
}

type Expectations = Arc<Mutex<Vec<(ChatCompletionRequest, Result<ChatCompletionResponse>)>>>;

#[derive(Clone)]
pub struct MockChatCompletion {
    expectations: Expectations,
    received_expectations: Expectations,
}

impl Default for MockChatCompletion {
    fn default() -> Self {
        Self::new()
    }
}

impl MockChatCompletion {
    pub fn new() -> Self {
        Self {
            expectations: Arc::new(Mutex::new(Vec::new())),
            received_expectations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn expect_complete(
        &self,
        request: ChatCompletionRequest,
        response: Result<ChatCompletionResponse>,
    ) {
        let mut mutex = self.expectations.lock().unwrap();

        mutex.insert(0, (request, response));
    }
}

#[async_trait]
impl ChatCompletion for MockChatCompletion {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        let (expected_request, response) = self
            .expectations
            .lock()
            .unwrap()
            .pop()
            .expect("Received completion request, but no expectations are set");

        assert_eq!(&expected_request, request, "Unexpected request {request:?}");

        if let Ok(response) = response {
            self.received_expectations
                .lock()
                .unwrap()
                .push((expected_request, Ok(response.clone())));

            Ok(response)
        } else {
            let err = response.unwrap_err();
            self.received_expectations
                .lock()
                .unwrap()
                .push((expected_request, Err(anyhow::anyhow!(err.to_string()))));

            Err(err)
        }
    }
}

impl Drop for MockChatCompletion {
    fn drop(&mut self) {
        // We are still cloned, so do not check assertions yet
        if Arc::strong_count(&self.received_expectations) > 1 {
            return;
        }
        if self.expectations.lock().unwrap().is_empty() {
            let num_received = self.received_expectations.lock().unwrap().len();
            tracing::debug!("[MockChatCompletion] All {num_received} expectations were met");
        } else {
            let received = self
                .received_expectations
                .lock()
                .unwrap()
                .iter()
                .map(pretty_expectation)
                .collect::<Vec<_>>()
                .join("---\n");

            let pending = self
                .expectations
                .lock()
                .unwrap()
                .iter()
                .map(pretty_expectation)
                .collect::<Vec<_>>()
                .join("---\n");

            panic!("[MockChatCompletion] Not all expectations were met\n received:\n{received}\n\npending:\n{pending}");
        }
    }
}

fn pretty_expectation(
    expectation: &(ChatCompletionRequest, Result<ChatCompletionResponse>),
) -> String {
    let mut output = String::new();

    let request = &expectation.0;
    for message in request.messages() {
        output.push_str(&format!(" {message}\n"));
    }

    output.push_str(" =>\n");

    let response_result = &expectation.1;

    if let Ok(response) = response_result {
        if let Some(message) = response.message() {
            output.push_str(&format!(" {message}\n"));
        }

        if let Some(tool_calls) = response.tool_calls() {
            for tool_call in tool_calls {
                output.push_str(&format!(" {tool_call}\n"));
            }
        }
    }

    output
}
