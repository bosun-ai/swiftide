//! Trait contracts used by task graphs.

use std::{any::Any, future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use dyn_clone::DynClone;

use super::{
    errors::TaskError,
    node::NodeId,
    task::Task,
    transition::{JoinDefinition, MarkedTransition, Transition},
};

/// A value that can flow into or out of a [`TaskNode`].
///
/// Task inputs, outputs, transition payloads, and join payloads all use this bound so they can be
/// moved safely across async task execution.
pub trait NodeArg: Send + Sync + DynClone + 'static {}

impl<T: Send + Sync + std::fmt::Debug + 'static + Clone> NodeArg for T {}

/// A typed step in a [`Task`](crate::tasks::Task).
///
/// Implement this trait for your own domain-specific nodes when you want full control over how a
/// task step runs. For lightweight nodes, use
/// [`Task::register_node_fn`](crate::tasks::Task::register_node_fn). For async closures, use
/// [`Task::register_node_async_fn`](crate::tasks::Task::register_node_async_fn) or
/// [`AsyncFn`](crate::tasks::AsyncFn).
#[async_trait]
pub trait TaskNode: Send + Sync + DynClone + Any {
    /// The input accepted by this node.
    type Input: NodeArg;
    /// The output produced by this node.
    type Output: NodeArg;
    /// The error returned when evaluation fails.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Evaluates the node with the current input.
    async fn evaluate(
        &self,
        node_id: &DynNodeId<Self>,
        input: &Self::Input,
    ) -> Result<Self::Output, Self::Error>;
}

/// Type-erased [`NodeId`] for the same input, output, and error types as `T`.
pub type DynNodeId<T> = NodeId<
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

#[doc(hidden)]
pub trait RegisterTransition<From: TaskNode + ?Sized>: 'static {
    #[doc(hidden)]
    fn register<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError>;
}

#[doc(hidden)]
pub trait RegisterTransitionAsync<From: TaskNode + ?Sized>: 'static {
    #[doc(hidden)]
    fn register_async<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError>;
}

pub(super) trait TransitionResult<From: TaskNode + ?Sized> {
    fn into_transition(self) -> Transition;
}

impl<From, To> TransitionResult<From> for MarkedTransition<To>
where
    From: TaskNode + 'static + ?Sized,
    To: TaskNode<Input = From::Output> + ?Sized,
{
    fn into_transition(self) -> Transition {
        self.into_inner()
    }
}

impl<From> TransitionResult<From> for Transition
where
    From: TaskNode + 'static + ?Sized,
{
    fn into_transition(self) -> Transition {
        self
    }
}

pub(crate) type BoxedTransitionFuture = Pin<Box<dyn Future<Output = Transition> + Send>>;
pub(crate) type TransitionHandler<Output> =
    Arc<dyn Fn(Output) -> BoxedTransitionFuture + Send + Sync + 'static>;
pub(crate) type BoxedJoinFuture = Pin<Box<dyn Future<Output = Arc<dyn Any + Send + Sync>> + Send>>;
pub(crate) type JoinHandler<Output> =
    Arc<dyn Fn(Output) -> BoxedJoinFuture + Send + Sync + 'static>;

pub(crate) enum RegisteredTransition<Output> {
    Missing,
    Flow(TransitionHandler<Output>),
    Join {
        definition: JoinDefinition,
        handler: JoinHandler<Output>,
    },
}

impl<Output> Clone for RegisteredTransition<Output> {
    fn clone(&self) -> Self {
        match self {
            RegisteredTransition::Missing => Self::Missing,
            RegisteredTransition::Flow(handler) => Self::Flow(handler.clone()),
            RegisteredTransition::Join {
                definition,
                handler,
            } => Self::Join {
                definition: *definition,
                handler: handler.clone(),
            },
        }
    }
}

impl<Output> std::fmt::Debug for RegisteredTransition<Output> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisteredTransition::Missing => f.write_str("RegisteredTransition::Missing"),
            RegisteredTransition::Flow(_) => f.write_str("RegisteredTransition::Flow(..)"),
            RegisteredTransition::Join { definition, .. } => f
                .debug_struct("RegisteredTransition::Join")
                .field("definition", definition)
                .finish_non_exhaustive(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum EvaluatedTransition {
    Flow(Transition),
    Join {
        definition: JoinDefinition,
        payload: Arc<dyn Any + Send + Sync>,
    },
}

/// Type-erased node executor used by the runtime.
#[async_trait]
pub(crate) trait AnyNodeExecutor: Any + Send + Sync + std::fmt::Debug + DynClone {
    fn node_as_any(&self) -> &dyn Any;

    fn transition_is_set(&self) -> bool;

    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<EvaluatedTransition, TaskError>;
}

dyn_clone::clone_trait_object!(AnyNodeExecutor);
