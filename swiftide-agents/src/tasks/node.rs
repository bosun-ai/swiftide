use async_trait::async_trait;
use dyn_clone::DynClone;

use super::{
    errors::NodeError,
    transition::{MarkedTransitionPayload, TransitionPayload},
};

pub trait NodeArg: DynClone + Send + Sync {}

dyn_clone::clone_trait_object!(NodeArg);

impl<T: Clone + Send + Sync + std::fmt::Debug + 'static> NodeArg for T {}

#[derive(Debug, Clone)]
pub struct NoopNode<Context: NodeArg> {
    _marker: std::marker::PhantomData<(Context, Box<dyn std::error::Error + Send + Sync>)>,
}

impl<Context> Default for NoopNode<Context>
where
    Context: NodeArg,
{
    fn default() -> Self {
        NoopNode {
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<Context: NodeArg + Clone> TaskNode for NoopNode<Context> {
    type Output = ();
    type Input = Context;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &AnyNodeId,
        _context: &Context,
    ) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

#[async_trait]
pub trait TaskNode: Send + Sync + DynClone {
    type Input: NodeArg;
    type Output: NodeArg;
    type Error: std::error::Error + Send + Sync + 'static;

    async fn evaluate(
        &self,
        node_id: &AnyNodeId,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error>;
}

dyn_clone::clone_trait_object!(
    TaskNode<
        Input = dyn NodeArg,
        Output = dyn NodeArg,
        Error = dyn std::error::Error + Send + Sync,
    >
);

#[derive(Debug, PartialEq, Eq)]
pub struct NodeId<T: TaskNode + ?Sized> {
    pub id: usize,
    _marker: std::marker::PhantomData<T>,
}

pub type AnyNodeId = usize;

impl<T: TaskNode + 'static> NodeId<T> {
    pub fn new(id: usize, _node: &T) -> Self {
        NodeId {
            id,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn transitions_with(&self, context: T::Input) -> MarkedTransitionPayload<T> {
        MarkedTransitionPayload::new(TransitionPayload::next_node(self, context))
    }

    /// Returns the internal id of the node without the type information.
    pub fn as_any(&self) -> AnyNodeId {
        self.id
    }
}

impl<T: TaskNode> Clone for NodeId<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: TaskNode> Copy for NodeId<T> {}
