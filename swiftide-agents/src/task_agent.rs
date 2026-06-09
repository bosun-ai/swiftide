use std::sync::Arc;

use async_trait::async_trait;
use swiftide_core::prompt::Prompt;
use swiftide_tasks::{NodeId, TaskNode};
use tokio::sync::Mutex;

use crate::{Agent, errors::AgentError};

/// Wraps an [`Agent`](crate::Agent) so it can participate in a task graph.
///
/// For more control you can always implement [`TaskNode`] directly.
///
/// `TaskAgent` keeps the wrapped agent behind a mutex and therefore serializes access when the
/// same adapter instance is shared across multiple task branches.
#[derive(Clone, Debug)]
pub struct TaskAgent(Arc<Mutex<Agent>>);

impl From<Agent> for TaskAgent {
    fn from(agent: Agent) -> Self {
        Self(Arc::new(Mutex::new(agent)))
    }
}

#[async_trait]
impl TaskNode for TaskAgent {
    type Input = Prompt;
    type Output = ();
    type Error = AgentError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.0.lock().await.query(input.clone()).await
    }
}
