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
//!     Transition::fan_out([left.target_with(value), right.target_with(value)])
//!         .join_with(join.join())
//! })?;
//! task.register_transition(left, join.join())?;
//! task.register_transition(right, join.join())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::{any::Any, marker::PhantomData, sync::Arc};

use super::node::{NodeArg, NodeId, TaskNode};

/// Identifies a concrete runtime branch within a task run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(pub usize);

/// Describes a branch that is still queued or paused inside the task runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveBranch {
    /// The runtime branch identifier.
    pub branch_id: BranchId,
    /// The numeric identifier of the node the branch will resume at.
    pub node_id: usize,
}

/// Controls whether newly scheduled work should run one branch at a time or concurrently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ConcurrencyModel {
    /// Run one branch to completion before starting the next runnable branch.
    #[default]
    Sequential,
    /// Allow multiple runnable branches to execute at the same time.
    Parallel,
}

/// Controls what should happen when a branch requests a pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum PauseBehavior {
    /// Let already-runnable work continue before returning a paused task.
    #[default]
    DrainRunnable,
    /// Stop scheduling new work as soon as a branch pauses the task.
    PauseTask,
}

/// Determines when a join node becomes ready to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinPolicy {
    /// Wait for all registered join branches.
    All,
    /// Run the join once at least `count` branches are ready.
    AtLeast {
        /// The minimum number of ready branches required before the join fires.
        count: usize,
        /// What should happen to the remaining branches once the join has fired.
        leftovers: JoinLeftoverBehavior,
    },
}

impl JoinPolicy {
    pub(crate) fn leftover_behavior(self) -> Option<JoinLeftoverBehavior> {
        match self {
            JoinPolicy::All => None,
            JoinPolicy::AtLeast { leftovers, .. } => Some(leftovers),
        }
    }
}

/// Controls what happens to unfinished branches after an `AtLeast` join fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinLeftoverBehavior {
    /// Cancel unfinished branches that belong to the join.
    CancelRemaining,
    /// Keep unfinished branches running after the join has already fired.
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct TransitionSettings {
    pub(crate) concurrency_model: Option<ConcurrencyModel>,
    pub(crate) pause_behavior: Option<PauseBehavior>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EffectiveTransitionSettings {
    pub(crate) concurrency_model: ConcurrencyModel,
    pub(crate) pause_behavior: PauseBehavior,
}

impl EffectiveTransitionSettings {
    pub(crate) fn with_overrides(self, overrides: TransitionSettings) -> Self {
        Self {
            concurrency_model: overrides
                .concurrency_model
                .unwrap_or(self.concurrency_model),
            pause_behavior: overrides.pause_behavior.unwrap_or(self.pause_behavior),
        }
    }
}

/// A concrete next-step target used by [`Transition::next`] and [`Transition::fan_out`].
#[derive(Debug, Clone)]
pub struct NextNode {
    pub(crate) node_id: usize,
    pub(crate) context: Arc<dyn Any + Send + Sync>,
}

impl NextNode {
    /// Creates a next-step target for the given node and input value.
    pub fn new<T: TaskNode + ?Sized>(node_id: NodeId<T>, context: T::Input) -> Self
    where
        <T as TaskNode>::Input: 'static,
    {
        Self {
            node_id: node_id.id(),
            context: Arc::new(context) as Arc<dyn Any + Send + Sync>,
        }
    }
}

impl From<NextNode> for Transition {
    fn from(next_node: NextNode) -> Self {
        Transition::next(next_node)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct JoinDefinition {
    pub(crate) join_node_id: usize,
    pub(crate) policy: JoinPolicy,
    pub(crate) settings: TransitionSettings,
}

/// A configured join destination for branches that should converge into a join node.
///
/// Build a `JoinTarget` from a join node with [`NodeId::join`](crate::tasks::NodeId::join),
/// [`NodeId::join_at_least`](crate::tasks::NodeId::join_at_least), or
/// [`NodeId::join_with`](crate::tasks::NodeId::join_with), and then register it through
/// [`Task::register_transition`](crate::tasks::Task::register_transition).
#[must_use]
pub struct JoinTarget<T: TaskNode<Input = JoinInput> + ?Sized> {
    pub(crate) definition: JoinDefinition,
    _marker: PhantomData<T>,
}

/// Builder for `JoinPolicy::AtLeast`.
pub struct AtLeastJoin<T: TaskNode<Input = JoinInput> + ?Sized> {
    node_id: NodeId<T>,
    count: usize,
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
    pub(crate) fn new(node_id: NodeId<T>, policy: JoinPolicy) -> Self {
        Self {
            definition: JoinDefinition {
                join_node_id: node_id.id(),
                policy,
                settings: TransitionSettings::default(),
            },
            _marker: PhantomData,
        }
    }

    pub(crate) fn into_definition(self) -> JoinDefinition {
        self.definition
    }

    /// Overrides the concurrency model for the join branch that will be scheduled.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.definition.settings.concurrency_model = Some(concurrency_model);
        self
    }

    /// Overrides pause behavior for the join branch that will be scheduled.
    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.definition.settings.pause_behavior = Some(pause_behavior);
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
    ///     Transition::fan_out([branch.target_with(value)])
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
    ///     Transition::fan_out([branch.target_with(value)])
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

impl<T: TaskNode<Input = JoinInput> + ?Sized> AtLeastJoin<T> {
    pub(crate) fn new(node_id: NodeId<T>, count: usize) -> Self {
        Self { node_id, count }
    }

    /// Builds an `at least N` join that cancels the remaining branches once the join fires.
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
    ///     Ok(input.ready_values::<i32>().into_iter().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out([left.target_with(value), right.target_with(value)])
    ///         .join_with(join.join_at_least(1).cancel_remaining())
    /// })?;
    /// task.register_transition(left, join.join_at_least(1).cancel_remaining())?;
    /// task.register_transition(right, join.join_at_least(1).cancel_remaining())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn cancel_remaining(self) -> JoinTarget<T> {
        JoinTarget::new(
            self.node_id,
            JoinPolicy::AtLeast {
                count: self.count,
                leftovers: JoinLeftoverBehavior::CancelRemaining,
            },
        )
    }

    /// Builds an `at least N` join that lets the remaining branches continue running.
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
    ///     Ok(input.ready_values::<i32>().into_iter().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out([left.target_with(value), right.target_with(value)])
    ///         .join_with(join.join_at_least(1).continue_remaining())
    /// })?;
    /// task.register_transition(left, join.join_at_least(1).continue_remaining())?;
    /// task.register_transition(right, join.join_at_least(1).continue_remaining())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn continue_remaining(self) -> JoinTarget<T> {
        JoinTarget::new(
            self.node_id,
            JoinPolicy::AtLeast {
                count: self.count,
                leftovers: JoinLeftoverBehavior::Continue,
            },
        )
    }
}

impl<T: TaskNode<Input = JoinInput> + ?Sized, F> MappedJoinTarget<T, F> {
    /// Overrides the concurrency model for the join branch that will be scheduled.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.join_target = self.join_target.concurrency_model(concurrency_model);
        self
    }

    /// Overrides pause behavior for the join branch that will be scheduled.
    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.join_target = self.join_target.pause_behavior(pause_behavior);
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
///     Transition::fan_out([left.target_with(value), right.target_with(value)])
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
    pub(crate) settings: TransitionSettings,
}

/// A linear transition that preserves the destination node type.
#[must_use]
pub struct MarkedTransition<To: TaskNode + ?Sized>(Transition, PhantomData<To>);

impl<To: TaskNode + ?Sized> MarkedTransition<To> {
    /// Wraps a transition while preserving the destination node type.
    pub fn new(transition: Transition) -> Self {
        Self(transition, PhantomData)
    }

    /// Returns the underlying untyped transition.
    pub fn into_inner(self) -> Transition {
        self.0
    }

    /// Overrides the concurrency model for work scheduled by this transition.
    pub fn concurrency_model(self, concurrency_model: ConcurrencyModel) -> Self {
        Self::new(self.0.concurrency_model(concurrency_model))
    }

    /// Overrides pause behavior for work scheduled by this transition.
    pub fn pause_behavior(self, pause_behavior: PauseBehavior) -> Self {
        Self::new(self.0.pause_behavior(pause_behavior))
    }
}

impl<To: TaskNode + ?Sized> From<MarkedTransition<To>> for Transition {
    fn from(marked_transition: MarkedTransition<To>) -> Self {
        marked_transition.into_inner()
    }
}

/// A fan-out transition builder that must be connected to a join before it can run.
///
/// `Transition::fan_out` returns this builder instead of a runnable [`Transition`] so every branch
/// set has an explicit join policy. This keeps task completion structured and avoids accidental
/// "first branch wins" behavior when multiple branches can finish independently.
#[derive(Debug)]
#[must_use]
pub struct FanOutTransition {
    targets: Vec<NextNode>,
    settings: TransitionSettings,
}

impl FanOutTransition {
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
            settings: self.settings,
        }
    }

    /// Overrides the concurrency model for work scheduled by this fan-out.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.settings.concurrency_model = Some(concurrency_model);
        self
    }

    /// Overrides pause behavior for work scheduled by this fan-out.
    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.settings.pause_behavior = Some(pause_behavior);
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
    /// Continues execution at the provided next-step target.
    ///
    /// This is the low-level entry point behind [`NodeId::transitions_with`].
    pub fn next(next_node: NextNode) -> Self {
        Self {
            action: TransitionAction::Next(next_node),
            settings: TransitionSettings::default(),
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
    /// task.register_transition(start, move |value| Transition::next_node(&finish, value))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn next_node<T: TaskNode + ?Sized>(node_id: &NodeId<T>, context: T::Input) -> Self {
        NextNode::new(*node_id, context).into()
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
    ///     Transition::fan_out([left.target_with(value), right.target_with(value)])
    ///         .join_with(join.join())
    /// })?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn fan_out(targets: impl IntoIterator<Item = NextNode>) -> FanOutTransition {
        FanOutTransition {
            targets: targets.into_iter().collect(),
            settings: TransitionSettings::default(),
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
            settings: TransitionSettings::default(),
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
            settings: TransitionSettings::default(),
        }
    }

    pub(crate) fn finish<T: NodeArg>(output: T) -> Self {
        Self {
            action: TransitionAction::Finish(Arc::new(output) as Arc<dyn Any + Send + Sync>),
            settings: TransitionSettings::default(),
        }
    }

    /// Overrides the concurrency model for work scheduled by this transition.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.settings.concurrency_model = Some(concurrency_model);
        self
    }

    /// Overrides pause behavior for work scheduled by this transition.
    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.settings.pause_behavior = Some(pause_behavior);
        self
    }
}

/// The aggregated view of branches that have reached a join node.
#[derive(Debug, Clone)]
pub struct JoinInput {
    branches: Vec<BranchEnvelope>,
}

impl JoinInput {
    pub(crate) fn new(branches: Vec<BranchEnvelope>) -> Self {
        Self { branches }
    }

    /// Iterates over every branch tracked by this join input in a stable order.
    pub fn iter(&self) -> std::slice::Iter<'_, BranchEnvelope> {
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
    ///     Transition::fan_out([left.target_with(value), right.target_with(value)])
    ///         .join_with(join.join())
    /// })?;
    /// task.register_transition(left, join.join())?;
    /// task.register_transition(right, join.join())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn ready_values<T: NodeArg>(&self) -> Vec<&T> {
        self.iter()
            .filter_map(BranchEnvelope::ready_value::<T>)
            .collect()
    }
}

impl<'a> IntoIterator for &'a JoinInput {
    type Item = &'a BranchEnvelope;
    type IntoIter = std::slice::Iter<'a, BranchEnvelope>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Describes one branch that belongs to a [`JoinInput`].
#[derive(Debug, Clone)]
pub struct BranchEnvelope {
    /// The runtime branch identifier.
    pub branch_id: BranchId,
    /// The numeric identifier of the node that produced this branch result.
    pub node_id: usize,
    /// The branch's current state from the join's point of view.
    pub outcome: BranchOutcome,
}

impl BranchEnvelope {
    /// Returns the ready value when the branch completed with a payload of type `T`.
    pub fn ready_value<T: NodeArg>(&self) -> Option<&T> {
        self.outcome.ready_value()
    }
}

/// The state of a branch as observed by a join node.
#[derive(Debug, Clone)]
pub enum BranchOutcome {
    /// The branch completed and produced a payload.
    Ready(Arc<dyn Any + Send + Sync>),
    /// The branch has not completed yet.
    Pending,
    /// The branch paused before completing.
    Paused,
    /// The branch was cancelled before completing.
    Cancelled,
    /// The branch completed after the join had already fired.
    LateArrival,
}

impl BranchOutcome {
    /// Returns the ready value when the outcome is [`BranchOutcome::Ready`] and the payload is `T`.
    pub fn ready_value<T: NodeArg>(&self) -> Option<&T> {
        match self {
            BranchOutcome::Ready(value) => value.downcast_ref::<T>(),
            _ => None,
        }
    }
}
