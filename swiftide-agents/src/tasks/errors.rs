use std::{any::Any, sync::Arc};

use super::transition::Transition;

/// Errors returned while defining, running, or resuming a task.
#[derive(thiserror::Error, Debug)]
pub enum TaskError {
    /// A registered node failed while executing.
    #[error(transparent)]
    NodeError(#[from] NodeError),

    /// A node was executed without an outgoing transition.
    #[error("MissingTransition: {0}")]
    MissingTransition(String),

    /// A transition referenced a node that does not exist.
    #[error("MissingNode: {0}")]
    MissingNode(String),

    /// The task produced a final output with the wrong type.
    #[error("Task failed with wrong output")]
    TypeError(String),

    /// `run` was called while the task still had live runtime state.
    #[error("Task already has active or paused work")]
    TaskActive,

    /// `resume` was called without paused or reset work to continue.
    #[error("Task has no paused or queued work to resume")]
    NotResumable,

    /// The task drained all work without finishing or pausing.
    #[error("Task ended without completing or pausing")]
    Incomplete,

    /// The task runtime detected an internal inconsistency.
    #[error("Task entered an invalid internal state: {0}")]
    InvalidState(String),

    /// A node was executed without the required input.
    #[error("MissingInput: {0}")]
    MissingInput(String),

    /// A node completed without the expected output.
    #[error("MissingOutput: {0}")]
    MissingOutput(String),

    /// The task does not have a valid start node and transition graph.
    #[error("Task is missing steps")]
    NoSteps,
}

impl TaskError {
    /// Creates a missing-transition error for `node_id`.
    pub fn missing_transition(node_id: usize) -> Self {
        TaskError::MissingTransition(format!("Node {node_id} is missing a transition"))
    }

    /// Creates a missing-node error for `node_id`.
    pub fn missing_node(node_id: usize) -> Self {
        TaskError::MissingNode(format!("Node {node_id} is missing"))
    }

    /// Creates a missing-input error for `node_id`.
    pub fn missing_input(node_id: usize) -> Self {
        TaskError::MissingInput(format!("Node {node_id} is missing input"))
    }

    /// Creates a missing-output error for `node_id`.
    pub fn missing_output(node_id: usize) -> Self {
        TaskError::MissingOutput(format!("Node {node_id} is missing output"))
    }

    /// Creates a type error describing the unexpected final output type.
    pub fn type_error<T: Any + Send>(output: &T) -> Self {
        let message = format!(
            "Expected output of type {}, but got {:?}",
            std::any::type_name::<T>(),
            output.type_id()
        );
        TaskError::TypeError(message)
    }

    /// Creates an invalid-state error with a custom message.
    pub fn invalid_state(message: impl Into<String>) -> Self {
        TaskError::InvalidState(message.into())
    }
}

/// Wraps an error produced by a specific node execution.
#[derive(Debug, thiserror::Error)]
pub struct NodeError {
    /// The original node error.
    pub node_error: Box<dyn std::error::Error + Send + Sync>,
    /// The transition that would have been applied after this node, when available.
    pub transition: Option<Arc<Transition>>,
    /// The numeric identifier of the failing node.
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
    /// Creates a new node error tied to `node_id`.
    pub fn new(
        node_error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
        node_id: usize,
        transition: Option<Transition>,
    ) -> Self {
        Self {
            node_error: node_error.into(),
            transition: transition.map(Arc::new),
            node_id,
        }
    }
}
