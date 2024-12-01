use async_trait::async_trait;
use dyn_clone::DynClone;

use crate::{AgentContext, CommandOutput};

use super::{
    chat_completion_request::ChatCompletionRequest,
    chat_completion_response::ChatCompletionResponse,
    errors::{ChatCompletionError, ToolError},
    ToolOutput, ToolSpec,
};

#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, ChatCompletionError>;
}

#[async_trait]
impl ChatCompletion for Box<dyn ChatCompletion> {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, ChatCompletionError> {
        (**self).complete(request).await
    }
}

#[async_trait]
impl ChatCompletion for &dyn ChatCompletion {
    async fn complete(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, ChatCompletionError> {
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
    ) -> Result<ChatCompletionResponse, ChatCompletionError> {
        (**self).complete(request).await
    }
}

impl<LLM> From<&LLM> for Box<dyn ChatCompletion>
where
    LLM: ChatCompletion + Clone + 'static,
{
    fn from(llm: &LLM) -> Self {
        Box::new(llm.clone())
    }
}

dyn_clone::clone_trait_object!(ChatCompletion);

impl From<CommandOutput> for ToolOutput {
    fn from(value: CommandOutput) -> Self {
        match value {
            CommandOutput::Text(value) => ToolOutput::Text(value),
            CommandOutput::Ok => ToolOutput::Text("Tool successfully completed".to_string()),
            CommandOutput::Shell {
                stdout,
                stderr,
                success,
                ..
            } => {
                let output = stdout + &stderr;
                if success {
                    ToolOutput::Text(output)
                } else {
                    ToolOutput::Fail(output)
                }
            }
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync + DynClone {
    // tbd
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput, ToolError>;

    fn name(&self) -> &'static str;

    fn tool_spec(&self) -> ToolSpec;

    fn boxed<'a>(self) -> Box<dyn Tool + 'a>
    where
        Self: Sized + 'a,
    {
        Box::new(self)
    }
}

dyn_clone::clone_trait_object!(Tool);

impl<T> From<T> for Box<dyn Tool + '_>
where
    for<'b> T: Tool + 'b,
{
    fn from(value: T) -> Self {
        // dyn_clone::clone_box(&value)
        Box::new(value)
    }
}

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
