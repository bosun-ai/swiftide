use std::path::PathBuf;

use crate::chat_completion::ChatMessage;
use anyhow::Result;
use async_trait::async_trait;

/// A tool executor that can be used within an `AgentContext`
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput>;
}

#[async_trait]
impl<T: ToolExecutor> ToolExecutor for &T {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput> {
        (*self).exec_cmd(cmd).await
    }
}

/// Commands that can be executed by the executor
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

/// Output from a `Command`
#[derive(Debug, Clone)]
pub enum CommandOutput {
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

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandOutput::Text(value) => write!(f, "{value}"),
            CommandOutput::Shell { stdout, stderr, .. } => {
                write!(f, "{stdout}{stderr}")
            }
            CommandOutput::Ok => write!(f, "Ok"),
        }
    }
}

impl From<String> for CommandOutput {
    fn from(value: String) -> Self {
        CommandOutput::Text(value)
    }
}

/// Acts as the interface to the external world and manages messages for completion
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

    /// Lists only the new messages after calling `new_completion`
    async fn current_new_messages(&self) -> Vec<ChatMessage>;

    /// Add messages for the next completion
    async fn add_messages(&self, item: Vec<ChatMessage>);

    /// Add messages for the next completion
    async fn add_message(&self, item: ChatMessage);

    /// Execute a command if the context supports it
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput>;

    async fn history(&self) -> Vec<ChatMessage>;
}
