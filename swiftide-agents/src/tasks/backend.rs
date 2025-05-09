//! A task backend is responsible for spawning and managing agents.
//!
//! The default backend spawns agents as tokio tasks, with optional limited concurrency.
use async_trait::async_trait;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::errors::AgentError;

use super::{running_agent::RunningAgent, TaskError};

#[async_trait]
pub trait Backend: Clone + Send + Sync + 'static {
    async fn spawn_agent(&self, agent: RunningAgent, instructions: &str) -> CancellationToken;
    async fn join_all(&self) -> Result<(), TaskError>;
    async fn join_next(&self) -> Result<(), TaskError>;
    async fn abort(&self);
}

/// A backend that tracks active tasks with an atomic counter + Notify,
/// and optionally limits max concurrency via a Semaphore.
#[derive(Clone, Debug)]
pub struct DefaultBackend {
    outstanding: Arc<AtomicUsize>,
    notify: Arc<Notify>,
    semaphore: Arc<Semaphore>, // cap on concurrent agents
    cancel_token: CancellationToken,
    first_error: Arc<Mutex<Option<AgentError>>>,
}

impl DefaultBackend {
    /// `max_concurrent = None` ⇒ unbounded; otherwise caps at `n`.
    pub fn new(max_concurrent: Option<usize>) -> Self {
        let cap = max_concurrent.unwrap_or(Semaphore::MAX_PERMITS);
        DefaultBackend {
            outstanding: Arc::new(AtomicUsize::new(0)),
            notify: Arc::new(Notify::new()),
            semaphore: Arc::new(Semaphore::new(cap)),
            cancel_token: CancellationToken::new(),
            first_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_max_concurrent(&mut self, max_concurrent: usize) -> &mut Self {
        self.semaphore = Arc::new(Semaphore::new(max_concurrent));
        self
    }
}

impl Default for DefaultBackend {
    fn default() -> Self {
        DefaultBackend::new(None)
    }
}

#[async_trait]
impl Backend for DefaultBackend {
    /// Spawn an agent
    ///
    /// Returns a cancellation token that can be used to cancel the agent.
    ///
    /// Note that if the backend drops or is aborted, all agents are cancelled.
    #[tracing::instrument(skip(self, agent, instructions))]
    async fn spawn_agent(&self, agent: RunningAgent, instructions: &str) -> CancellationToken {
        // 1) Acquire one permit (awaits if we're already at max_concurrent).
        let permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("Semaphore closed");

        // 2) Bump the “outstanding” counter.
        self.outstanding.fetch_add(1, Ordering::SeqCst);

        // 3) Create a child cancel token. If the backend cancels, all childs are cancelled as
        //    well.
        let cancel_token = self.cancel_token.child_token();
        let notify = self.notify.clone();
        let outstanding = self.outstanding.clone();
        let instructions = instructions.to_string();
        let agent_cancel_token = cancel_token.clone();
        let first_error = self.first_error.clone();

        // 4) Spawn the real work, moving in the permit so it isn’t
        //    released until the future completes or is aborted.
        tokio::spawn(async move {
            // hold permit until this async block finishes:
            let _permit = permit;

            // run the agent, but allow aborts
            let work = async {
                let mut lock = agent.lock().await;
                lock.query(instructions).await
            };

            tokio::select! {
                biased;
                () = agent_cancel_token.cancelled() => {
                    tracing::warn!("Agent {} aborted", agent.name());
                }
                result = work => {
                    match result {
                        Ok(()) => tracing::info!("Agent {} completed", agent.name()),
                        Err(e)  => { first_error.lock().await.replace(e); },
                    }
                }
            }

            // 5) Task is definitely done — drop the permit, decrement counter, notify joiners.
            outstanding.fetch_sub(1, Ordering::SeqCst);
            notify.notify_waiters();
        });

        cancel_token
    }

    /// Returns a handle that is resolved when all agents are done.
    ///
    /// Agents are run immediately, and this function returns a handle that is resolved when all agents are done.
    #[tracing::instrument(skip(self))]
    async fn join_all(&self) -> Result<(), TaskError> {
        let outstanding = self.outstanding.clone();
        let notify = self.notify.clone();
        let first_error = self.first_error.clone();

        loop {
            if let Some(e) = { first_error.lock().await.take() } {
                return Err(TaskError::AgentError(e));
            }

            if outstanding.load(Ordering::SeqCst) == 0 {
                tracing::info!("All agents completed");
                return Ok(());
            }

            notify.notified().await;
        }
    }

    #[tracing::instrument(skip(self))]
    async fn join_next(&self) -> Result<(), TaskError> {
        // If there already is an error, return it immediately.
        if let Some(e) = self.first_error.lock().await.take() {
            return Err(TaskError::AgentError(e));
        }

        if self.outstanding.load(Ordering::SeqCst) == 0 {
            return Ok(());
        }

        self.notify.notified().await;

        // If the last notification was due to an error, return it.
        if let Some(e) = self.first_error.lock().await.take() {
            return Err(TaskError::AgentError(e));
        }

        Ok(())
    }

    async fn abort(&self) {
        self.cancel_token.cancel();
    }
}

impl Drop for DefaultBackend {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}
