use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Result;
use std::pin::Pin;
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
use swiftide_core::{Command, CommandError, CommandOutput, ToolExecutor, indexing::IndexingStream};
use tokio::sync::OnceCell;

use super::ToolExecutorExt;

type InitFn = Arc<dyn Fn() -> BoxFuture<'static, Result<Arc<dyn ToolExecutorExt>>> + Send + Sync>;

/// Defer executor startup while preserving Fluyt's command, cleanup, and download delegation.
///
/// Clones share initialization. Failed or cancelled initialization can retry. Cleanup before
/// initialization is a no-op; download initializes the executor and delegates to it.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use swiftide_agents::tools::local_executor::LocalExecutor;
/// use swiftide_agents_contrib::executors::{LazyExecutor, ToolExecutorExt};
/// let executor = LazyExecutor::new(|| async {
///     Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutorExt>)
/// });
/// assert!(!executor.is_started());
/// ```
#[derive(Clone)]
pub struct LazyExecutor {
    init: InitFn,
    executor: Arc<OnceCell<Arc<dyn ToolExecutorExt>>>,
    started: Arc<AtomicBool>,
}

impl LazyExecutor {
    /// Construct an executor without starting it.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use swiftide_agents::tools::local_executor::LocalExecutor;
    /// use swiftide_agents_contrib::executors::{LazyExecutor, ToolExecutorExt};
    /// let executor = LazyExecutor::new(|| async {
    ///     Ok(Arc::new(LocalExecutor::default()) as Arc<dyn ToolExecutorExt>)
    /// });
    /// assert!(!executor.is_started());
    /// ```
    pub fn new<F, Fut>(init: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Arc<dyn ToolExecutorExt>>> + Send + 'static,
    {
        let init = Arc::new(move || Box::pin(init()) as BoxFuture<'static, Result<_>>);
        Self {
            init,
            executor: Arc::new(OnceCell::new()),
            started: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn resolve(&self) -> Result<Arc<dyn ToolExecutorExt>> {
        let executor = self
            .executor
            .get_or_try_init(|| async {
                let executor = (self.init)().await?;
                self.started.store(true, Ordering::Release);
                Ok::<_, anyhow::Error>(executor)
            })
            .await?;
        Ok(Arc::clone(executor))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for LazyExecutor {
    async fn exec_cmd(&self, cmd: &Command) -> Result<CommandOutput, CommandError> {
        let executor = self.resolve().await.map_err(CommandError::ExecutorError)?;
        executor.exec_cmd(cmd).await
    }

    async fn exec_cmd_streaming(
        &self,
        cmd: &Command,
        output: &mut swiftide_core::CommandOutputSink<'_>,
    ) -> Result<CommandOutput, CommandError> {
        let executor = self.resolve().await.map_err(CommandError::ExecutorError)?;
        executor.exec_cmd_streaming(cmd, output).await
    }

    async fn stream_files(
        &self,
        path: &std::path::Path,
        extensions: Option<Vec<String>>,
    ) -> Result<IndexingStream<String>> {
        let executor = self.resolve().await?;
        executor.stream_files(path, extensions).await
    }
}

#[async_trait::async_trait]
impl ToolExecutorExt for LazyExecutor {
    fn is_started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    async fn cleanup(&self) -> Result<()> {
        let Some(executor) = self.executor.get() else {
            return Ok(());
        };

        executor.cleanup().await
    }

    async fn download(
        &self,
        remote: &std::path::Path,
        local: &std::path::Path,
        max_bytes: u64,
    ) -> Result<u64> {
        self.resolve()
            .await?
            .download(remote, local, max_bytes)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use swiftide_core::ScopedExecutor;

    #[derive(Clone)]
    #[allow(clippy::struct_field_names)]
    struct CountingExecutor {
        exec_calls: Arc<AtomicUsize>,
        stream_calls: Arc<AtomicUsize>,
        cleanup_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for CountingExecutor {
        async fn exec_cmd(&self, _cmd: &Command) -> Result<CommandOutput, CommandError> {
            self.exec_calls.fetch_add(1, Ordering::SeqCst);
            Ok(CommandOutput::new("ok"))
        }

        async fn stream_files(
            &self,
            _path: &std::path::Path,
            _extensions: Option<Vec<String>>,
        ) -> Result<IndexingStream<String>> {
            self.stream_calls.fetch_add(1, Ordering::SeqCst);
            Ok(IndexingStream::empty())
        }
    }

    #[async_trait::async_trait]
    impl ToolExecutorExt for CountingExecutor {
        async fn cleanup(&self) -> Result<()> {
            self.cleanup_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn lazy_executor_initializes_once() -> Result<()> {
        let init_calls = Arc::new(AtomicUsize::new(0));
        let exec_calls = Arc::new(AtomicUsize::new(0));
        let stream_calls = Arc::new(AtomicUsize::new(0));
        let cleanup_calls = Arc::new(AtomicUsize::new(0));

        let lazy = LazyExecutor::new({
            let init_calls = Arc::clone(&init_calls);
            let exec_calls = Arc::clone(&exec_calls);
            let stream_calls = Arc::clone(&stream_calls);
            let cleanup_calls = Arc::clone(&cleanup_calls);
            move || {
                let init_calls = Arc::clone(&init_calls);
                let exec_calls = Arc::clone(&exec_calls);
                let stream_calls = Arc::clone(&stream_calls);
                let cleanup_calls = Arc::clone(&cleanup_calls);
                Box::pin(async move {
                    init_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Arc::new(CountingExecutor {
                        exec_calls,
                        stream_calls,
                        cleanup_calls,
                    }) as Arc<dyn ToolExecutorExt>)
                })
            }
        });

        assert!(!lazy.is_started());
        let scoped = ScopedExecutor::new(lazy.clone(), "workspace");
        assert!(!scoped.is_started());
        assert_eq!(init_calls.load(Ordering::SeqCst), 0);

        let _ = lazy.exec_cmd(&Command::shell("echo hi")).await?;
        assert!(lazy.is_started());
        assert!(scoped.is_started());
        assert_eq!(init_calls.load(Ordering::SeqCst), 1);
        assert_eq!(exec_calls.load(Ordering::SeqCst), 1);

        let clone = lazy.clone();
        assert!(clone.is_started());
        let _ = clone.exec_cmd(&Command::shell("echo again")).await?;
        assert_eq!(init_calls.load(Ordering::SeqCst), 1);
        assert_eq!(exec_calls.load(Ordering::SeqCst), 2);

        let _ = clone.stream_files(std::path::Path::new("."), None).await?;
        assert_eq!(init_calls.load(Ordering::SeqCst), 1);
        assert_eq!(stream_calls.load(Ordering::SeqCst), 1);
        assert_eq!(cleanup_calls.load(Ordering::SeqCst), 0);

        Ok(())
    }

    #[tokio::test]
    async fn lazy_executor_cleanup_forwards_after_initialization() -> Result<()> {
        let cleanup_calls = Arc::new(AtomicUsize::new(0));
        let exec_calls = Arc::new(AtomicUsize::new(0));

        let lazy = LazyExecutor::new({
            let cleanup_calls = Arc::clone(&cleanup_calls);
            let exec_calls = Arc::clone(&exec_calls);
            move || {
                let cleanup_calls = Arc::clone(&cleanup_calls);
                let exec_calls = Arc::clone(&exec_calls);
                Box::pin(async move {
                    Ok(Arc::new(CountingExecutor {
                        exec_calls,
                        stream_calls: Arc::new(AtomicUsize::new(0)),
                        cleanup_calls,
                    }) as Arc<dyn ToolExecutorExt>)
                })
            }
        });

        let _ = lazy.exec_cmd(&Command::shell("echo hi")).await?;
        assert_eq!(exec_calls.load(Ordering::SeqCst), 1);

        lazy.cleanup().await?;
        assert_eq!(cleanup_calls.load(Ordering::SeqCst), 1);

        Ok(())
    }
}
