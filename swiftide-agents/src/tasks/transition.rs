use std::any::Any;

use async_trait::async_trait;

use super::{
    errors::NodeError,
    node::{NodeId, TaskNode},
};

// pub trait TransitionFn:
//     for<'a> Fn(&'a NodeArg) -> TransitionPayload<'a> + Send + Sync + DynClone
// {
// }

pub(crate) struct Transition<T: TaskNode> {
    pub(crate) node: T,
    pub(crate) node_id: NodeId<T>,
    pub(crate) r#fn: Box<dyn Fn(T::Output) -> TransitionPayload + Send + Sync>,
    pub(crate) is_set: bool,
}

#[derive(Debug)]
pub struct NextNode {
    // If we make this an enum instead, we can support spawning many nodes as well
    pub(crate) node_id: usize,
    pub(crate) context: Box<dyn Any + Send + Sync>,
}

impl NextNode {
    pub fn new<T: TaskNode>(node_id: NodeId<T>, context: T::Input) -> Self
    where
        <T as TaskNode>::Input: 'static,
    {
        let context = Box::new(context) as Box<dyn Any + Send + Sync>;

        NextNode {
            node_id: node_id.id,
            context,
        }
    }
}

impl From<NextNode> for TransitionPayload {
    fn from(next_node: NextNode) -> Self {
        TransitionPayload::NextNode(next_node)
    }
}

#[derive(Debug)]
pub enum TransitionPayload {
    NextNode(NextNode),
    Pause,
}

impl TransitionPayload {
    pub fn next_node<T: TaskNode + 'static>(node_id: &NodeId<T>, context: T::Input) -> Self {
        NextNode::new(*node_id, context).into()
    }

    pub fn pause() -> Self {
        TransitionPayload::Pause
    }
}

pub struct MarkedTransitionPayload<To: TaskNode>(TransitionPayload, std::marker::PhantomData<To>);

impl<To: TaskNode> MarkedTransitionPayload<To> {
    pub fn new(payload: TransitionPayload) -> Self {
        MarkedTransitionPayload(payload, std::marker::PhantomData)
    }

    pub fn into_inner(self) -> TransitionPayload {
        self.0
    }
}

impl<T: TaskNode> std::ops::Deref for MarkedTransitionPayload<T> {
    type Target = TransitionPayload;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
pub(crate) trait AnyNodeTransition: Any + Send + Sync {
    fn transition_is_set(&self) -> bool;

    async fn evaluate_next(
        &self,
        context: Box<dyn Any + Send + Sync>,
    ) -> Result<TransitionPayload, NodeError>;

    fn node_id(&self) -> usize;
}

#[async_trait]
impl<T: TaskNode + 'static> AnyNodeTransition for Transition<T>
where
    <T as TaskNode>::Input: 'static,
{
    async fn evaluate_next(
        &self,
        context: Box<dyn Any + Send + Sync>,
    ) -> Result<TransitionPayload, NodeError> {
        let context = context.downcast::<T::Input>().unwrap();

        match self.node.evaluate(&self.node_id.as_any(), &context).await {
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
