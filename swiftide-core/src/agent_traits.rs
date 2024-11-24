use std::{path::PathBuf, sync::Arc};

use crate::chat_completion::ChatMessage;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    // type Command: Send + Sync;
    // type Output: Send + Sync;

    // tbd if associated type makes sense
    // Pro: Flexible and up to executor to decide how it communicates and works
    // Con: Tools are not interchangeable if the executor uses different types.
    async fn exec_cmd(&self, cmd: &Command) -> Result<Output>;
}

#[async_trait]
impl<T: ToolExecutor> ToolExecutor for &T {
    async fn exec_cmd(&self, cmd: &Command) -> Result<Output> {
        (*self).exec_cmd(cmd).await
    }
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

impl Command {
    pub fn shell<S: Into<String>>(cmd: S) -> Self {
        Command::Shell(cmd.into())
    }

    pub fn read_file<P: Into<PathBuf>>(path: P) -> Self {
        Command::ReadFile(path.into())
    }

    pub fn write_file<P: Into<PathBuf>, S: Into<String>>(path: P, content: S) -> Self {
        Command::WriteFile(path.into(), content.into())
    }
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
            Output::Shell { stdout, stderr, .. } => {
                write!(f, "{stdout}{stderr}")
            }
            Output::Ok => write!(f, "Ok"),
        }
    }
}

impl From<String> for Output {
    fn from(value: String) -> Self {
        Output::Text(value)
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
