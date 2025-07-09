//! A `Task` facilitaties running multiple agents in sequence or in parallel.
//!
//! Agents can delegate work to each other, and the task keeps track of the work.
//!
//! A task takes a list of agents, a list of actions that the agents can take.
//!
//! It is also possible to hook into various lifecycle stages of the task.
//!
//! # Example
//! TODO: `no_run` me when this works
//!
//! ```ignore
//! Task::builder()
//!     .agents(vec![
//!               Agent::builder().name("agent1").build()?,
//!               Agent::builder().name("agent2").build()?
//!             ])
//!     .starts_with("agent1")
//!     .with(Action::for_agent("agent1").delegates_to("agent2").and_back())
//!     .with(Action::for_agent("agent2").can_complete())
//!     .invoke("Do a task thing")
//!     .await?;
//! ```
use crate::errors::AgentError;
use derive_builder::{Builder, UninitializedFieldError};
use std::sync::Arc;
use std::sync::atomic::{self, AtomicUsize};
use thiserror::Error;
use tokio::sync::RwLock;

use super::action::{Action, ActionError};
use super::backend::{Backend, DefaultBackend};
use super::running_agent::RunningAgent;

/// Marker (for now) trait for a mutable state that agents and hooks can use to progress the task.
///
/// A task state must always be owned.
pub trait TaskState: Send + Sync + Clone + 'static {}

/// Implementations for some common types
///
/// Allow everything that is `Send + Sync + Clone + 'static` to be used as a task state.
impl<T> TaskState for T where T: Send + Sync + Clone + 'static {}

#[derive(Builder, Clone, Debug)]
#[builder(build_fn(skip, error = TaskBuilderError))]
pub struct Task<B: Backend = DefaultBackend, S: TaskState = ()> {
    #[builder(field(ty = "Option<Vec<RunningAgent>>"), setter(custom))]
    agents: Arc<RwLock<Vec<RunningAgent>>>,
    #[builder(field(ty = "Option<Vec<Action>>"), setter(custom))]
    actions: Arc<Vec<Action>>,
    #[builder(setter(custom))]
    starts_with: Arc<String>,

    #[builder(setter(custom))]
    state: S,
    #[builder(private, default)]
    current_agent: Arc<AtomicUsize>,

    #[builder(setter(custom))]
    backend: B,
}

#[derive(Error, Debug)]
pub enum TaskBuilderError {
    #[error("Uninitialized field: {0}")]
    UninitializedField(&'static str),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error(transparent)]
    ActionError(#[from] ActionError),
}

impl From<UninitializedFieldError> for TaskBuilderError {
    fn from(err: UninitializedFieldError) -> Self {
        TaskBuilderError::UninitializedField(err.field_name())
    }
}

impl<B: Backend, S: TaskState> TaskBuilder<B, S> {
    #[must_use]
    pub fn backend<N: Backend>(self, backend: N) -> TaskBuilder<N, S> {
        TaskBuilder {
            agents: self.agents,
            actions: self.actions,
            starts_with: self.starts_with,
            state: self.state,
            current_agent: self.current_agent,
            backend: Some(backend),
        }
    }

    #[must_use]
    pub fn state<N: TaskState>(self, state: N) -> TaskBuilder<B, N> {
        TaskBuilder {
            agents: self.agents,
            actions: self.actions,
            starts_with: self.starts_with,
            state: Some(state),
            current_agent: self.current_agent,
            backend: self.backend,
        }
    }

    pub fn with(&mut self, action: impl Into<Action>) -> &mut Self {
        self.actions
            .get_or_insert_with(Vec::new)
            .push(action.into());
        self
    }

    pub fn actions<I, ACTION>(&mut self, actions: I) -> &mut Self
    where
        I: IntoIterator<Item = ACTION>,
        ACTION: Into<Action>,
    {
        self.actions
            .get_or_insert_with(Vec::new)
            .extend(actions.into_iter().map(Into::into));
        self
    }

    pub fn starts_with(&mut self, starts_with: impl Into<String>) -> &mut Self {
        self.starts_with = Some(Arc::new(starts_with.into()));
        self
    }

    pub fn agents<I, AGENT>(&mut self, agents: I) -> &mut Self
    where
        I: IntoIterator<Item = AGENT>,
        AGENT: Into<RunningAgent>,
    {
        self.agents
            .get_or_insert_with(Vec::new)
            .extend(agents.into_iter().map(Into::into));
        self
    }

    pub async fn build(&mut self) -> Result<Task<B, S>, TaskBuilderError> {
        // TODO: Validate that all names are unique
        let agents = self
            .agents
            .take()
            .ok_or(TaskBuilderError::UninitializedField("agents"))?;

        let starts_with = self
            .starts_with
            .take()
            .ok_or(TaskBuilderError::UninitializedField("starts_with"))?;

        let current_agent = agents
            .iter()
            .position(|agent| agent.name() == starts_with.as_str())
            .ok_or(TaskBuilderError::ValidationError(
                "Could not find starting agent in agents".to_string(),
            ))?;

        let state = self
            .state
            .take()
            .ok_or(TaskBuilderError::UninitializedField("state"))?;

        let backend = self
            .backend
            .take()
            .ok_or(TaskBuilderError::UninitializedField("backend"))?;

        let task = Task {
            agents: Arc::new(RwLock::new(agents)),
            actions: Arc::new(self.actions.clone().unwrap_or_default()),
            current_agent: Arc::new(current_agent.into()),
            starts_with,
            state,
            backend,
        };

        if let Some(actions) = self.actions.take() {
            for action in actions {
                action.apply(&task).await?;
            }
        }
        Ok(task)
    }
}

#[derive(Error, Debug)]
pub enum TaskError {
    #[error("Could not find an active agent")]
    NoActiveAgent,

    #[error("Could not find an agent with the name {0}")]
    MissingAgent(String),

    #[error(transparent)]
    AgentError(#[from] AgentError),

    #[error(transparent)]
    JoinSetError(#[from] tokio::task::JoinError),
}

impl Task {
    /// Build a new task
    pub fn builder() -> TaskBuilder<DefaultBackend, ()> {
        TaskBuilder::<DefaultBackend, ()>::default()
            .state(())
            .backend(DefaultBackend::default())
    }
}

impl<B: Backend, S: TaskState> Task<B, S> {
    /// Invokes the task, finding the current agent and querying it with the instructions
    ///
    /// Note that this is a non-blocking call, and the task will return immediately.
    ///
    /// Use `join_all` to wait for all agents to complete.
    #[tracing::instrument(skip(self), err)]
    pub async fn query(&self, instructions: &str) -> Result<(), TaskError> {
        self.backend
            .spawn_agent(self.current_agent().await?, Some(instructions))
            .await;

        Ok(())
    }

    /// Invokes the task, finding the current agent and running it
    ///
    /// Unlike `query`, this does not pass any instructions to the agent.
    ///
    /// Note that this is a non-blocking call, and the task will return immediately.
    ///
    /// Use `join_all` to wait for all agents to complete.
    #[tracing::instrument(skip(self), err)]
    pub async fn run(&self) -> Result<(), TaskError> {
        self.backend
            .spawn_agent(self.current_agent().await?, None)
            .await;

        Ok(())
    }

    /// Consumes the task and waiting for all agents to complete
    #[tracing::instrument(skip(self))]
    pub async fn join_all(&self) -> Result<(), B::Error> {
        self.backend.join_all().await
    }

    /// Forcibly aborts the task and all agents
    #[tracing::instrument(skip(self))]
    pub async fn abort(&mut self) {
        self.backend.abort().await;
    }

    #[tracing::instrument(skip(self))]
    pub async fn current_agent(&self) -> Result<RunningAgent, TaskError> {
        let current_index = self.current_agent.load(atomic::Ordering::Relaxed);
        let agents = self.agents.read().await;
        agents
            .get(current_index)
            .cloned()
            .ok_or(TaskError::NoActiveAgent)
    }

    #[tracing::instrument(skip(self))]
    /// Returns the number of agents that are currently running
    pub fn outstanding(&self) -> usize {
        self.backend.outstanding()
    }

    #[tracing::instrument(skip(self))]
    pub(crate) async fn switch_to_agent(&self, agent: &RunningAgent) -> Result<(), TaskError> {
        let locked_agents = self.agents.write().await;
        let agent_index = locked_agents
            .iter()
            .position(|a| a == agent)
            .ok_or(TaskError::NoActiveAgent)?;

        self.current_agent
            .store(agent_index, atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Find an agent by name
    pub(crate) async fn find_agent(&self, name: &str) -> Option<RunningAgent> {
        self.agents
            .read()
            .await
            .iter()
            .find(|agent| agent.name() == name)
            .cloned()
    }
}
