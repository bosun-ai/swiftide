use std::any::Any;

use super::transition::TransitionPayload;

#[derive(thiserror::Error, Debug)]
pub enum TaskError {
    #[error(transparent)]
    NodeError(#[from] NodeError),

    #[error("MissingTransition: {0}")]
    MissingTransition(String),

    #[error("MissingNode: {0}")]
    MissingNode(String),

    #[error("Task failed with wrong output: {0:?}")]
    TypeError(Box<dyn Any>),
}

impl TaskError {
    pub fn missing_transition(node_id: usize) -> Self {
        TaskError::MissingTransition(format!("Node {node_id} is missing a transition"))
    }

    pub fn missing_node(node_id: usize) -> Self {
        TaskError::MissingNode(format!("Node {node_id} is missing"))
    }

    pub fn type_error<T: Any>(output: T) -> Self {
        TaskError::TypeError(Box::new(output))
    }
}

#[derive(Debug, thiserror::Error)]
pub struct NodeError {
    pub node_error: Box<dyn std::error::Error + Send + Sync>,
    pub transition_payload: Option<TransitionPayload>,
    pub node_id: usize,
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node error in node {}: {:?}",
            self.node_id, self.node_error
        )
    }
}

impl NodeError {
    pub fn new<E: std::error::Error + Send + Sync + 'static>(
        node_error: E,
        node_id: usize,
        transition_payload: Option<TransitionPayload>,
    ) -> Self {
        Self {
            node_error: Box::new(node_error),
            transition_payload,
            node_id,
        }
    }
}
