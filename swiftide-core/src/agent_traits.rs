use std::{hash::Hash, path::PathBuf};

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

/// Commands that can be executed by the executor
/// TODO: Borrow it all?
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Command {
    Shell(String),
    ReadFile(PathBuf),
    WriteFile(PathBuf, String),
}

#[derive(Debug, Clone)]
pub enum Output {
    /// Infallible text output
    Text(String),
    /// Empty infallible output
    Ok,
    /// Output from a shell command
    Shell {
        stdout: String,
        stderr: String,
        status: i32,
        success: bool,
    },
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Output::Text(value) => write!(f, "{value}"),
            Output::Shell { stdout, .. } => write!(f, "{stdout}"),
            Output::Ok => write!(f, "Ok"),
        }
    }
}

impl From<String> for Output {
    fn from(value: String) -> Self {
        Output::Text(value)
    }
}

impl From<Output> for ToolOutput {
    fn from(value: Output) -> Self {
        match value {
            Output::Text(value) => ToolOutput::Text(value),
            Output::Ok => ToolOutput::Ok,
            Output::Shell {
                stdout,
                stderr,
                success,
                ..
            } => {
                if success {
                    ToolOutput::Text(stdout)
                } else {
                    ToolOutput::Text(stderr)
                }
            }
        }
    }
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
pub trait AgentContext: Send + Sync {
    /// List of all messages for this agent
    ///
    /// Used as main source for the next completion and expects all
    /// messages to be returned if new messages are present.
    ///
    /// Once this method has been called, there should not be new messages
    ///
    /// TODO: Figure out a nice way to return a reference instead while still supporting i.e.
    /// mutexes
    async fn next_completion(&self) -> Option<Vec<ChatMessage>>;

    /// Add messages for the next completion
    async fn add_messages(&self, item: &[ChatMessage]);

    /// Add messages for the next completion
    async fn add_message(&self, item: &ChatMessage);

    /// Instruct the context to no longer return new completions
    fn stop(&self);

    /// Execute a command if the context supports it
    async fn exec_cmd(&self, cmd: &Command) -> Result<Output>;

    async fn history(&self) -> Vec<ChatMessage>;
}
