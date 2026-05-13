//! Typed node traits and handles for task graphs.
//!
//! This module defines the contracts for task nodes and the typed identifiers used to wire them
//! together.
//!
//! # Examples
//!
//! ```no_run
//! use swiftide_agents::tasks::{JoinInput, NodeError, Task, Transition};
//!
//! let mut task = Task::<i32, i32>::new();
//! let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
//! let branch = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
//! let join = task.register_node_fn(|input: &JoinInput| -> Result<i32, NodeError> {
//!     Ok(input.ready_values::<i32>().into_iter().copied().sum())
//! });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| {
//!     Transition::fan_out([branch.target_with(value)])
//!         .join_with(join.join())
//! })?;
//! task.register_transition(branch, join.join())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::any::Any;

use async_trait::async_trait;
use dyn_clone::DynClone;

use super::errors::NodeError;
use super::transition::{
    AtLeastJoin, JoinInput, JoinPolicy, JoinTarget, MarkedTransition, NextNode, Transition,
};

/// A value that can flow into or out of a [`TaskNode`].
///
/// Task inputs, outputs, transition payloads, and join payloads all use this bound so they can be
/// moved safely across async task execution.
pub trait NodeArg: Send + Sync + DynClone + 'static {}

impl<T: Send + Sync + std::fmt::Debug + 'static + Clone> NodeArg for T {}

/// A typed placeholder node that returns its input unchanged.
///
/// This is useful when code needs to reconstruct a typed [`NodeId`] for an already registered
/// node and only has its numeric identifier.
#[derive(Debug)]
pub struct NoopNode<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> Default for NoopNode<T> {
    fn default() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Clone for NoopNode<T> {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[async_trait]
impl<T: NodeArg + Clone> TaskNode for NoopNode<T> {
    type Input = T;
    type Output = T;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &DynNodeId<Self>,
        input: &Self::Input,
    ) -> Result<T, NodeError> {
        Ok(input.clone())
    }
}

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

/// A typed handle to a registered node in a [`Task`](crate::tasks::Task).
///
/// `NodeId` keeps the node's type information so transitions can be expressed without manual
/// downcasts. Use [`NodeId::transitions_with`] for the common linear case,
/// [`NodeId::target_with`] when building fan-out transitions, and [`NodeId::join`] for join nodes.
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

impl<T: TaskNode + ?Sized> NodeId<T> {
    /// Returns the stable numeric identifier assigned when the node was registered.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Builds a typed transition to this node with the provided input.
    ///
    /// This is the most ergonomic way to connect one node to the next in a linear task.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let finish = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input * 2) });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| finish.transitions_with(value))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn transitions_with(&self, context: T::Input) -> MarkedTransition<T> {
        MarkedTransition::new(Transition::next_node(self, context))
    }

    /// Builds a fan-out target pointing at this node with the provided input.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task, Transition};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    /// let left = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let right = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 2) });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out([left.target_with(value), right.target_with(value)])
    /// })?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn target_with(&self, context: T::Input) -> NextNode {
        NextNode::new(*self, context)
    }
}

impl<T> NodeId<T>
where
    T: TaskNode<Input = JoinInput> + ?Sized,
{
    /// Creates a join target that waits for all registered branches.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{JoinInput, NodeError, Task, Transition};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    /// let branch = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let join = task.register_node_fn(|input: &JoinInput| -> Result<i32, NodeError> {
    ///     Ok(input.ready_values::<i32>().into_iter().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out([branch.target_with(value)])
    ///         .join_with(join.join())
    /// })?;
    /// task.register_transition(branch, join.join())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn join(&self) -> JoinTarget<T> {
        self.join_with(JoinPolicy::All)
    }

    /// Starts building an `at least N` join policy.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{JoinInput, NodeError, Task, Transition};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    /// let fast = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let slow = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 2) });
    /// let join = task.register_node_fn(|input: &JoinInput| -> Result<i32, NodeError> {
    ///     Ok(input.ready_values::<i32>().into_iter().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out([fast.target_with(value), slow.target_with(value)])
    ///         .join_with(join.join_at_least(1).cancel_remaining())
    /// })?;
    /// task.register_transition(fast, join.join_at_least(1).cancel_remaining())?;
    /// task.register_transition(slow, join.join_at_least(1).cancel_remaining())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn join_at_least(&self, count: usize) -> AtLeastJoin<T> {
        AtLeastJoin::new(*self, count)
    }

    /// Creates a join target with an explicit join policy.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{
    ///     JoinInput, JoinLeftoverBehavior, JoinPolicy, NodeError, Task, Transition,
    /// };
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    /// let branch = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let join = task.register_node_fn(|input: &JoinInput| -> Result<i32, NodeError> {
    ///     Ok(input.ready_values::<i32>().into_iter().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out([branch.target_with(value)])
    ///         .join_with(join.join_with(JoinPolicy::AtLeast {
    ///             count: 1,
    ///             leftovers: JoinLeftoverBehavior::CancelRemaining,
    ///         }))
    /// })?;
    /// task.register_transition(
    ///     branch,
    ///     join.join_with(JoinPolicy::AtLeast {
    ///         count: 1,
    ///         leftovers: JoinLeftoverBehavior::CancelRemaining,
    ///     }),
    /// )?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn join_with(&self, policy: JoinPolicy) -> JoinTarget<T> {
        JoinTarget::new(*self, policy)
    }
}

impl<T: TaskNode + 'static + ?Sized> NodeId<T> {
    /// Creates a typed node identifier for an already-registered node.
    pub fn new(id: usize, _node: &T) -> Self {
        NodeId {
            id,
            _marker: std::marker::PhantomData,
        }
    }
    /// Erases the concrete node type while keeping the node's typed input and output contracts.
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
