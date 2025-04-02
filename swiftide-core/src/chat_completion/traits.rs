use async_trait::async_trait;
use dyn_clone::DynClone;
use std::borrow::Cow;

use crate::{AgentContext, CommandOutput};

use super::{
    chat_completion_request::ChatCompletionRequest,
    chat_completion_response::ChatCompletionResponse,
    errors::{LanguageModelError, ToolError},
    ToolOutput, ToolSpec,
};

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
