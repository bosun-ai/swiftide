use std::{any::Any, pin::Pin, sync::Arc};

use async_trait::async_trait;
use dyn_clone::DynClone;

use super::{
    errors::NodeError,
    node::{NodeArg, NodeId, TaskNode},
};

pub trait TransitionFn<Input: Send + Sync>:
    for<'a> Fn(Input) -> Pin<Box<dyn Future<Output = TransitionPayload> + Send>> + Send + Sync
{
}

// dyn_clone::clone_trait_object!(<Input> TransitionFn<Input>);

impl<Input: Send + Sync, F> TransitionFn<Input> for F where
    F: for<'a> Fn(Input) -> Pin<Box<dyn Future<Output = TransitionPayload> + Send>> + Send + Sync
{
}

pub(crate) struct Transition<
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
> {
    pub(crate) node: Box<dyn TaskNode<Input = Input, Output = Output, Error = Error> + Send + Sync>,
    pub(crate) node_id: Box<NodeId<dyn TaskNode<Input = Input, Output = Output, Error = Error>>>,
    // pub(crate) r#fn: Arc<dyn Fn(Output) -> TransitionPayload + Send + Sync>,
    pub(crate) r#fn: Arc<dyn TransitionFn<Output> + Send>,
    pub(crate) is_set: bool,
}

impl<Input, Output, Error> Clone for Transition<Input, Output, Error>
where
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Transition {
            node: self.node.clone(),
            node_id: self.node_id.clone(),
            r#fn: self.r#fn.clone(),
            is_set: self.is_set,
        }
    }
}

impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    std::fmt::Debug for Transition<Input, Output, Error>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transition")
            .field("node_id", &self.node_id.id)
            .field("is_set", &self.is_set)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct NextNode {
    // If we make this an enum instead, we can support spawning many nodes as well
    pub(crate) node_id: usize,
    pub(crate) context: Arc<dyn Any + Send + Sync>,
}

impl NextNode {
    pub fn new<T: TaskNode + ?Sized>(node_id: NodeId<T>, context: T::Input) -> Self
    where
        <T as TaskNode>::Input: 'static,
    {
        let context = Arc::new(context) as Arc<dyn Any + Send + Sync>;

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
    Error(Box<dyn std::error::Error + Send + Sync>),
}

impl TransitionPayload {
    pub fn next_node<T: TaskNode + ?Sized>(node_id: &NodeId<T>, context: T::Input) -> Self {
        NextNode::new(*node_id, context).into()
    }

    pub fn pause() -> Self {
        TransitionPayload::Pause
    }

    pub fn error(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        TransitionPayload::Error(error.into())
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
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<TransitionPayload, NodeError>;

    fn node_id(&self) -> usize;
}

dyn_clone::clone_trait_object!(AnyNodeTransition);

#[async_trait]
impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    AnyNodeTransition for Transition<Input, Output, Error>
{
    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<TransitionPayload, NodeError> {
        let context = context.downcast::<Input>().unwrap();

        match self.node.evaluate(&self.node_id.as_dyn(), &context).await {
            Ok(output) => Ok((self.r#fn)(output).await),
            Err(error) => Err(NodeError::new(error, self.node_id.id, None)), /* node_id will be
                                                                              * set by caller */
        }
    }

    fn transition_is_set(&self) -> bool {
        self.is_set
    }

    fn node_id(&self) -> usize {
        self.node_id.id
    }
}
