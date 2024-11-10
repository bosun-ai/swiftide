use std::hash::Hash;

use crate::chat_completion::{ChatMessage, JsonSpec, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    // type Command: Send + Sync;
    // type Output: Send + Sync;

    // tbd if associated type makes sense
    // Pro: Flexible and up to executor to decide how it communicates and works
    // Con: Tools are not interchangeable if the executor uses different types.
    async fn exec_cmd(&self, cmd: &Command) -> Result<Output>;
}

pub enum Command {
    Shell(String),
}

pub enum Output {
    Text(String),
}

// dyn_clone::clone_trait_object!(Workspace);

#[async_trait]
pub trait Tool: Send + Sync + DynClone {
    // tbd
    async fn invoke(
        &self,
        agent_context: &dyn AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput>;

    fn name(&self) -> &'static str;

    // Ideas:
    // Typed instead of string
    // LLMs have different requirements, validators?
    fn json_spec(&self) -> JsonSpec;
}

dyn_clone::clone_trait_object!(Tool);

impl<'a, T: 'a> From<T> for Box<dyn Tool + 'a>
where
    T: Tool,
{
    fn from(value: T) -> Self {
        dyn_clone::clone_box(&value)
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
impl Hash for Box<dyn Tool> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name().hash(state);
    }
}

/// Acts as the interface to the external world and any overlapping state
/// NOTE: Async as expecting locks
#[async_trait]
pub trait AgentContext<EXECUTOR: ToolExecutor = ()>: Send + Sync {
    /// List of all messages for this agent
    ///
    /// Used as main source for the next completion and expects all
    /// what you would expect in an inference conversation to be present.
    async fn completion_history(&self) -> &[ChatMessage];

    async fn add_message(&mut self, item: ChatMessage);

    async fn record_iteration(&mut self);

    async fn current_chat_messages(&self) -> &[ChatMessage];

    fn stop(&mut self);

    fn should_stop(&self) -> bool;

    async fn exec_cmd(&self, cmd: &Command) -> Result<Output> {
        anyhow::bail!("Command execution not implemented");
    }
}

#[async_trait]
impl ToolExecutor for () {
    async fn exec_cmd(&self, _cmd: &Command) -> Result<Output> {
        anyhow::bail!("No tool executor provided");
    }
}

// dyn_clone::clone_trait_object!(AgentContext);
// #[async_trait]
// impl<WORKSPACE: Workspace + Clone> AgentContext<WORKSPACE> for Box<dyn AgentContext<WORKSPACE>> {
//     async fn completion_history(&self) -> &[ChatMessage] {
//         self.as_ref().completion_history().await
//     }
//     async fn add_message(&mut self, item: ChatMessage) {
//         self.as_mut().add_message(item).await;
//     }
//
//     async fn record_iteration(&mut self) {
//         self.as_mut().record_iteration().await;
//     }
//
//     async fn current_chat_messages(&self) -> &[ChatMessage] {
//         self.as_ref().current_chat_messages().await
//     }
//
//     fn stop(&mut self) {
//         self.as_mut().stop();
//     }
//
//     fn should_stop(&self) -> bool {
//         self.as_ref().should_stop()
//     }
//
//     async fn exec_cmd(&self, cmd: &WORKSPACE::Command) -> Result<WORKSPACE::Output> {
//         self.as_ref().exec_cmd(cmd).await
//     }
// }
