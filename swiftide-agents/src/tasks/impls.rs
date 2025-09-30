use std::sync::Arc;

use async_trait::async_trait;
use swiftide_core::{
    ChatCompletion, Command, CommandError, CommandOutput, DynToolExecutor, SimplePrompt,
    chat_completion::{ChatCompletionRequest, ChatCompletionResponse, errors::LanguageModelError},
    prompt::Prompt,
};
use tokio::sync::Mutex;

use crate::{Agent, errors::AgentError};

use super::node::{NodeArg, NodeId, TaskNode};

/// An example of wrapping an Agent as a `TaskNode`
///
/// For more control you can always roll your own
#[derive(Clone, Debug)]
pub struct TaskAgent(Arc<Mutex<Agent>>);

impl From<Agent> for TaskAgent {
    fn from(agent: Agent) -> Self {
        TaskAgent(Arc::new(Mutex::new(agent)))
    }
}

/// A 'default' implementation for an agent where there is no output
#[async_trait]
impl TaskNode for TaskAgent {
    type Input = Prompt;

    type Output = ();

    type Error = AgentError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.0.lock().await.query(input.clone()).await
    }
}

#[async_trait]
impl TaskNode for Box<dyn SimplePrompt> {
    type Input = Prompt;

    type Output = String;

    type Error = LanguageModelError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // TODO: Prompt should be borrowed
        self.prompt(input.clone()).await
    }
}

#[async_trait]
impl TaskNode for Arc<dyn SimplePrompt> {
    type Input = Prompt;

    type Output = String;

    type Error = LanguageModelError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        // TODO: Prompt should be borrowed
        self.prompt(input.clone()).await
    }
}

#[async_trait]
impl TaskNode for Box<dyn ChatCompletion> {
    type Input = ChatCompletionRequest;

    type Output = ChatCompletionResponse;

    type Error = LanguageModelError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.complete(input).await
    }
}

#[async_trait]
impl TaskNode for Arc<dyn ChatCompletion> {
    type Input = ChatCompletionRequest;

    type Output = ChatCompletionResponse;

    type Error = LanguageModelError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.complete(input).await
    }
}

#[async_trait]
impl TaskNode for Box<dyn DynToolExecutor> {
    type Input = Command;

    type Output = CommandOutput;

    type Error = CommandError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.exec_cmd(input).await
    }
}

#[async_trait]
impl TaskNode for Arc<dyn DynToolExecutor> {
    type Input = Command;

    type Output = CommandOutput;

    type Error = CommandError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.exec_cmd(input).await
    }
}

// Note: This only works for function pointers, not closures.
#[async_trait]
impl<I: NodeArg, O: NodeArg, E: std::error::Error + Send + Sync + 'static> TaskNode
    for fn(&I) -> Result<O, E>
{
    type Input = I;

    type Output = O;

    type Error = E;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        (self)(input)
    }
}
