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
//!     Ok(input.iter::<i32>().copied().sum())
//! });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| {
//!     // `join_with` defines the branch group and the join node that waits for it.
//!     Transition::fan_out(&branch, value)
//!         .join_with(join.join())
//! })?;
//! // Branches still decide where their own output goes before the join can run.
//! task.register_transition(branch, join.join())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use super::traits::TaskNode;
use super::transition::{JoinInput, JoinTarget, MarkedTransition, Transition};

/// A typed handle to a registered node in a [`Task`](crate::tasks::Task).
///
/// `NodeId` keeps the node's type information so transitions can be expressed without manual
/// downcasts. Use [`NodeId::transitions_with`] for the common linear case,
/// [`Transition::fan_out`](crate::tasks::Transition::fan_out) when building static fan-out
/// transitions, and [`NodeId::join`] for join nodes.
#[derive(PartialEq, Eq)]
pub struct NodeId<T: TaskNode + ?Sized> {
    id: usize,
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
        Transition::next(self, context)
    }
}

impl<T> NodeId<T>
where
    T: TaskNode<Input = JoinInput> + ?Sized,
{
    /// Creates a join target that waits for every branch in the fan-out group.
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
    ///     Ok(input.iter::<i32>().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out(&branch, value)
    ///         .join_with(join.join())
    /// })?;
    /// task.register_transition(branch, join.join())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn join(&self) -> JoinTarget<T> {
        JoinTarget::new(*self)
    }
}

impl<T: TaskNode + 'static + ?Sized> NodeId<T> {
    /// Creates a typed node identifier for an already-registered node.
    pub(crate) fn new(id: usize, _node: &T) -> Self {
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
