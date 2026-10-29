use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use swiftide_agents::tools::local_executor::LocalExecutor;
use swiftide_core::{Command, CommandError, ToolExecutor};

/// Executor lifecycle and binary-download operations used by [`super::LazyExecutor`].
///
/// Local executors have no cleanup work; wrappers should delegate lifecycle and download calls.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::tools::local_executor::LocalExecutor;
/// use swiftide_agents_contrib::executors::ToolExecutorExt;
/// let executor = LocalExecutor::default();
/// assert!(executor.is_started());
/// ```
#[async_trait]
pub trait ToolExecutorExt: ToolExecutor {
    /// Whether the executor has started; ordinary executors are started by default.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tools::local_executor::LocalExecutor;
    /// use swiftide_agents_contrib::executors::ToolExecutorExt;
    /// let executor = LocalExecutor::default();
    /// assert!(executor.is_started());
    /// ```
    fn is_started(&self) -> bool {
        true
    }

    /// Release the executor's resources. A lazy executor delegates only after startup.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tools::local_executor::LocalExecutor;
    /// use swiftide_agents_contrib::executors::ToolExecutorExt;
    /// # async fn example() -> anyhow::Result<()> {
    /// LocalExecutor::default().cleanup().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the underlying executor's cleanup error.
    async fn cleanup(&self) -> Result<()>;

    /// Streams a file out of the executor to a local path without holding it in memory.
    ///
    /// Binary safe and checksum verified; returns the number of bytes written, and fails
    /// before writing when the file is larger than `max_bytes`. The default works over shell
    /// commands on every executor. Wrappers that rewrite command output must delegate to
    /// their inner executor so the payload is never altered.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use swiftide_agents::tools::local_executor::LocalExecutor;
    /// use swiftide_agents_contrib::executors::ToolExecutorExt;
    /// # async fn example() -> anyhow::Result<()> {
    /// LocalExecutor::default().download(Path::new("report.bin"), Path::new("copy.bin"), 1_000_000).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error for an oversized file, failed command, local write failure, or checksum
    /// mismatch.
    async fn download(&self, remote: &Path, local: &Path, max_bytes: u64) -> Result<u64> {
        super::download::download(self, remote, local, max_bytes, super::download::CHUNK_SIZE).await
    }
}

/// Stderr of a failed command kept in its error message, in bytes. The message travels
/// to whoever asked for the run, so it carries the tail that says what went wrong rather
/// than however much the command chose to print.
const STDERR_IN_ERROR: usize = 4 << 10;

/// Runs `command` and returns its stdout only, so stderr noise never reaches a payload; a
/// non-zero exit fails with the last [`STDERR_IN_ERROR`] bytes of its stderr, and stdout
/// over `max_bytes` fails instead of being returned. The limit bounds what a caller gets
/// back, not what the executor gathers: the executor holds the whole output of the command
/// before it is checked.
pub(super) async fn stdout_of<E: ToolExecutor + ?Sized>(
    executor: &E,
    command: Command,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    match executor.exec_cmd(&command).await {
        Ok(output) => {
            let size: usize = output.stdout().map(tokio_util::bytes::Bytes::len).sum();
            anyhow::ensure!(
                size <= max_bytes,
                "the command printed {size} bytes, more than the {max_bytes} expected"
            );
            Ok(output
                .stdout()
                .flat_map(|bytes| bytes.iter().copied())
                .collect())
        }
        Err(CommandError::NonZeroExit(output)) => {
            let stderr: Vec<u8> = output
                .stderr()
                .flat_map(|bytes| bytes.iter().copied())
                .collect();
            anyhow::bail!("command exited non-zero: {}", tail_of(&stderr))
        }
        Err(err) => Err(err.into()),
    }
}

/// The last [`STDERR_IN_ERROR`] bytes of `stderr` as text, said to be cut when it was.
fn tail_of(stderr: &[u8]) -> String {
    let from = stderr.len().saturating_sub(STDERR_IN_ERROR);
    let tail = String::from_utf8_lossy(&stderr[from..]).trim().to_owned();
    if from == 0 {
        tail
    } else {
        format!("…{tail}")
    }
}

#[async_trait]
impl ToolExecutorExt for LocalExecutor {
    async fn cleanup(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl<E> ToolExecutorExt for swiftide_core::ScopedExecutor<E>
where
    E: ToolExecutorExt + Clone,
{
    fn is_started(&self) -> bool {
        self.inner().is_started()
    }

    async fn cleanup(&self) -> Result<()> {
        self.inner().cleanup().await
    }

    /// Resolves a relative `remote` against the scope, then delegates so the inner executor's
    /// own `download` (for example the redaction bypass) applies.
    async fn download(&self, remote: &Path, local: &Path, max_bytes: u64) -> Result<u64> {
        self.inner()
            .download(&self.scope().join(remote), local, max_bytes)
            .await
    }
}
