//! A `Task` facilitaties running multiple agents in sequence or in parallel.
//!
//! Agents can delegate work to each other, and the task keeps track of the work.
//!
//! A task takes a list of agents, a list of actions that the agents can take.
//!
//! It is also possible to hook into various lifecycle stages of the task.
//!
//! # Example
//! TODO: no_run me when this works
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
use std::sync::atomic::{self, AtomicUsize};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::task::{AbortHandle, JoinSet};

use super::action::{Action, ActionError};
use super::running_agent::RunningAgent;

// TODO:
// - [ ] What if the agent is already running
// - [ ] deadlockdouble check
// - [ ] Check if possible to run in parallel (fine if not in current)
// - [ ] Double check after all changes tokio lock variants are still needed
// - [ ] Maybe store the abort handle optionally in the running agent?
//
#[derive(Builder, Clone, Debug)]
#[builder(build_fn(skip, error = TaskBuilderError))]
pub struct Task {
    #[builder(field(ty = "Option<Vec<RunningAgent>>"), setter(custom))]
    agents: Arc<tokio::sync::RwLock<Vec<RunningAgent>>>,
    #[builder(field(ty = "Option<Vec<Action>>"))]
    actions: Arc<Vec<Action>>,
    #[builder(setter(custom))]
    starts_with: Arc<String>,
    state: Arc<TaskState>,
    #[builder(private, default)]
    current_agent: Arc<AtomicUsize>,

    #[builder(private, default = Arc::new(Mutex::new(JoinSet::new())))]
    // All spawned agents
    running_agents: Arc<std::sync::Mutex<JoinSet<Result<(), TaskError>>>>,
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

impl TaskBuilder {
    pub fn with(&mut self, action: impl Into<Action>) -> &mut Self {
        self.actions
            .get_or_insert_with(Vec::new)
            .push(action.into());
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

    pub async fn build(&mut self) -> Result<Task, TaskBuilderError> {
        // TODO: Validate that all names are unique
        let agents = self
            .agents
            .clone()
            .ok_or(TaskBuilderError::UninitializedField("agents"))?;

        let starts_with = self
            .starts_with
            .clone()
            .ok_or(TaskBuilderError::UninitializedField("starts_with"))?;

        let current_agent = agents
            .iter()
            .position(|agent| agent.name() == starts_with.as_str())
            .ok_or(TaskBuilderError::ValidationError(
                "Could not find starting agent in agents".to_string(),
            ))?;

        let task = Task {
            agents: Arc::new(tokio::sync::RwLock::new(agents)),
            actions: Arc::new(self.actions.clone().unwrap_or_default()),
            current_agent: Arc::new(current_agent.into()),
            starts_with,
            state: Arc::new(TaskState::Pending),
            running_agents: Arc::new(Mutex::new(JoinSet::new())),
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
}

impl Task {
    /// Build a new task
    pub fn builder() -> TaskBuilder {
        TaskBuilder::default()
    }

    /// Queries the current active agent with the given instructions and waits for all agents to
    /// complete
    /// TODO: Maybe return a stop reason, ie from the last agent that ran?
    /// Should also return an abort handle on the full join set
    /// Naming: Maybe invoke_blocking and invoke?
    /// Should probably take a `Prompt`
    /// How can we avoid agents calling this, as it will deadlock
    #[tracing::instrument(skip(self))]
    pub async fn invoke(&self, instructions: &str) -> Result<(), TaskError> {
        let current_agent = self.current_agent().await.ok_or(TaskError::NoActiveAgent)?;

        self.spawn_agent(current_agent, instructions);
        self.join_all().await?;

        Ok(())
    }

    /// Queries the current active agent without waiting for the result.
    /// TODO: Should probably take a `Prompt`
    #[tracing::instrument(skip(self))]
    pub async fn query_current(&self, instructions: &str) -> Result<(), TaskError> {
        let current_agent = self.current_agent().await.ok_or(TaskError::NoActiveAgent)?;

        self.spawn_agent(current_agent, instructions);

        Ok(())
    }

    /// Awaits for all agents to complete
    #[tracing::instrument(skip(self))]
    pub async fn join_all(&self) -> Result<(), TaskError> {
        let join_set = {
            // Swap the existing join set with a new one, then join all tasks
            std::mem::replace(&mut *self.running_agents.lock().unwrap(), JoinSet::new())
        };
        join_set.join_all().await;

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub(crate) async fn swap_active_agent(&self, agent: &RunningAgent) -> Result<(), TaskError> {
        let locked_agents = self.agents.write().await;
        let agent_index = locked_agents
            .iter()
            .position(|a| a == agent)
            .ok_or(TaskError::NoActiveAgent)?;

        self.current_agent
            .store(agent_index, atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Spawns an agent with instructions, non-blocking onto the current join set
    fn spawn_agent(&self, agent: RunningAgent, instructions: &str) -> AbortHandle {
        let instructions = instructions.to_string();

        // Clone the task to avoid lifetime issues
        let cloned_task = self.clone();

        let running_agents = self.running_agents.clone();
        let mut join_set = running_agents.lock().unwrap();

        join_set.spawn(async move { cloned_task.query_agent(agent, &instructions).await })
    }

    /// Retrieves a copy of an agent by name
    pub(crate) async fn find_agent(&self, name: &str) -> Option<RunningAgent> {
        self.agents
            .read()
            .await
            .iter()
            .find(|agent| agent.name() == name)
            .cloned()
    }

    pub(crate) async fn current_agent(&self) -> Option<RunningAgent> {
        let current_agent_index = self.current_agent.load(atomic::Ordering::Relaxed);
        self.agents.read().await.get(current_agent_index).cloned()
    }

    /// Finds an agent by name and queries it with the given instructions
    ///
    /// Intended to be spawned on the internal joinset
    async fn query_agent(&self, agent: RunningAgent, instructions: &str) -> Result<(), TaskError> {
        let mut lock = agent.lock().await;
        lock.query(instructions)
            .await
            .map_err(TaskError::AgentError)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub enum TaskState {
    #[default]
    Pending,
    Running,
    Completed,
}
