use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{
    Command, CommandError, CommandOutput, CommandOutputSink, ToolExecutor, indexing::IndexingStream,
};
use tokio::sync::OnceCell;

type Initialization = Pin<Box<dyn Future<Output = Result<Arc<dyn ToolExecutor>>> + Send>>;

/// Initialize an executor on its first command or file-stream operation.
///
/// Clones share the same executor and serialize concurrent initialization. Failed or cancelled
/// initialization is retried by the next operation. The factory must tolerate retries and clean
/// up partial resources on failure/cancellation. This wrapper does not add a cleanup lifecycle to
/// [`ToolExecutor`]; callers retain responsibility for their underlying resources.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use swiftide_agents::tools::local_executor::LocalExecutor;
/// use swiftide_agents_contrib::executors::LazyExecutor;
/// use swiftide_core::ToolExecutor;
///
/// let executor = LazyExecutor::new(|| async {
///     Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>)
/// });
/// assert!(!executor.is_initialized());
/// ```
#[derive(Clone)]
pub struct LazyExecutor {
    initialize: Arc<dyn Fn() -> Initialization + Send + Sync>,
    executor: Arc<OnceCell<Arc<dyn ToolExecutor>>>,
}

impl LazyExecutor {
    /// Construct without starting the executor.
    pub fn new<F, Fut>(initialize: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Arc<dyn ToolExecutor>>> + Send + 'static,
    {
        Self {
            initialize: Arc::new(move || Box::pin(initialize())),
            executor: Arc::default(),
        }
    }

    /// Whether initialization has completed successfully. Does not initialize the executor.
    pub fn is_initialized(&self) -> bool {
        self.executor.initialized()
    }

    async fn resolve(&self) -> Result<&Arc<dyn ToolExecutor>> {
        self.executor.get_or_try_init(|| (self.initialize)()).await
    }
}

impl std::fmt::Debug for LazyExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyExecutor")
            .field("initialized", &self.is_initialized())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ToolExecutor for LazyExecutor {
    async fn exec_cmd(&self, command: &Command) -> Result<CommandOutput, CommandError> {
        self.resolve()
            .await
            .map_err(CommandError::ExecutorError)?
            .exec_cmd(command)
            .await
    }

    async fn exec_cmd_streaming(
        &self,
        command: &Command,
        output: &mut CommandOutputSink<'_>,
    ) -> Result<CommandOutput, CommandError> {
        self.resolve()
            .await
            .map_err(CommandError::ExecutorError)?
            .exec_cmd_streaming(command, output)
            .await
    }

    async fn stream_files(
        &self,
        path: &std::path::Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream<String>> {
        self.resolve().await?.stream_files(path, extensions).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::TryStreamExt as _;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use swiftide_agents::tools::local_executor::LocalExecutor;

    #[tokio::test]
    async fn cancelled_initialization_releases_waiters_for_retry() {
        let calls = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::clone(&calls);
        let executor = LazyExecutor::new(move || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt == 0 {
                    std::future::pending::<()>().await;
                }
                Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>)
            }
        });
        let command = Command::shell("printf retried");
        {
            let initial = executor.exec_cmd(&command);
            tokio::pin!(initial);
            assert!(futures_util::poll!(initial).is_pending());
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            executor.exec_cmd(&command),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.to_string_lossy(), "retried");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn clones_initialize_once_under_concurrent_use_and_forward_output() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let executor = LazyExecutor::new(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            async {
                tokio::task::yield_now().await;
                Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>)
            }
        });
        let clone = executor.clone();
        assert!(!executor.is_initialized());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let command = Command::shell("printf hello");
        let (left, right) = tokio::join!(executor.exec_cmd(&command), clone.exec_cmd(&command));
        assert_eq!(left.unwrap().to_string_lossy(), "hello");
        assert_eq!(right.unwrap().to_string_lossy(), "hello");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(clone.is_initialized());
        let mut chunks = Vec::new();
        let output = clone
            .exec_cmd_streaming(&command, &mut |chunk| chunks.push(chunk.clone()))
            .await
            .unwrap();
        assert_eq!(output.to_string_lossy(), "hello");
        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    async fn failed_initialization_retries_on_a_file_stream_operation() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let directory = temp_dir::TempDir::new().unwrap();
        std::fs::write(directory.path().join("one.txt"), "contents").unwrap();
        let executor = LazyExecutor::new(move || {
            let attempt = seen.fetch_add(1, Ordering::SeqCst);
            async move {
                anyhow::ensure!(attempt > 0, "try again");
                Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutor>)
            }
        });
        assert!(executor.exec_cmd(&Command::shell("true")).await.is_err());
        assert!(!executor.is_initialized());
        let files: Vec<_> = executor
            .stream_files(directory.path(), Some(vec!["txt".into()]))
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert!(executor.is_initialized());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
