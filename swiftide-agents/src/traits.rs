use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use dyn_clone::DynClone;
use swiftide_core::{chat_completion::JsonSpec, prompt::Prompt};

use crate::{agent::Agent, agent_context::AgentContext};

#[async_trait]
pub trait Workspace: Send + Sync + DynClone {
    // tbd naming
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput>;

    // Ensures commands can be run
    // tbd what to do with git setup etc

    // Maybe leave it to user?
    async fn init(&self) -> Result<()>;

    async fn teardown(self);
}

dyn_clone::clone_trait_object!(Workspace);

#[async_trait]
pub trait Tool: Send + Sync + DynClone {
    // tbd
    async fn invoke(
        &self,
        agent_context: &AgentContext,
        raw_args: Option<&str>,
    ) -> Result<ToolOutput>;

    fn name(&self) -> &'static str;

    // Ideas:
    // Typed instead of string
    // LLMs have different requirements, validators?
    fn json_spec(&self) -> JsonSpec;
}

/// Acts as the interface to the external world and any overlapping state
/// NOTE: Async as expecting locks
// #[async_trait]
// pub trait AgentContext: Send + Sync + DynClone {
//     async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput>;
//
//     /// List of all messages for this agent, for the purpose of completion and logs
//     async fn conversation_history(&self) -> &[MessageHistoryRecord];
//
//     // async fn record_message_history<T: ?Sized + Into<MessageHistoryRecord>>(&mut self, history: T);
// }
//

// TMP
pub enum Command {
    Shell(String),
    // Git, Github, File, Code, etc
}

pub enum CommandOutput {
    // tbd
    Stdout(String),
    Stderr(String),
    Status(i32),
}

// Idea to have semantically usuable types that have default behaviour
// Additionally, unlike Fluyt, handle as much as possible via tools
//
// Maybe this should be a struct?

type Success = bool;
