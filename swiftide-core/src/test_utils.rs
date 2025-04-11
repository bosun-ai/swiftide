#![allow(clippy::missing_panics_doc)]
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::chat_completion::{
    errors::LanguageModelError, ChatCompletion, ChatCompletionRequest, ChatCompletionResponse,
};
use anyhow::Result;
use pretty_assertions::assert_eq;

#[macro_export]
macro_rules! assert_default_prompt_snapshot {
    ($node:expr, $($key:expr => $value:expr),*) => {
        #[tokio::test]
        async fn test_default_prompt() {
        let template = default_prompt();
        let mut prompt = template.clone().with_node(&Node::new($node));
        $(
            prompt = prompt.with_context_value($key, $value);
        )*
        insta::assert_snapshot!(prompt.render().unwrap());
        }
    };

    ($($key:expr => $value:expr),*) => {
        #[tokio::test]
        async fn test_default_prompt() {
            let template = default_prompt();
            let mut prompt = template;
            $(
                prompt = prompt.with_context_value($key, $value);
            )*
            insta::assert_snapshot!(prompt.render().unwrap());
        }
    };
}

type Expectations = Arc<Mutex<Vec<(ChatCompletionRequest, Result<ChatCompletionResponse>)>>>;

#[derive(Clone)]
pub struct MockChatCompletion {
    pub expectations: Expectations,
    pub received_expectations: Expectations,
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
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        let (expected_request, response) =
            self.expectations.lock().unwrap().pop().unwrap_or_else(|| {
                panic!(
                    "Received completion request, but no expectations are set\n {}",
                    pretty_request(request)
                )
            });

        assert_eq!(
            &expected_request,
            request,
            "Unexpected request\n: {}\nRemaining expectations:\n{}",
            pretty_request(request),
            pretty_expectation(&(expected_request.clone(), response))
                + "---\n"
                + &self
                    .expectations
                    .lock()
                    .unwrap()
                    .iter()
                    .map(pretty_expectation)
                    .collect::<Vec<_>>()
                    .join("---\n")
        );

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

            Err(LanguageModelError::PermanentError(err.into()))
        }
    }
}

impl Drop for MockChatCompletion {
    fn drop(&mut self) {
        // We are still cloned, so do not check assertions yet
        if Arc::strong_count(&self.received_expectations) > 1 {
            return;
        }
        let Ok(expectations) = self.expectations.lock() else {
            return;
        };
        let Ok(received) = self.received_expectations.lock() else {
            return;
        };

        if expectations.is_empty() {
            let num_received = received.len();
            tracing::debug!("[MockChatCompletion] All {num_received} expectations were met");
        } else {
            let received = received
                .iter()
                .map(pretty_expectation)
                .collect::<Vec<_>>()
                .join("---\n");

            let pending = expectations
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
    output.push_str("Request:\n");
    output.push_str(&pretty_request(request));

    output.push_str(" =>\n");

    let response_result = &expectation.1;

    if let Ok(response) = response_result {
        output += &pretty_response(response);
    }

    output
}

fn pretty_request(request: &ChatCompletionRequest) -> String {
    let mut output = String::new();
    for message in request.messages() {
        writeln!(output, " {message}").unwrap();
    }
    output
}

fn pretty_response(response: &ChatCompletionResponse) -> String {
    let mut output = String::new();
    if let Some(message) = response.message() {
        writeln!(output, " {message}").unwrap();
    }
    if let Some(tool_calls) = response.tool_calls() {
        for tool_call in tool_calls {
            writeln!(output, " {tool_call}").unwrap();
        }
    }
    output
}
