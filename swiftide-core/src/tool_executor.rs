use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{command_output::CommandOutput, indexing::IndexingStream};
use anyhow::Result;
use async_trait::async_trait;
use dyn_clone::DynClone;
use thiserror::Error;

/// A `ToolExecutor` provides an interface for agents to interact with a system
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
pub trait ToolExecutor: Send + Sync + DynClone {
    /// Execute a command in the executor
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError>;

    /// Stream files from the executor
    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream<String>>;
}

dyn_clone::clone_trait_object!(ToolExecutor);

/// Lightweight executor wrapper that applies a default working directory to forwarded commands.
///
/// Most callers should construct this via [`ExecutorExt::scoped`], which borrows the underlying
/// executor and only clones commands/paths when the scope actually changes their resolution.
#[derive(Debug, Clone)]
pub struct ScopedExecutor<E> {
    executor: E,
    scope: PathBuf,
}

impl<E> ScopedExecutor<E> {
    /// Build a new wrapper around `executor` that prefixes relative paths with `scope`.
    pub fn new(executor: E, scope: impl Into<PathBuf>) -> Self {
        Self {
            executor,
            scope: scope.into(),
        }
    }

    /// Returns either the original command or a scoped clone depending on the current directory.
    fn apply_scope<'a>(&'a self, cmd: &'a Command) -> Cow<'a, Command> {
        match cmd.current_dir_path() {
            Some(path) if path.is_absolute() || self.scope.as_os_str().is_empty() => {
                Cow::Borrowed(cmd)
            }
            Some(path) => {
                let mut scoped = cmd.clone();
                scoped.current_dir(self.scope.join(path));
                Cow::Owned(scoped)
            }
            None if self.scope.as_os_str().is_empty() => Cow::Borrowed(cmd),
            None => {
                let mut scoped = cmd.clone();
                scoped.current_dir(self.scope.clone());
                Cow::Owned(scoped)
            }
        }
    }

    /// Returns a path adjusted for the scope when the provided path is relative.
    fn scoped_path<'a>(&'a self, path: &'a Path) -> Cow<'a, Path> {
        if path.is_absolute() || self.scope.as_os_str().is_empty() {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(self.scope.join(path))
        }
    }

    /// Access the inner executor.
    pub fn inner(&self) -> &E {
        &self.executor
    }

    /// Expose the scope that will be applied to relative paths.
    pub fn scope(&self) -> &Path {
        &self.scope
    }
}

#[async_trait]
impl<E> ToolExecutor for ScopedExecutor<E>
where
    E: ToolExecutor + Send + Sync + Clone,
{
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        let scoped_cmd = self.apply_scope(cmd);
        self.executor.exec_cmd(scoped_cmd.as_ref()).await
    }

    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream<String>> {
        let scoped_path = self.scoped_path(path);
        self.executor
            .stream_files(scoped_path.as_ref(), extensions)
            .await
    }
}

/// Convenience methods for scoping executors without cloning them.
pub trait ExecutorExt {
    /// Borrow `self` and return a wrapper that resolves relative operations inside `path`.
    fn scoped(&self, path: impl Into<PathBuf>) -> ScopedExecutor<&Self>;

    fn scoped_owned(self, path: impl Into<PathBuf>) -> ScopedExecutor<Self>
    where
        Self: Sized;
}

impl<T> ExecutorExt for T
where
    T: ToolExecutor,
{
    fn scoped(&self, path: impl Into<PathBuf>) -> ScopedExecutor<&Self> {
        ScopedExecutor::new(self, path)
    }

    fn scoped_owned(self, path: impl Into<PathBuf>) -> ScopedExecutor<Self> {
        ScopedExecutor::new(self, path)
    }
}

#[async_trait]
impl<T> ToolExecutor for &T
where
    T: ToolExecutor + ?Sized,
{
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        (**self).exec_cmd(cmd).await
    }

    async fn stream_files(
        &self,
        path: &Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream<String>> {
        (**self).stream_files(path, extensions).await
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
    ) -> Result<IndexingStream<String>> {
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
    ) -> Result<IndexingStream<String>> {
        self.as_ref().stream_files(path, extensions).await
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    /// The executor itself failed
    #[error("executor error: {0:#}")]
    ExecutorError(#[from] anyhow::Error),

    /// The command exceeded its allotted time budget
    #[error("command timed out after {timeout:?}: {output}")]
    TimedOut {
        timeout: Duration,
        output: CommandOutput,
    },

    /// The command failed, i.e. failing tests with stderr. This error might be handled
    #[error("command failed with NonZeroExit: {0}")]
    NonZeroExit(CommandOutput),
}

impl From<std::io::Error> for CommandError {
    fn from(err: std::io::Error) -> Self {
        CommandError::ExecutorError(err.into())
    }
}

#[cfg(test)]
mod command_error_tests {
    use super::{CommandError, CommandOutput};

    #[test]
    fn renders_the_command_output() {
        let output = CommandOutput::new("onetwo");

        assert_eq!(
            CommandError::NonZeroExit(output).to_string(),
            "command failed with NonZeroExit: onetwo"
        );
    }

    #[test]
    fn classifies_io_errors_as_executor_errors() {
        let error = CommandError::from(std::io::Error::other("failed"));

        assert!(matches!(error, CommandError::ExecutorError(_)));
    }
}

/// Commands that can be executed by the executor
/// Conceptually, `Shell` allows any kind of input, and other commands enable more optimized
/// implementations.
///
/// There is an ongoing consideration to make this an associated type on the executor
///
/// TODO: Should be able to borrow everything?
///
/// Use the constructor helpers (e.g. [`Command::shell`]) and then chain configuration methods
/// such as [`Command::with_current_dir`] or [`Command::current_dir`] for builder-style ergonomics.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Command {
    Shell {
        command: String,
        current_dir: Option<PathBuf>,
        timeout: Option<Duration>,
    },
    ReadFile {
        path: PathBuf,
        current_dir: Option<PathBuf>,
        timeout: Option<Duration>,
    },
    WriteFile {
        path: PathBuf,
        content: String,
        current_dir: Option<PathBuf>,
        timeout: Option<Duration>,
    },
}

impl Command {
    pub fn shell<S: Into<String>>(cmd: S) -> Self {
        Command::Shell {
            command: cmd.into(),
            current_dir: None,
            timeout: None,
        }
    }

    pub fn read_file<P: Into<PathBuf>>(path: P) -> Self {
        Command::ReadFile {
            path: path.into(),
            current_dir: None,
            timeout: None,
        }
    }

    pub fn write_file<P: Into<PathBuf>, S: Into<String>>(path: P, content: S) -> Self {
        Command::WriteFile {
            path: path.into(),
            content: content.into(),
            current_dir: None,
            timeout: None,
        }
    }

    /// Override the working directory used when executing this command.
    ///
    /// Executors may interpret relative paths in the context of their own
    /// working directory.
    #[must_use]
    pub fn with_current_dir<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.current_dir(path);
        self
    }

    /// Override the working directory using the `std::process::Command`
    /// builder-lite style API.
    pub fn current_dir<P: Into<PathBuf>>(&mut self, path: P) -> &mut Self {
        let dir = Some(path.into());
        match self {
            Command::Shell { current_dir, .. }
            | Command::ReadFile { current_dir, .. }
            | Command::WriteFile { current_dir, .. } => {
                *current_dir = dir;
            }
        }
        self
    }

    pub fn clear_current_dir(&mut self) -> &mut Self {
        match self {
            Command::Shell { current_dir, .. }
            | Command::ReadFile { current_dir, .. }
            | Command::WriteFile { current_dir, .. } => {
                *current_dir = None;
            }
        }
        self
    }

    pub fn current_dir_path(&self) -> Option<&Path> {
        match self {
            Command::Shell { current_dir, .. }
            | Command::ReadFile { current_dir, .. }
            | Command::WriteFile { current_dir, .. } => current_dir.as_deref(),
        }
    }

    /// Override the timeout used when executing this command.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout(timeout);
        self
    }

    /// Override the timeout using the builder-style API.
    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        match self {
            Command::Shell { timeout: slot, .. }
            | Command::ReadFile { timeout: slot, .. }
            | Command::WriteFile { timeout: slot, .. } => {
                *slot = Some(timeout);
            }
        }
        self
    }

    /// Remove any timeout previously configured on this command.
    pub fn clear_timeout(&mut self) -> &mut Self {
        match self {
            Command::Shell { timeout, .. }
            | Command::ReadFile { timeout, .. }
            | Command::WriteFile { timeout, .. } => {
                *timeout = None;
            }
        }
        self
    }

    /// Returns the timeout associated with this command, if any.
    pub fn timeout_duration(&self) -> Option<&Duration> {
        match self {
            Command::Shell { timeout, .. }
            | Command::ReadFile { timeout, .. }
            | Command::WriteFile { timeout, .. } => timeout.as_ref(),
        }
    }
}
