use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::chat_completion::{ChatCompletion, ChatCompletionRequest, ChatCompletionResponse};
use anyhow::Result;

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

#[derive(Clone)]
pub struct MockChatCompletion {
    expectations: Arc<Mutex<Vec<ChatCompletionRequest>>>,
    responses: Arc<Mutex<Vec<Result<ChatCompletionResponse>>>>,
}

impl MockChatCompletion {
    pub fn new() -> Self {
        Self {
            expectations: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn expect_complete(
        &self,
        request: ChatCompletionRequest,
        response: Result<ChatCompletionResponse>,
    ) {
        self.expectations.lock().unwrap().push(request);
        self.responses.lock().unwrap().push(response);
    }
}

#[async_trait]
impl ChatCompletion for MockChatCompletion {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        let mut expectations = self.expectations.lock().unwrap();
        let mut responses = self.responses.lock().unwrap();

        if let Some(expected_request) = expectations.pop() {
            assert_eq!(&expected_request, request, "Unexpected request");
        } else {
            panic!("No more expectations set for complete");
        }

        if let Some(response) = responses.pop() {
            response
        } else {
            panic!("No more responses set for complete");
        }
    }
}

impl Drop for MockChatCompletion {
    fn drop(&mut self) {
        let expectations = self.expectations.lock().unwrap();
        let responses = self.responses.lock().unwrap();
        if !expectations.is_empty() {
            panic!("Not all expectations were met");
        }
        if !responses.is_empty() {
            panic!("Not all responses were returned");
        }
    }
}
