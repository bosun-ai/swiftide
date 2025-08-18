use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;
use futures_util::Stream;
use std::{borrow::Cow, pin::Pin, sync::Arc};

use crate::AgentContext;

use super::{
    ToolCall, ToolOutput, ToolSpec,
    chat_completion_request::ChatCompletionRequest,
    chat_completion_response::ChatCompletionResponse,
    errors::{LanguageModelError, ToolError},
};

pub type ChatCompletionStream =
    Pin<Box<dyn Stream<Item = Result<ChatCompletionResponse, LanguageModelError>> + Send>>;
#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError>;

    /// Stream the completion response. If it's not supported, it will return a single
    /// response
    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        Box::pin(tokio_stream::iter(vec![self.complete(request).await]))
    }

    fn boxed(self) -> Box<dyn ChatCompletion>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

#[async_trait]
impl ChatCompletion for Box<dyn ChatCompletion> {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LanguageModelError> {
        (**self).complete(request).await
    }

    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        (**self).complete_stream(request).await
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

    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        (**self).complete_stream(request).await
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

    async fn complete_stream(&self, request: &ChatCompletionRequest) -> ChatCompletionStream {
        (**self).complete_stream(request).await
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
        tool_call: &ToolCall,
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
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        (**self).invoke(agent_context, tool_call).await
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
