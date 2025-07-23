use std::any::Any;

use async_trait::async_trait;

use super::{
    errors::NodeError,
    node::{NodeId, TaskNode},
};

pub(crate) struct Transition<T: TaskNode> {
    pub(crate) node: Box<dyn TaskNode<Input = T::Input, Output = T::Output, Error = T::Error>>,
    pub(crate) node_id: NodeId<T>,
    pub(crate) r#fn: Box<dyn Fn(T::Output) -> TransitionPayload + Send + Sync>,
    pub(crate) is_set: bool,
}

#[derive(Debug)]
pub struct TransitionPayload {
    // If we make this an enum instead, we can support spawning many nodes as well
    pub(crate) node_id: usize,
    pub(crate) context: Box<dyn Any + Send>,
}

impl TransitionPayload {
    pub fn new<T: TaskNode + 'static>(node_id: &NodeId<T>, context: T::Input) -> Self {
        TransitionPayload {
            node_id: node_id.id,
            context: Box::new(context),
        }
    }
}

#[async_trait]
pub(crate) trait AnyNodeTransition: Any + Send + Sync {
    fn transition_is_set(&self) -> bool;

    async fn evaluate(&self, context: Box<dyn Any + Send>) -> Result<TransitionPayload, NodeError>;

    fn node_id(&self) -> usize;
}

#[async_trait]
impl<T: TaskNode + 'static> AnyNodeTransition for Transition<T> {
    async fn evaluate(&self, context: Box<dyn Any + Send>) -> Result<TransitionPayload, NodeError> {
        let context = context.downcast::<T::Input>().unwrap();

        match self.node.evaluate(&context).await {
            Ok(output) => Ok((self.r#fn)(output)),
            Err(error) => Err(NodeError::new(error, self.node_id.id, None)), // node_id will be set by caller
        }
    }

    fn transition_is_set(&self) -> bool {
        self.is_set
    }

    fn node_id(&self) -> usize {
        self.node_id.id
    }
}
