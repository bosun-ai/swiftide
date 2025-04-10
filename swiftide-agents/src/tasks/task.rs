use crate::Agent;
use derive_builder::Builder;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Builder, Clone, Debug)]
pub struct Task {
    agents: Vec<Agent>,
    actions: Vec<Action>,
    starts_with_agent: Option<String>,
    state: TaskState,
}

impl TaskBuilder {
    pub fn agents(&mut self, agents: Vec<Agent>) -> &mut Self {
        self.agents = Some(agents);
        self
    }

    pub fn with(&mut self, action: Action) -> &mut Self {
        self.actions.get_or_insert_with(Vec::new).push(action);
        self
    }

    pub fn on_complete<F>(&mut self, _: F) -> &mut Self
    where
        F: Fn(&Task),
    {
        self
    }

    pub fn starts_with(&mut self, agent_name: &'static str) -> &mut Self {
        self.starts_with_agent = Some(agent_name.to_string());
        self
    }

    pub fn build(&self) -> Result<Task, &'static str> {
        Ok(Task {
            agents: self.agents.clone().ok_or("No agents specified")?,
            actions: self.actions.clone().unwrap_or_default(),
            starts_with_agent: self.starts_with_agent.clone(),
            state: TaskState::Pending,
        })
    }
}

impl Task {
    pub fn invoke(&self, instructions: &str) {
        // Implement invocation logic here using the given instructions
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
