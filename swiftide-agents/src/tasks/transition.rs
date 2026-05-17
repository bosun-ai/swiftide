//! Transition values describe how task execution should continue after a node finishes.
//!
//! This module contains the public building blocks for linear transitions, fan-out, join
//! configuration, and join payload inspection.
//!
//! # Examples
//!
//! ```no_run
//! use swiftide_agents::tasks::{JoinInput, NodeError, Task, Transition};
//!
//! let mut task = Task::<i32, i32>::new();
//! let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
//! let left = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
//! let right = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 2) });
//! let join = task.register_node_fn(|input: &JoinInput| -> Result<i32, NodeError> {
//!     Ok(input.ready_values::<i32>().into_iter().copied().sum())
//! });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| {
//!     Transition::fan_out(&left, value)
//!         .and(&right, value)
//!         .join_with(join.join())
//! })?;
//! task.register_transition(left, join.join())?;
//! task.register_transition(right, join.join())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::{any::Any, marker::PhantomData, sync::Arc};

use super::{
    node::NodeId,
    traits::{NodeArg, TaskNode},
};

/// Identifies a concrete runtime branch within a task run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BranchId(pub(crate) usize);

/// Controls whether newly scheduled work should run one branch at a time or concurrently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ConcurrencyModel {
    /// Run one branch to completion before starting the next runnable branch.
    #[default]
    Sequential,
    /// Allow multiple runnable branches to execute at the same time.
    Parallel,
}

#[derive(Debug, Clone)]
pub(crate) struct NextNode {
    pub(crate) node_id: usize,
    pub(crate) context: Arc<dyn Any + Send + Sync>,
}

impl NextNode {
    pub(crate) fn new<T: TaskNode + ?Sized>(node_id: NodeId<T>, context: T::Input) -> Self
    where
        <T as TaskNode>::Input: 'static,
    {
        Self {
            node_id: node_id.id(),
            context: Arc::new(context) as Arc<dyn Any + Send + Sync>,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct JoinDefinition {
    pub(crate) join_node_id: usize,
    pub(crate) concurrency_model: Option<ConcurrencyModel>,
}

/// A configured join destination for branches that should converge into a join node.
///
/// Build a `JoinTarget` from a join node with [`NodeId::join`](crate::tasks::NodeId::join),
/// and then register it through
/// [`Task::register_transition`](crate::tasks::Task::register_transition).
#[must_use]
pub struct JoinTarget<T: TaskNode<Input = JoinInput> + ?Sized> {
    pub(crate) definition: JoinDefinition,
    _marker: PhantomData<T>,
}

/// A join target with a synchronous payload mapping step.
#[must_use]
pub struct MappedJoinTarget<T: TaskNode<Input = JoinInput> + ?Sized, F> {
    pub(crate) join_target: JoinTarget<T>,
    pub(crate) map: F,
}

/// A join target with an asynchronous payload mapping step.
pub type AsyncMappedJoinTarget<T, F> = MappedJoinTarget<T, F>;

impl<T: TaskNode<Input = JoinInput> + ?Sized> JoinTarget<T> {
    pub(crate) fn new(node_id: NodeId<T>) -> Self {
        Self {
            definition: JoinDefinition {
                join_node_id: node_id.id(),
                concurrency_model: None,
            },
            _marker: PhantomData,
        }
    }

    pub(crate) fn into_definition(self) -> JoinDefinition {
        self.definition
    }

    /// Overrides the concurrency model for the join branch that will be scheduled.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.definition.concurrency_model = Some(concurrency_model);
        self
    }

    /// Maps each joining branch output into the payload stored for the join node.
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
    ///     Transition::fan_out(&branch, value)
    ///         .join_with(join.join())
    /// })?;
    /// task.register_transition(branch, join.join().map(|value| value * 2))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn map<F>(self, map: F) -> MappedJoinTarget<T, F>
    where
        F: Send + Sync + 'static,
    {
        MappedJoinTarget {
            join_target: self,
            map,
        }
    }

    /// Maps each joining branch output asynchronously before storing it for the join node.
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
    ///     Transition::fan_out(&branch, value)
    ///         .join_with(join.join())
    /// })?;
    /// task.register_transition_async(branch, join.join().map_async(|value| async move { value * 2 }))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn map_async<F>(self, map: F) -> AsyncMappedJoinTarget<T, F>
    where
        F: Send + Sync + 'static,
    {
        MappedJoinTarget {
            join_target: self,
            map,
        }
    }
}

impl<T: TaskNode<Input = JoinInput> + ?Sized, F> MappedJoinTarget<T, F> {
    /// Overrides the concurrency model for the join branch that will be scheduled.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.join_target = self.join_target.concurrency_model(concurrency_model);
        self
    }
}

/// Describes how task execution should continue after a node completes.
///
/// Most transitions are created either through [`NodeId::transitions_with`] for the linear case or
/// [`Transition::fan_out`] plus [`FanOutTransition::join_with`] for branching. Use
/// [`Transition::pause`] and [`Transition::error`] when a transition closure needs to control task
/// execution directly.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::tasks::{NodeError, Task, TaskRunState, Transition};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut task = Task::<i32, i32>::new();
///
/// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
/// let left = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
/// let right = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 2) });
/// let join = task.register_node_fn(
///     |input: &swiftide_agents::tasks::JoinInput| -> Result<i32, NodeError> {
///         Ok(input.ready_values::<i32>().into_iter().copied().sum())
///     },
/// );
///
/// task.starts_with(start);
/// task.register_transition(start, move |value| {
///     Transition::fan_out(&left, value)
///         .and(&right, value)
///         .join_with(join.join())
///         .concurrency_model(swiftide_agents::tasks::ConcurrencyModel::Parallel)
/// })?;
/// task.register_transition(left, join.join())?;
/// task.register_transition(right, join.join())?;
/// task.register_transition(join, task.transitions_to_finish())?;
///
/// assert_eq!(task.run(1).await?, TaskRunState::Completed(5));
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
#[must_use]
pub struct Transition {
    pub(crate) action: TransitionAction,
    pub(crate) concurrency_model: Option<ConcurrencyModel>,
}

/// A linear transition that preserves the destination node type.
#[must_use]
pub struct MarkedTransition<To: TaskNode + ?Sized>(Transition, PhantomData<To>);

impl<To: TaskNode + ?Sized> MarkedTransition<To> {
    pub(crate) fn new(transition: Transition) -> Self {
        Self(transition, PhantomData)
    }

    pub(crate) fn into_inner(self) -> Transition {
        self.0
    }

    /// Overrides the concurrency model for work scheduled by this transition.
    pub fn concurrency_model(self, concurrency_model: ConcurrencyModel) -> Self {
        Self::new(self.0.concurrency_model(concurrency_model))
    }
}

impl<To: TaskNode + ?Sized> From<MarkedTransition<To>> for Transition {
    fn from(marked_transition: MarkedTransition<To>) -> Self {
        marked_transition.into_inner()
    }
}

/// A typed fan-out transition builder that must be connected to a join before it can run.
///
/// Build one with [`Transition::fan_out`] and add additional branch targets with
/// [`FanOutTransition::and`]. Each branch input is type-checked before the branch target is erased
/// for runtime scheduling.
#[derive(Debug)]
#[must_use]
pub struct FanOutTransition {
    targets: Vec<NextNode>,
    concurrency_model: Option<ConcurrencyModel>,
}

impl FanOutTransition {
    /// Adds another typed branch target to this fan-out.
    ///
    /// The branch input must convert into the target node's input type.
    pub fn and<T, Input>(mut self, node_id: &NodeId<T>, context: Input) -> Self
    where
        T: TaskNode + ?Sized,
        Input: Into<T::Input>,
    {
        self.targets.push(NextNode::new(*node_id, context.into()));
        self
    }

    /// Attaches every branch from this fan-out to the provided join target.
    ///
    /// This is useful when branches join after one or more intermediate nodes instead of joining
    /// immediately at their first fan-out target.
    pub fn join_with<T>(self, join_target: JoinTarget<T>) -> Transition
    where
        T: TaskNode<Input = JoinInput> + ?Sized,
    {
        Transition {
            action: TransitionAction::FanOut {
                targets: self.targets,
                join: join_target.into_definition(),
            },
            concurrency_model: self.concurrency_model,
        }
    }

    /// Overrides the concurrency model for work scheduled by this fan-out.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.concurrency_model = Some(concurrency_model);
        self
    }
}

#[derive(Debug)]
pub(crate) enum TransitionAction {
    Next(NextNode),
    FanOut {
        targets: Vec<NextNode>,
        join: JoinDefinition,
    },
    Pause,
    Error(Box<dyn std::error::Error + Send + Sync>),
    Finish(Arc<dyn Any + Send + Sync>),
}

impl Transition {
    pub(crate) fn from_next_node(next_node: NextNode) -> Self {
        Self {
            action: TransitionAction::Next(next_node),
            concurrency_model: None,
        }
    }

    /// Continues execution at `node_id` with the provided input.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task, Transition};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let finish = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input * 2) });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| Transition::next(&finish, value))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn next<T: TaskNode + ?Sized>(
        node_id: &NodeId<T>,
        context: T::Input,
    ) -> MarkedTransition<T> {
        MarkedTransition::new(Self::from_next_node(NextNode::new(*node_id, context)))
    }

    /// Builds a fan-out that schedules one or more branches from the current node output.
    ///
    /// The returned [`FanOutTransition`] must attach an explicit join with
    /// [`FanOutTransition::join_with`] before it can be returned from a task transition.
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
    /// let join = task.register_node_fn(
    ///     |input: &swiftide_agents::tasks::JoinInput| -> Result<i32, NodeError> {
    ///         Ok(input.ready_values::<i32>().into_iter().copied().sum())
    ///     },
    /// );
    ///
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out(&left, value)
    ///         .and(&right, value)
    ///         .join_with(join.join())
    /// })?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn fan_out<T, Input>(node_id: &NodeId<T>, context: Input) -> FanOutTransition
    where
        T: TaskNode + ?Sized,
        Input: Into<T::Input>,
    {
        FanOutTransition {
            targets: vec![NextNode::new(*node_id, context.into())],
            concurrency_model: None,
        }
    }

    /// Pauses the current branch.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task, TaskRunState, Transition};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut task = Task::<i32, ()>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, |_value| Transition::pause())?;
    ///
    /// assert_eq!(task.run(1).await?, TaskRunState::Paused);
    /// # Ok(())
    /// # }
    /// ```
    pub fn pause() -> Self {
        Self {
            action: TransitionAction::Pause,
            concurrency_model: None,
        }
    }

    /// Fails the current branch with the provided error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::io::Error;
    ///
    /// use swiftide_agents::tasks::{NodeError, Task, Transition};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, |_value| Transition::error(Error::other("boom")))?;
    ///
    /// assert!(task.run(1).await.is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub fn error(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            action: TransitionAction::Error(error.into()),
            concurrency_model: None,
        }
    }

    pub(crate) fn finish<T: NodeArg>(output: T) -> Self {
        Self {
            action: TransitionAction::Finish(Arc::new(output) as Arc<dyn Any + Send + Sync>),
            concurrency_model: None,
        }
    }

    /// Overrides the concurrency model for work scheduled by this transition.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.concurrency_model = Some(concurrency_model);
        self
    }
}

/// The aggregated view of branches that have reached a join node.
#[derive(Debug, Clone)]
pub struct JoinInput {
    branches: Vec<Arc<dyn Any + Send + Sync>>,
}

impl JoinInput {
    pub(crate) fn new(branches: Vec<Arc<dyn Any + Send + Sync>>) -> Self {
        Self { branches }
    }

    /// Iterates over every ready branch payload in stable branch creation order.
    pub fn iter(&self) -> std::slice::Iter<'_, Arc<dyn Any + Send + Sync>> {
        self.branches.iter()
    }

    /// Returns the ready values that can be downcast to `T`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{JoinInput, NodeError, Task, Transition};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    /// let left = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let right = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 2) });
    /// let join = task.register_node_fn(|input: &JoinInput| -> Result<i32, NodeError> {
    ///     let values = input.ready_values::<i32>();
    ///     Ok(values.into_iter().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out(&left, value)
    ///         .and(&right, value)
    ///         .join_with(join.join())
    /// })?;
    /// task.register_transition(left, join.join())?;
    /// task.register_transition(right, join.join())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn ready_values<T: NodeArg>(&self) -> Vec<&T> {
        self.iter()
            .filter_map(|value| value.downcast_ref::<T>())
            .collect()
    }
}

impl<'a> IntoIterator for &'a JoinInput {
    type Item = &'a Arc<dyn Any + Send + Sync>;
    type IntoIter = std::slice::Iter<'a, Arc<dyn Any + Send + Sync>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
