use std::any::Any;

use async_trait::async_trait;
use dyn_clone::DynClone;

use super::{
    errors::NodeError,
    transition::{MarkedTransitionPayload, TransitionPayload},
};

pub trait NodeArg: Send + Sync + DynClone + 'static {}

impl<T: Send + Sync + std::fmt::Debug + 'static + Clone> NodeArg for T {}

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
        _node_id: &DynNodeId<Self>,
        _context: &Context,
    ) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

#[async_trait]
pub trait TaskNode: Send + Sync + DynClone + Any {
    type Input: NodeArg;
    type Output: NodeArg;
    type Error: std::error::Error + Send + Sync + 'static;

    async fn evaluate(
        &self,
        node_id: &DynNodeId<Self>,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error>;
}

type DynNodeId<T> = NodeId<
    dyn TaskNode<
            Input = <T as TaskNode>::Input,
            Output = <T as TaskNode>::Output,
            Error = <T as TaskNode>::Error,
        >,
>;

dyn_clone::clone_trait_object!(
    TaskNode<
        Input = dyn NodeArg,
        Output = dyn NodeArg,
        Error = dyn std::error::Error + Send + Sync,
    >
);

#[async_trait]
impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static> TaskNode
    for Box<dyn TaskNode<Input = Input, Output = Output, Error = Error>>
{
    type Input = Input;
    type Output = Output;
    type Error = Error;

    async fn evaluate(
        &self,
        node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        self.as_ref().evaluate(node_id, input).await
    }
}

dyn_clone::clone_trait_object!(<Input, Output, Error> TaskNode<Input = Input, Output = Output, Error = Error>);

#[derive(PartialEq, Eq)]
pub struct NodeId<T: TaskNode + ?Sized> {
    pub id: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T: TaskNode + ?Sized> std::fmt::Debug for NodeId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let type_name = std::any::type_name::<T>();

        write!(f, "NodeId<{type_name}>({})", self.id)
    }
}

pub type AnyNodeId = usize;

impl<T: TaskNode + 'static + ?Sized> NodeId<T> {
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

    pub fn as_dyn(
        self,
    ) -> NodeId<dyn TaskNode<Input = T::Input, Output = T::Output, Error = T::Error>> {
        NodeId {
            id: self.id,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: TaskNode + ?Sized> Clone for NodeId<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: TaskNode + ?Sized> Copy for NodeId<T> {}
