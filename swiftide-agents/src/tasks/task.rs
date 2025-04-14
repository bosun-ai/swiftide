//! A `Task` facilitaties running multiple agents in sequence or in parallel.
//!
//! Agents can delegate work to each other, and the task keeps track of the work.
//!
//! A task takes a list of agents, a list of actions that the agents can take.
//!
//! It is also possible to hook into various lifecycle stages of the task.
use crate::errors::AgentError;
use crate::Agent;
use derive_builder::Builder;
use std::collections::HashSet;
use std::ops::Deref;
use std::sync::atomic::{self, AtomicUsize};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::task::{AbortHandle, JoinSet};

use super::action::Action;

// TODO:
// - [ ] What if the agent is already running
// - [ ] deadlockdouble check
// - [ ] Check if possible to run in parallel (fine if not in current)
//
#[derive(Builder, Clone, Debug)]
#[builder(build_fn(skip))]
pub struct Task {
    #[builder(field(ty = "Option<Vec<RunningAgent>>"))]
    agents: Arc<tokio::sync::RwLock<Vec<RunningAgent>>>,
    #[builder(field(ty = "Option<Vec<Action>>"))]
    actions: Arc<Vec<Action>>,
    starts_with: Arc<String>,
    state: Arc<TaskState>,
    #[builder(private, default)]
    current_agent: Arc<AtomicUsize>,

    #[builder(private, default = Arc::new(Mutex::new(JoinSet::new())))]
    // All spawned agents
    running_agents: Arc<std::sync::Mutex<JoinSet<Result<(), TaskError>>>>,
}

// TODO: Maybe handle or work with cancel tokens?
#[derive(Clone, Debug)]
pub(crate) struct RunningAgent(Arc<tokio::sync::Mutex<Agent>>, Arc<String>);

impl RunningAgent {
    pub fn name(&self) -> &str {
        self.1.as_str()
    }
}

impl PartialEq for RunningAgent {
    fn eq(&self, other: &Self) -> bool {
        self.1 == other.1
    }
}

impl From<Agent> for RunningAgent {
    fn from(agent: Agent) -> Self {
        // We want to be able to find the agent without using the mutex
        let name = agent.name().to_string();
        RunningAgent(Arc::new(tokio::sync::Mutex::new(agent)), Arc::new(name))
    }
}

impl Deref for RunningAgent {
    type Target = Arc<tokio::sync::Mutex<Agent>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TaskBuilder {
    pub fn with(&mut self, action: impl Into<Action>) -> &mut Self {
        self.actions
            .get_or_insert_with(Vec::new)
            .push(action.into());
        self
    }

    pub fn on_complete<F>(&mut self, _: F) -> &mut Self
    where
        F: Fn(&Task),
    {
        self
    }

    pub fn build(&mut self) -> Result<Task, TaskBuilderError> {
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

        Ok(Task {
            agents: Arc::new(tokio::sync::RwLock::new(
                agents.into_iter().map(Into::into).collect::<Vec<_>>(),
            )),
            actions: Arc::new(self.actions.clone().unwrap_or_default()),
            current_agent: Arc::new(current_agent.into()),
            starts_with,
            state: Arc::new(TaskState::Pending),
            running_agents: Arc::new(Mutex::new(JoinSet::new())),
        })
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
    /// Retrieves a copy of an agent by name
    pub(crate) async fn find_agent(&self, name: &str) -> Option<RunningAgent> {
        self.agents
            .read()
            .await
            .iter()
            .find(|agent| agent.name() == name)
            .cloned()
    }

    /// Spawns an agent with instructions, non-blocking onto the current join set
    fn spawn_agent(&self, agent_name: &str, instructions: &str) -> Result<AbortHandle, TaskError> {
        let agent_name = agent_name.to_string();
        let instructions = instructions.to_string();

        // Clone the task to avoid lifetime issues
        let cloned_task = self.clone();

        let running_agents = self.running_agents.clone();
        let mut join_set = running_agents.lock().unwrap();

        let handle = join_set
            .spawn(async move { cloned_task.query_agent(&agent_name, &instructions).await });

        Ok(handle)
    }

    async fn query_agent(&self, agent_name: &str, instructions: &str) -> Result<(), TaskError> {
        let agent = {
            let locked_agents = self.agents.read().await;

            locked_agents
                .iter()
                .find(|a| a.name() == agent_name)
                .ok_or(TaskError::MissingAgent(agent_name.to_string()))
                .cloned()
        }?;

        let mut lock = agent.lock().await;
        lock.query(instructions)
            .await
            .map_err(TaskError::AgentError)?;

        Ok(())
    }

    /// Queries the current active agent with the given instructions.
    /// TODO: Maybe return a stop reason, ie from the last agent that ran?
    pub async fn invoke(&self, instructions: &str) -> Result<(), TaskError> {
        let current_agent_index = self.current_agent.load(atomic::Ordering::Relaxed);

        let locked_agents = self.agents.read().await;
        let current_agent = locked_agents
            .get(current_agent_index)
            .ok_or(TaskError::NoActiveAgent)?
            .name();

        self.spawn_agent(current_agent, instructions);

        let join_set = {
            // Swap the existing join set with a new one, then join all tasks
            std::mem::replace(&mut *self.running_agents.lock().unwrap(), JoinSet::new())
        };
        join_set.join_all().await;

        Ok(())
    }

    /// Queries the current active agent without waiting for the result.
    pub async fn query_current(&self, instructions: &str) -> Result<(), TaskError> {
        let current_agent_index = self.current_agent.load(atomic::Ordering::Relaxed);

        let locked_agents = self.agents.read().await;
        let current_agent = locked_agents
            .get(current_agent_index)
            .ok_or(TaskError::NoActiveAgent)?
            .name();

        self.spawn_agent(current_agent, instructions)?;

        Ok(())
    }

    pub async fn swap_active_agent(&self, agent: &RunningAgent) -> Result<(), TaskError> {
        let locked_agents = self.agents.write().await;
        let agent_index = locked_agents
            .iter()
            .position(|a| a == agent)
            .ok_or(TaskError::NoActiveAgent)?;

        self.current_agent
            .store(agent_index, atomic::Ordering::Relaxed);
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
