use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    chat_completion::{ChatMessage, ToolCall},
    indexing::IndexingStream,
};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A ToolExecutor provides an interface for agents to interact with a system
/// in an isolated context.
///
/// When starting up an agent, it's context expects an executor. For example,
/// you might want your coding agent to work with a fresh, isolated set of files,
/// separated from the rest of the system.
///
/// See `swiftide-docker-executor` for an executor that uses Docker. By default
/// the executor is a local executor.
///
/// Additionally, the executor can be used stream files files for indexing.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a command in the executor
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError>;

    /// Stream files from the executor
    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream>;
}

#[async_trait]
impl<T: ToolExecutor> ToolExecutor for &T {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (*self).exec_cmd(cmd).await
    }

    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream> {
        (*self).stream_files(path, extensions).await
    }
}

#[async_trait]
impl ToolExecutor for Arc<dyn ToolExecutor> {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        self.as_ref().exec_cmd(cmd).await
    }
    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream> {
        self.as_ref().stream_files(path, extensions).await
    }
}

#[async_trait]
impl ToolExecutor for Box<dyn ToolExecutor> {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        self.as_ref().exec_cmd(cmd).await
    }
    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream> {
        self.as_ref().stream_files(path, extensions).await
    }
}

#[async_trait]
impl ToolExecutor for &dyn ToolExecutor {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (*self).exec_cmd(cmd).await
    }
    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream> {
        (*self).stream_files(path, extensions).await
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

impl AsRef<str> for CommandOutput {
    fn as_ref(&self) -> &str {
        &self.output
    }
}

/// Feedback that can be given on a tool, i.e. with a human in the loop
#[derive(Debug, Clone, Serialize, Deserialize, strum_macros::EnumIs)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ToolFeedback {
    Approved { payload: Option<serde_json::Value> },
    Refused { payload: Option<serde_json::Value> },
}

impl ToolFeedback {
    pub fn approved() -> Self {
        ToolFeedback::Approved { payload: None }
    }

    pub fn refused() -> Self {
        ToolFeedback::Refused { payload: None }
    }

    pub fn payload(&self) -> Option<&serde_json::Value> {
        match self {
            ToolFeedback::Refused { payload } | ToolFeedback::Approved { payload } => {
                payload.as_ref()
            }
        }
    }

    #[must_use]
    pub fn with_payload(self, payload: serde_json::Value) -> Self {
        match self {
            ToolFeedback::Approved { .. } => ToolFeedback::Approved {
                payload: Some(payload),
            },
            ToolFeedback::Refused { .. } => ToolFeedback::Refused {
                payload: Some(payload),
            },
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
    async fn next_completion(&self) -> Result<Option<Vec<ChatMessage>>>;

    /// Lists only the new messages after calling `new_completion`
    async fn current_new_messages(&self) -> Result<Vec<ChatMessage>>;

    /// Add messages for the next completion
    async fn add_messages(&self, item: Vec<ChatMessage>) -> Result<()>;

    /// Add messages for the next completion
    async fn add_message(&self, item: ChatMessage) -> Result<()>;

    /// Execute a command if the context supports it
    ///
    /// Deprecated: use executor instead to access the executor directly
    #[deprecated(note = "use executor instead")]
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError>;

    fn executor(&self) -> &Arc<dyn ToolExecutor>;

    async fn history(&self) -> Result<Vec<ChatMessage>>;

    /// Pops the last messages up until the last completion
    ///
    /// LLMs failing completion for various reasons is unfortunately a common occurrence
    /// This gives a way to redrive the last completion in a generic way
    async fn redrive(&self) -> Result<()>;

    /// Tools that require feedback or approval (i.e. from a human) can use this to check if the
    /// feedback is received
    async fn has_received_feedback(&self, tool_call: &ToolCall) -> Option<ToolFeedback>;

    async fn feedback_received(&self, tool_call: &ToolCall, feedback: &ToolFeedback) -> Result<()>;
}

#[async_trait]
impl AgentContext for Box<dyn AgentContext> {
    async fn next_completion(&self) -> Result<Option<Vec<ChatMessage>>> {
        (**self).next_completion().await
    }

    async fn current_new_messages(&self) -> Result<Vec<ChatMessage>> {
        (**self).current_new_messages().await
    }

    async fn add_messages(&self, item: Vec<ChatMessage>) -> Result<()> {
        (**self).add_messages(item).await
    }

    async fn add_message(&self, item: ChatMessage) -> Result<()> {
        (**self).add_message(item).await
    }

    #[allow(deprecated)]
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }

    fn executor(&self) -> &Arc<dyn ToolExecutor> {
        (**self).executor()
    }

    async fn history(&self) -> Result<Vec<ChatMessage>> {
        (**self).history().await
    }

    async fn redrive(&self) -> Result<()> {
        (**self).redrive().await
    }

    async fn has_received_feedback(&self, tool_call: &ToolCall) -> Option<ToolFeedback> {
        (**self).has_received_feedback(tool_call).await
    }

    async fn feedback_received(&self, tool_call: &ToolCall, feedback: &ToolFeedback) -> Result<()> {
        (**self).feedback_received(tool_call, feedback).await
    }
}

#[async_trait]
impl AgentContext for Arc<dyn AgentContext> {
    async fn next_completion(&self) -> Result<Option<Vec<ChatMessage>>> {
        (**self).next_completion().await
    }

    async fn current_new_messages(&self) -> Result<Vec<ChatMessage>> {
        (**self).current_new_messages().await
    }

    async fn add_messages(&self, item: Vec<ChatMessage>) -> Result<()> {
        (**self).add_messages(item).await
    }

    async fn add_message(&self, item: ChatMessage) -> Result<()> {
        (**self).add_message(item).await
    }

    #[allow(deprecated)]
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }

    fn executor(&self) -> &Arc<dyn ToolExecutor> {
        (**self).executor()
    }

    async fn history(&self) -> Result<Vec<ChatMessage>> {
        (**self).history().await
    }

    async fn redrive(&self) -> Result<()> {
        (**self).redrive().await
    }

    async fn has_received_feedback(&self, tool_call: &ToolCall) -> Option<ToolFeedback> {
        (**self).has_received_feedback(tool_call).await
    }

    async fn feedback_received(&self, tool_call: &ToolCall, feedback: &ToolFeedback) -> Result<()> {
        (**self).feedback_received(tool_call, feedback).await
    }
}

#[async_trait]
impl AgentContext for &dyn AgentContext {
    async fn next_completion(&self) -> Result<Option<Vec<ChatMessage>>> {
        (**self).next_completion().await
    }

    async fn current_new_messages(&self) -> Result<Vec<ChatMessage>> {
        (**self).current_new_messages().await
    }

    async fn add_messages(&self, item: Vec<ChatMessage>) -> Result<()> {
        (**self).add_messages(item).await
    }

    async fn add_message(&self, item: ChatMessage) -> Result<()> {
        (**self).add_message(item).await
    }

    #[allow(deprecated)]
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }

    fn executor(&self) -> &Arc<dyn ToolExecutor> {
        (**self).executor()
    }

    async fn history(&self) -> Result<Vec<ChatMessage>> {
        (**self).history().await
    }

    async fn redrive(&self) -> Result<()> {
        (**self).redrive().await
    }

    async fn has_received_feedback(&self, tool_call: &ToolCall) -> Option<ToolFeedback> {
        (**self).has_received_feedback(tool_call).await
    }

    async fn feedback_received(&self, tool_call: &ToolCall, feedback: &ToolFeedback) -> Result<()> {
        (**self).feedback_received(tool_call, feedback).await
    }
}

/// Convenience implementation for empty agent context
///
/// Errors if tools attempt to execute commands
#[async_trait]
impl AgentContext for () {
    async fn next_completion(&self) -> Result<Option<Vec<ChatMessage>>> {
        Ok(None)
    }

    async fn current_new_messages(&self) -> Result<Vec<ChatMessage>> {
        Ok(Vec::new())
    }

    async fn add_messages(&self, _item: Vec<ChatMessage>) -> Result<()> {
        Ok(())
    }

    async fn add_message(&self, _item: ChatMessage) -> Result<()> {
        Ok(())
    }

    async fn exec_cmd(&self, _cmd: &Command) -> Result<CommandOutput, CommandError> {
        Err(CommandError::ExecutorError(anyhow::anyhow!(
            "Empty agent context does not have a tool executor"
        )))
    }

    fn executor(&self) -> &Arc<dyn ToolExecutor> {
        unimplemented!("Empty agent context does not have a tool executor")
    }

    async fn history(&self) -> Result<Vec<ChatMessage>> {
        Ok(Vec::new())
    }

    async fn redrive(&self) -> Result<()> {
        Ok(())
    }

    async fn has_received_feedback(&self, _tool_call: &ToolCall) -> Option<ToolFeedback> {
        Some(ToolFeedback::Approved { payload: None })
    }

    async fn feedback_received(
        &self,
        _tool_call: &ToolCall,
        _feedback: &ToolFeedback,
    ) -> Result<()> {
        Ok(())
    }
}

/// A backend for the agent context. A default is provided for Arc<Mutex<Vec<ChatMessage>>>
///
/// If you want to use for instance a database, implement this trait and pass it to the agent
/// context when creating it.
#[async_trait]
pub trait MessageHistory: Send + Sync + std::fmt::Debug {
    /// Returns the history of messages
    async fn history(&self) -> Result<Vec<ChatMessage>>;

    /// Add a message to the history
    async fn push_owned(&self, item: ChatMessage) -> Result<()>;

    /// Overwrite the history with the given items
    async fn overwrite(&self, items: Vec<ChatMessage>) -> Result<()>;

    /// Add a message to the history
    async fn push(&self, item: &ChatMessage) -> Result<()> {
        self.push_owned(item.clone()).await
    }

    /// Extend the history with the given items
    async fn extend(&self, items: &[ChatMessage]) -> Result<()> {
        self.extend_owned(items.to_vec()).await
    }

    /// Extend the history with the given items, taking ownership of them
    async fn extend_owned(&self, items: Vec<ChatMessage>) -> Result<()> {
        for item in items {
            self.push_owned(item).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl MessageHistory for Mutex<Vec<ChatMessage>> {
    async fn history(&self) -> Result<Vec<ChatMessage>> {
        Ok(self.lock().unwrap().clone())
    }

    async fn push_owned(&self, item: ChatMessage) -> Result<()> {
        self.lock().unwrap().push(item);

        Ok(())
    }

    async fn overwrite(&self, items: Vec<ChatMessage>) -> Result<()> {
        let mut lock = self.lock().unwrap();
        *lock = items;

        Ok(())
    }
}
