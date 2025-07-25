use std::{any::Any, sync::Arc};

use async_trait::async_trait;
use dyn_clone::DynClone;

use super::{
    errors::NodeError,
    node::{NodeArg, NodeId, TaskNode},
};

pub trait TransitionFn:
    for<'a> Fn(Box<Self::Input>) -> TransitionPayload + Send + Sync + DynClone
{
    type Input: NodeArg + 'static;
}

// impl<F, I> TransitionFn for F
// where
//     F: for<'a> Fn(&'a Self::Input) -> TransitionPayload + Send + Sync + DynClone,
//     I: NodeArg + 'static,
// {
//     type Input = I;
// }

#[derive(Clone)]
pub(crate) struct Transition<T: TaskNode + ?Sized> {
    pub(crate) node: Box<T>,
    pub(crate) node_id: NodeId<T>,
    pub(crate) r#fn: Arc<dyn Fn(T::Output) -> TransitionPayload + Send + Sync>,
    // pub(crate) r#fn: Box<dyn TransitionFn<Input = T::Input> + Send + Sync>,
    pub(crate) is_set: bool,
}

impl<T: TaskNode + ?Sized> std::fmt::Debug for Transition<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transition")
            .field("node_id", &self.node_id)
            .field("is_set", &self.is_set)
            .finish()
    }
}

#[derive(Debug)]
pub struct NextNode {
    // If we make this an enum instead, we can support spawning many nodes as well
    pub(crate) node_id: usize,
    pub(crate) context: Box<dyn Any + Send + Sync>,
}

impl NextNode {
    pub fn new<T: TaskNode + ?Sized>(node_id: NodeId<T>, context: T::Input) -> Self
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
    pub fn next_node<T: TaskNode + 'static + ?Sized>(
        node_id: &NodeId<T>,
        context: T::Input,
    ) -> Self {
        NextNode::new(*node_id, context).into()
    }

    pub fn pause() -> Self {
        TransitionPayload::Pause
    }
}

pub struct MarkedTransitionPayload<To: TaskNode + ?Sized>(
    TransitionPayload,
    std::marker::PhantomData<To>,
);

impl<To: TaskNode + ?Sized> MarkedTransitionPayload<To> {
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
pub(crate) trait AnyNodeTransition: Any + Send + Sync + std::fmt::Debug + DynClone {
    fn transition_is_set(&self) -> bool;

    async fn evaluate_next(
        &self,
        context: Box<dyn Any + Send + Sync>,
    ) -> Result<TransitionPayload, NodeError>;

    fn node_id(&self) -> usize;
}

dyn_clone::clone_trait_object!(AnyNodeTransition);

#[async_trait]
impl<T: TaskNode + Clone + 'static> AnyNodeTransition for Transition<T>
where
    <T as TaskNode>::Input: Clone,
    <T as TaskNode>::Output: Clone,
{
    async fn evaluate_next(
        &self,
        context: Box<dyn Any + Send + Sync>,
    ) -> Result<TransitionPayload, NodeError> {
        let context = context.downcast::<T::Input>().unwrap();

        match self.node.evaluate(&self.node_id.as_dyn(), &context).await {
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
