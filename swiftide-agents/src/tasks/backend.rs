//! A task backend is responsible for spawning and managing agents.
//!
//! WARN: If you implement a new backend, make sure it is cheap to clone (i.e. inner Arcs),
//! as the task (or users of) _will_ clone to do cool stuff.
//!
//! The default backend spawns agents as tokio tasks, with optional limited concurrency.
use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    sync::{Mutex, Notify, OwnedSemaphorePermit, Semaphore},
    task::yield_now,
};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument as _, info_span};

use crate::{StopReason, errors::AgentError};

use super::{TaskError, running_agent::RunningAgent};

#[async_trait]
pub trait Backend: Clone + Send + Sync + 'static {
    type Error: std::error::Error;

    async fn spawn_agent(
        &self,
        agent: RunningAgent,
        maybe_instructions: Option<&str>,
    ) -> CancellationToken;
    async fn join_all(&self) -> Result<(), Self::Error>;
    async fn join_next(&self) -> Result<(), Self::Error>;
    async fn abort(&mut self);

    /// Returns the number of agents that are currently running.
    fn outstanding(&self) -> usize;
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
    type Error = TaskError;

    /// Spawn an agent
    ///
    /// Returns a cancellation token that can be used to cancel the agent.
    ///
    /// Note that if the backend drops or is aborted, all agents are cancelled.
    #[tracing::instrument(skip(self, agent, instructions))]
    async fn spawn_agent(
        &self,
        agent: RunningAgent,
        instructions: Option<&str>,
    ) -> CancellationToken {
        // 1) Acquire one permit (awaits if we're already at max_concurrent).
        let permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("Semaphore closed");

        // 2) Bump the “outstanding” counter.
        self.outstanding.fetch_add(1, Ordering::SeqCst);

        // 3) Create a child cancel token. If the backend cancels, all children are cancelled as well.
        let cancel_token = self.cancel_token.child_token();
        let notify = self.notify.clone();
        let outstanding = self.outstanding.clone();
        let instructions = instructions.map(str::to_string);
        let agent_cancel_token = cancel_token.clone();
        let first_error = self.first_error.clone();

        // 4) Spawn the real work, moving in the permit so it isn’t released until the future
        //    completes or is aborted.
        // let agent_span =
        //     info_span!("agent", "otel.name" = format!("agent.{}", agent.name())).or_current();

        tokio::spawn(async move {
            // hold permit until this async block finishes:
            let _permit = permit;

            // run the agent, but allow aborts
            let work = async {
                let mut lock = agent.lock().await;
                lock.run_agent(instructions, false).await
            };

            tokio::select! {
                biased;
                () = agent_cancel_token.cancelled() => {
                    // TODO: Verify I don't deadlock
                    let mut lock = agent.lock().await;
                    lock.stop(StopReason::TaskAborted).await;

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

    /// Waits for all agents to complete, returning the first error if any agent failed.
    #[tracing::instrument(skip(self))]
    async fn join_all(&self) -> Result<(), Self::Error> {
        loop {
            yield_now().await;

            if let Some(e) = { self.first_error.lock().await.take() } {
                return Err(TaskError::AgentError(e));
            }

            if self.outstanding.load(Ordering::SeqCst) == 0 {
                tracing::info!("All agents completed");
                return Ok(());
            }

            self.notify.notified().await;
        }
    }

    #[tracing::instrument(skip(self))]
    async fn join_next(&self) -> Result<(), Self::Error> {
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

    async fn abort(&mut self) {
        self.cancel_token.cancel();
        self.cancel_token = CancellationToken::new();
    }

    fn outstanding(&self) -> usize {
        self.outstanding.load(Ordering::Relaxed)
    }
}
