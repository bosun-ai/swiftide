use std::{pin::Pin, sync::Arc};

use async_trait::async_trait;
use swiftide_core::{
    ChatCompletion, Command, CommandError, CommandOutput, SimplePrompt, ToolExecutor,
    chat_completion::{ChatCompletionRequest, ChatCompletionResponse, errors::LanguageModelError},
    prompt::Prompt,
};
use tokio::sync::Mutex;

use crate::{Agent, errors::AgentError};

use super::{
    errors::NodeError,
    node::{NodeArg, NodeId, TaskNode},
};

#[derive(Clone)]
pub struct SyncFn<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Send + Sync + Clone + 'static,
{
    pub f: F,
    _phantom: std::marker::PhantomData<(I, O)>,
}

#[derive(Clone)]
pub struct AsyncFn<F, I, O>
where
    F: for<'a> Fn(&'a I) -> Pin<Box<dyn Future<Output = Result<O, NodeError>> + Send + 'a>>
        + Send
        + Sync
        + Clone
        + 'static,
{
    pub f: F,
    _phantom: std::marker::PhantomData<(I, O)>,
}

impl<F, I, O> SyncFn<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Send + Sync + Clone + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F, I, O> AsyncFn<F, I, O>
where
    F: for<'a> Fn(&'a I) -> Pin<Box<dyn Future<Output = Result<O, NodeError>> + Send + 'a>>
        + Send
        + Sync
        + Clone
        + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F> From<F> for SyncFn<F, (), ()>
where
    F: Fn(&()) -> Result<(), NodeError> + Send + Sync + Clone + 'static,
{
    fn from(f: F) -> Self {
        Self::new(f)
    }
}

impl<F> From<F> for AsyncFn<F, (), ()>
where
    F: for<'a> Fn(&'a ()) -> Pin<Box<dyn Future<Output = Result<(), NodeError>> + Send + 'a>>
        + Send
        + Sync
        + Clone
        + 'static,
{
    fn from(f: F) -> Self {
        Self::new(f)
    }
}

#[async_trait]
impl<F, I, O> TaskNode for SyncFn<F, I, O>
where
    F: Fn(&I) -> Result<O, NodeError> + Clone + Send + Sync + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    type Input = I;
    type Output = O;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        (self.f)(input)
    }
}

#[async_trait]
impl<F, I, O> TaskNode for AsyncFn<F, I, O>
where
    F: for<'a> Fn(&'a I) -> Pin<Box<dyn Future<Output = Result<O, NodeError>> + Send + 'a>>
        + Clone
        + Send
        + Sync
        + 'static,
    I: NodeArg + Clone,
    O: NodeArg + Clone,
{
    type Input = I;
    type Output = O;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        (self.f)(input).await
    }
}

/// An example of wrapping an Agent as a `TaskNode`
///
/// For more control you can always roll your own
#[derive(Clone, Debug)]
pub struct TaskAgent(Arc<Mutex<Agent>>);

impl From<Agent> for TaskAgent {
    fn from(agent: Agent) -> Self {
        Self(Arc::new(Mutex::new(agent)))
    }
}

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

macro_rules! impl_task_node_for_prompt_like {
    ($ty:ty, $input:ty, $output:ty, $error:ty, |$this:ident, $input_value:ident| $body:expr) => {
        #[async_trait]
        impl TaskNode for $ty {
            type Input = $input;
            type Output = $output;
            type Error = $error;

            async fn evaluate(
                &self,
                _node_id: &NodeId<
                    dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
                >,
                $input_value: &Self::Input,
            ) -> Result<Self::Output, Self::Error> {
                let $this = self;
                $body
            }
        }
    };
}

impl_task_node_for_prompt_like!(
    Box<dyn SimplePrompt>,
    Prompt,
    String,
    LanguageModelError,
    |this, input| {
        // TODO: Prompt should be borrowed
        this.prompt(input.clone()).await
    }
);

impl_task_node_for_prompt_like!(
    Arc<dyn SimplePrompt>,
    Prompt,
    String,
    LanguageModelError,
    |this, input| {
        // TODO: Prompt should be borrowed
        this.prompt(input.clone()).await
    }
);

impl_task_node_for_prompt_like!(
    Box<dyn ChatCompletion>,
    ChatCompletionRequest<'static>,
    ChatCompletionResponse,
    LanguageModelError,
    |this, input| this.complete(input).await
);

impl_task_node_for_prompt_like!(
    Arc<dyn ChatCompletion>,
    ChatCompletionRequest<'static>,
    ChatCompletionResponse,
    LanguageModelError,
    |this, input| this.complete(input).await
);

impl_task_node_for_prompt_like!(
    Box<dyn ToolExecutor>,
    Command,
    CommandOutput,
    CommandError,
    |this, input| this.exec_cmd(input).await
);

impl_task_node_for_prompt_like!(
    Arc<dyn ToolExecutor>,
    Command,
    CommandOutput,
    CommandError,
    |this, input| this.exec_cmd(input).await
);

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
