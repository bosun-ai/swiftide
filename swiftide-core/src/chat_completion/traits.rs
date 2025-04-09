use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;
use std::{borrow::Cow, sync::Arc};

use crate::{AgentContext, CommandOutput, LanguageModelWithBackOff};

use super::{
    chat_completion_request::ChatCompletionRequest,
    chat_completion_response::ChatCompletionResponse,
    errors::{LanguageModelError, ToolError},
    ToolOutput, ToolSpec,
};

#[async_trait]
impl<LLM: ChatCompletion + Clone> ChatCompletion for LanguageModelWithBackOff<LLM> {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        let strategy = self.strategy();

        let op = || {
            let request = request.clone();
            async move {
                self.inner.complete(&request).await.map_err(|e| match e {
                    LanguageModelError::ContextLengthExceeded(e) => {
                        backoff::Error::Permanent(LanguageModelError::ContextLengthExceeded(e))
                    }
                    LanguageModelError::PermanentError(e) => {
                        backoff::Error::Permanent(LanguageModelError::PermanentError(e))
                    }
                    LanguageModelError::TransientError(e) => {
                        backoff::Error::transient(LanguageModelError::TransientError(e))
                    }
                })
            }
        };

        backoff::future::retry(strategy, op).await
    }
}

#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError>;
}

#[async_trait]
impl ChatCompletion for Box<dyn ChatCompletion> {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        (**self).complete(request).await
    }
}

#[async_trait]
impl ChatCompletion for &dyn ChatCompletion {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        (**self).complete(request).await
    }
}

#[async_trait]
impl<T> ChatCompletion for &T
where
    T: ChatCompletion + Clone + 'static,
{
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        (**self).complete(request).await
    }
}

impl<LLM> From<&LLM> for Box<dyn ChatCompletion>
where
    LLM: ChatCompletion + Clone + 'static,
{
    fn from(llm: &LLM) -> Self {
        Box::new(llm.clone()) as Box<dyn ChatCompletion>
    }
}

dyn_clone::clone_trait_object!(ChatCompletion);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BackoffConfiguration;
    use std::{
        collections::HashSet,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    #[derive(Clone)]
    enum MockErrorType {
        Transient,
        Permanent,
        ContextLengthExceeded,
    }

    #[derive(Clone)]
    struct MockChatCompletion {
        call_count: Arc<AtomicUsize>,
        should_fail_count: usize,
        error_type: MockErrorType,
    }

    #[async_trait]
    impl ChatCompletion for MockChatCompletion {
        async fn complete(
            &self,
            _request: &ChatCompletionRequest,
        ) -> Result<ChatCompletionResponse, LanguageModelError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);

            if count < self.should_fail_count {
                match self.error_type {
                    MockErrorType::Transient => Err(LanguageModelError::TransientError(Box::new(
                        std::io::Error::new(std::io::ErrorKind::ConnectionReset, "Transient error"),
                    ))),
                    MockErrorType::Permanent => Err(LanguageModelError::PermanentError(Box::new(
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "Permanent error"),
                    ))),
                    MockErrorType::ContextLengthExceeded => Err(
                        LanguageModelError::ContextLengthExceeded(Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Context length exceeded",
                        ))),
                    ),
                }
            } else {
                Ok(ChatCompletionResponse {
                    message: Some("Success response".to_string()),
                    tool_calls: None,
                })
            }
        }
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_retries_chat_completion_transient_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_chat = MockChatCompletion {
            call_count: call_count.clone(),
            should_fail_count: 2, // Fail twice, succeed on third attempt
            error_type: MockErrorType::Transient,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_chat, config);

        let request = ChatCompletionRequest {
            messages: vec![],
            tools_spec: HashSet::default(),
        };

        let result = model_with_backoff.complete(&request).await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(
            result.unwrap().message,
            Some("Success response".to_string())
        );
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_does_not_retry_chat_completion_permanent_errors() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_chat = MockChatCompletion {
            call_count: call_count.clone(),
            should_fail_count: 2, // Would fail twice if retried
            error_type: MockErrorType::Permanent,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_chat, config);

        let request = ChatCompletionRequest {
            messages: vec![],
            tools_spec: HashSet::default(),
        };

        let result = model_with_backoff.complete(&request).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should only be called once

        match result {
            Err(LanguageModelError::PermanentError(_)) => {} // Expected
            _ => panic!("Expected PermanentError, got {result:?}"),
        }
    }

    #[tokio::test]
    async fn test_language_model_with_backoff_does_not_retry_chat_completion_context_length_errors()
    {
        let call_count = Arc::new(AtomicUsize::new(0));
        let mock_chat = MockChatCompletion {
            call_count: call_count.clone(),
            should_fail_count: 2, // Would fail twice if retried
            error_type: MockErrorType::ContextLengthExceeded,
        };

        let config = BackoffConfiguration {
            initial_interval_sec: 1,
            max_elapsed_time_sec: 10,
            multiplier: 1.5,
            randomization_factor: 0.5,
        };

        let model_with_backoff = LanguageModelWithBackOff::new(mock_chat, config);

        let request = ChatCompletionRequest {
            messages: vec![],
            tools_spec: HashSet::default(),
        };

        let result = model_with_backoff.complete(&request).await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should only be called once

        match result {
            Err(LanguageModelError::ContextLengthExceeded(_)) => {} // Expected
            _ => panic!("Expected ContextLengthExceeded, got {result:?}"),
        }
    }
}

impl From<CommandOutput> for ToolOutput {
    fn from(value: CommandOutput) -> Self {
        ToolOutput::Text(value.output)
    }
}

/// The `Tool` trait is the main interface for chat completion and agent tools.
///
/// `swiftide-macros` provides a set of macros to generate implementations of this trait. If you
/// need more control over the implementation, you can implement the trait manually.
///
/// The `ToolSpec` is what will end up with the LLM. A builder is provided. The `name` is expected
/// to be unique, and is used to identify the tool. It should be the same as the name in the
/// `ToolSpec`.
#[async_trait]
pub trait Tool: Send + Sync + DynClone {
    // tbd
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput, ToolError>;

    fn name(&self) -> Cow<'_, str>;

    fn tool_spec(&self) -> ToolSpec;

    fn boxed<'a>(self) -> Box<dyn Tool + 'a>
    where
        Self: Sized + 'a,
    {
        Box::new(self) as Box<dyn Tool>
    }
}

/// A toolbox is a collection of tools
///
/// It can be a list, an mcp client, or anything else we can think of.
///
/// This allows agents to not know their tools when they are created, and to get them at runtime.
///
/// It also allows for tools to be dynamically loaded and unloaded, etc.
#[async_trait]
pub trait ToolBox: Send + Sync + DynClone {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>>;

    fn name(&self) -> Cow<'_, str> {
        Cow::Borrowed("Unnamed ToolBox")
    }

    fn boxed<'a>(self) -> Box<dyn ToolBox + 'a>
    where
        Self: Sized + 'a,
    {
        Box::new(self) as Box<dyn ToolBox>
    }
}

#[async_trait]
impl ToolBox for Vec<Box<dyn Tool>> {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        Ok(self.clone())
    }
}

#[async_trait]
impl ToolBox for Box<dyn ToolBox> {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        (**self).available_tools().await
    }
}

#[async_trait]
impl ToolBox for Arc<dyn ToolBox> {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        (**self).available_tools().await
    }
}

#[async_trait]
impl ToolBox for &dyn ToolBox {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        (**self).available_tools().await
    }
}

#[async_trait]
impl ToolBox for &[Box<dyn Tool>] {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        Ok(self.to_vec())
    }
}

#[async_trait]
impl ToolBox for [Box<dyn Tool>] {
    async fn available_tools(&self) -> Result<Vec<Box<dyn Tool>>> {
        Ok(self.to_vec())
    }
}

dyn_clone::clone_trait_object!(ToolBox);

#[async_trait]
impl Tool for Box<dyn Tool> {
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        (**self).invoke(agent_context, raw_args).await
    }
    fn name(&self) -> Cow<'_, str> {
        (**self).name()
    }
    fn tool_spec(&self) -> ToolSpec {
        (**self).tool_spec()
    }
}

dyn_clone::clone_trait_object!(Tool);

/// Tools are identified and unique by name
/// These allow comparison and lookups
impl PartialEq for Box<dyn Tool> {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}
impl Eq for Box<dyn Tool> {}
impl std::hash::Hash for Box<dyn Tool> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name().hash(state);
    }
}
