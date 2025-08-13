use std::{any::Any, sync::Arc};

use super::transition::TransitionPayload;

#[derive(thiserror::Error, Debug)]
pub enum TaskError {
    #[error(transparent)]
    NodeError(#[from] NodeError),

    #[error("MissingTransition: {0}")]
    MissingTransition(String),

    #[error("MissingNode: {0}")]
    MissingNode(String),

    #[error("Task failed with wrong output")]
    TypeError(String),

    #[error("MissingInput: {0}")]
    MissingInput(String),

    #[error("MissingOutput: {0}")]
    MissingOutput(String),

    #[error("Task is missing steps")]
    NoSteps,
}

impl TaskError {
    pub fn missing_transition(node_id: usize) -> Self {
        TaskError::MissingTransition(format!("Node {node_id} is missing a transition"))
    }

    pub fn missing_node(node_id: usize) -> Self {
        TaskError::MissingNode(format!("Node {node_id} is missing"))
    }

    pub fn missing_input(node_id: usize) -> Self {
        TaskError::MissingInput(format!("Node {node_id} is missing input"))
    }

    pub fn missing_output(node_id: usize) -> Self {
        TaskError::MissingOutput(format!("Node {node_id} is missing output"))
    }

    pub fn type_error<T: Any + Send>(output: &T) -> Self {
        let message = format!(
            "Expected output of type {}, but got {:?}",
            std::any::type_name::<T>(),
            output.type_id()
        );
        TaskError::TypeError(message)
    }
}

#[derive(Debug, thiserror::Error)]
pub struct NodeError {
    pub node_error: Box<dyn std::error::Error + Send + Sync>,
    pub transition_payload: Option<Arc<TransitionPayload>>,
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
    pub fn new(
        node_error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
        node_id: usize,
        transition_payload: Option<TransitionPayload>,
    ) -> Self {
        Self {
            node_error: node_error.into(),
            transition_payload: transition_payload.map(Arc::new),
            node_id,
        }
    }
}
