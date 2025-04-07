use std::{path::PathBuf, sync::Arc};

use crate::chat_completion::ChatMessage;
use anyhow::Result;
use async_trait::async_trait;
use thiserror::Error;

/// A tool executor that can be used within an `AgentContext`
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError>;
}

#[async_trait]
impl<T: ToolExecutor> ToolExecutor for &T {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (*self).exec_cmd(cmd).await
    }
}

#[async_trait]
impl ToolExecutor for Arc<dyn ToolExecutor> {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }
}

#[async_trait]
impl ToolExecutor for Box<dyn ToolExecutor> {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }
}

#[async_trait]
impl ToolExecutor for &dyn ToolExecutor {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    /// The executor itself failed
    #[error("executor error: {0:#}")]
    ExecutorError(#[from] anyhow::Error),

    /// The command failed, i.e. failing tests with stderr. This error might be handled
    #[error("command failed with NonZeroExit: {0}")]
    NonZeroExit(CommandOutput),
}

impl From<std::io::Error> for CommandError {
    fn from(err: std::io::Error) -> Self {
        CommandError::NonZeroExit(err.to_string().into())
    }
}

/// Commands that can be executed by the executor
/// Conceptually, `Shell` allows any kind of input, and other commands enable more optimized
/// implementations.
///
/// There is an ongoing consideration to make this an associated type on the executor
///
/// TODO: Should be able to borrow everything?
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
pub struct CommandOutput {
    pub output: String,
    // status_code: i32,
    // success: bool,
}

impl CommandOutput {
    pub fn empty() -> Self {
        CommandOutput {
            output: String::new(),
        }
    }

    pub fn new(output: impl Into<String>) -> Self {
        CommandOutput {
            output: output.into(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.output.is_empty()
    }
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.output.fmt(f)
    }
}

impl<T: Into<String>> From<T> for CommandOutput {
    fn from(value: T) -> Self {
        CommandOutput {
            output: value.into(),
        }
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
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError>;

    async fn history(&self) -> Vec<ChatMessage>;

    /// Pops the last messages up until the last completion
    ///
    /// LLMs failing completion for various reasons is unfortunately a common occurrence
    /// This gives a way to redrive the last completion in a generic way
    async fn redrive(&self);
}

#[async_trait]
impl AgentContext for Box<dyn AgentContext> {
    async fn next_completion(&self) -> Option<Vec<ChatMessage>> {
        (**self).next_completion().await
    }

    async fn current_new_messages(&self) -> Vec<ChatMessage> {
        (**self).current_new_messages().await
    }

    async fn add_messages(&self, item: Vec<ChatMessage>) {
        (**self).add_messages(item).await;
    }

    async fn add_message(&self, item: ChatMessage) {
        (**self).add_message(item).await;
    }

    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }

    async fn history(&self) -> Vec<ChatMessage> {
        (**self).history().await
    }

    async fn redrive(&self) {
        (**self).redrive().await;
    }
}

#[async_trait]
impl AgentContext for Arc<dyn AgentContext> {
    async fn next_completion(&self) -> Option<Vec<ChatMessage>> {
        (**self).next_completion().await
    }

    async fn current_new_messages(&self) -> Vec<ChatMessage> {
        (**self).current_new_messages().await
    }

    async fn add_messages(&self, item: Vec<ChatMessage>) {
        (**self).add_messages(item).await;
    }

    async fn add_message(&self, item: ChatMessage) {
        (**self).add_message(item).await;
    }

    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }

    async fn history(&self) -> Vec<ChatMessage> {
        (**self).history().await
    }

    async fn redrive(&self) {
        (**self).redrive().await;
    }
}

#[async_trait]
impl AgentContext for &dyn AgentContext {
    async fn next_completion(&self) -> Option<Vec<ChatMessage>> {
        (**self).next_completion().await
    }

    async fn current_new_messages(&self) -> Vec<ChatMessage> {
        (**self).current_new_messages().await
    }

    async fn add_messages(&self, item: Vec<ChatMessage>) {
        (**self).add_messages(item).await;
    }

    async fn add_message(&self, item: ChatMessage) {
        (**self).add_message(item).await;
    }

    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }

    async fn history(&self) -> Vec<ChatMessage> {
        (**self).history().await
    }

    async fn redrive(&self) {
        (**self).redrive().await;
    }
}

/// Convenience implementation for empty agent context
///
/// Errors if tools attempt to execute commands
#[async_trait]
impl AgentContext for () {
    async fn next_completion(&self) -> Option<Vec<ChatMessage>> {
        None
    }

    async fn current_new_messages(&self) -> Vec<ChatMessage> {
        Vec::new()
    }

    async fn add_messages(&self, _item: Vec<ChatMessage>) {}

    async fn add_message(&self, _item: ChatMessage) {}

    async fn exec_cmd(&self, _cmd: &Command) -> Result<CommandOutput, CommandError> {
        Err(CommandError::ExecutorError(anyhow::anyhow!(
            "Empty agent context does not have a tool executor"
        )))
    }

    async fn history(&self) -> Vec<ChatMessage> {
        Vec::new()
    }

    async fn redrive(&self) {}
}
