use crate::errors::AgentError;
use crate::Agent;
use derive_builder::Builder;
use thiserror::Error;
use tokio::task::{AbortHandle, JoinSet};
use std::collections::HashSet;
use std::sync::atomic::{self, AtomicUsize};
use std::sync::{Arc, Mutex};

use super::action::Action;

#[derive(Builder, Clone, Debug)]
#[builder(build_fn(skip))]
pub struct Task {
    #[builder(field(ty = "Option<Vec<Agent>>"))]
    agents: Arc<Mutex<Vec<Agent>>>,
    #[builder(field(ty = "Option<Vec<Action>>"))]
    actions: Arc<Vec<Action>>,
    starts_with: Arc<String>,
    state: Arc<TaskState>,
    #[builder(private, default)]
    current_agent: Arc<AtomicUsize>,

    #[builder(private, default = Arc::new(Mutex::new(JoinSet::new())))]
    // All spawned agents
    running_agents: Arc<Mutex<JoinSet<Result<(), AgentError>>>>
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
        let agents = self
            .agents
            .clone()
            .ok_or(TaskBuilderError::UninitializedField("agents"))?;

        let starts_with =  self
            .starts_with
            .clone()
            .ok_or(TaskBuilderError::UninitializedField("starts_with"))?;

        let current_agent = agents.iter().position(|agent| agent.name() == starts_with.as_str())
            .clone()
            .ok_or(TaskBuilderError::ValidationError("Could not find starting agent in agents".to_string()))?;

        Ok(Task {
            agents: Arc::new(Mutex::new(agents)),
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
    AgentError(#[from] AgentError)
}

impl Task {
    async fn query_agent(&self, agent_name: &str, instructions: &str) -> Result<AbortHandle, TaskError> {
        let mut locked_agents = self.agents.lock().unwrap();
        let agent = locked_agents.iter_mut().find(|a| a.name() == agent_name).ok_or(TaskError::MissingAgent(agent_name.to_string()))?;


        Ok(handle)
    }
    pub async fn invoke(&self, instructions: &str) -> Result<(), TaskError> {
        // Would be even cooler if this spawns the agent in a thread/task
        // Or a joinset for all agents?

        let mut locked_agents = self.agents.lock().unwrap();
        let agent = locked_agents.get_mut(self.current_agent.load(atomic::Ordering::Relaxed)).ok_or(TaskError::NoActiveAgent)?;

        let _ = agent.query(instructions).await?;

        Ok(())
    }

    pub async fn swap_active_agent(&self, agent: &str) -> Result<(), TaskError> {
        let mut locked_agents = self.agents.lock().unwrap();
        let agent_index = locked_agents.iter().position(|a| a.name() == agent).ok_or(TaskError::NoActiveAgent)?;

        self.current_agent.store(agent_index, atomic::Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
}

impl Default for TaskState {
    fn default() -> Self {
        TaskState::Pending
    }
}

#[cfg(test)]
mod tests {
    fn test_builder_
}
