//! A running agent wraps an agent and its name, and is cheap to clone with inner mutability.
//!
//! This allows tasks to keep track of running agents without needing to lock the list of agents
//! itself.
//!
//! This allows

use std::{ops::Deref, sync::Arc};

use crate::Agent;
#[derive(Clone, Debug)]
pub struct RunningAgent(Arc<tokio::sync::Mutex<Agent>>, Arc<String>);

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
