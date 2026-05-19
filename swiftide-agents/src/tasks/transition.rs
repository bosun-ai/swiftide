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
//! let join = task.register_node_fn(|input: &JoinInput<i32>| -> Result<i32, NodeError> {
//!     Ok(input.iter().copied().sum())
//! });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| {
//!     // `join_with` defines the branch group and the join node that waits for it.
//!     Transition::fan_out(&left, value)
//!         .and(&right, value)
//!         .join_with(join.join())
//! })?;
//! // Branches still decide where their own output goes before the join can run.
//! task.register_transition(left, join.join())?;
//! task.register_transition(right, join.join())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::{any::Any, marker::PhantomData, sync::Arc};

use super::{
    errors::TaskError,
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

type ErasedPayload = Arc<dyn Any + Send + Sync>;

#[derive(Clone, Copy)]
enum JoinInputFactory {
    Fallible(fn(Vec<ErasedPayload>) -> Result<ErasedPayload, TaskError>),
    Infallible(fn(Vec<ErasedPayload>) -> ErasedPayload),
}

impl JoinInputFactory {
    fn build(self, payloads: Vec<ErasedPayload>) -> Result<ErasedPayload, TaskError> {
        match self {
            JoinInputFactory::Fallible(factory) => factory(payloads),
            JoinInputFactory::Infallible(factory) => Ok(factory(payloads)),
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct JoinDefinition {
    pub(crate) join_node_id: usize,
    pub(crate) concurrency_model: Option<ConcurrencyModel>,
    input_factory: JoinInputFactory,
}

impl std::fmt::Debug for JoinDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinDefinition")
            .field("join_node_id", &self.join_node_id)
            .field("concurrency_model", &self.concurrency_model)
            .finish_non_exhaustive()
    }
}

impl JoinDefinition {
    fn typed<Payload: NodeArg>(join_node_id: usize) -> Self {
        Self {
            join_node_id,
            concurrency_model: None,
            input_factory: JoinInputFactory::Fallible(typed_join_input::<Payload>),
        }
    }

    fn any(join_node_id: usize) -> Self {
        Self {
            join_node_id,
            concurrency_model: None,
            input_factory: JoinInputFactory::Infallible(any_join_input),
        }
    }

    pub(crate) fn into_input(
        self,
        payloads: Vec<ErasedPayload>,
    ) -> Result<ErasedPayload, TaskError> {
        self.input_factory.build(payloads)
    }
}

fn typed_join_input<Payload: NodeArg>(
    payloads: Vec<ErasedPayload>,
) -> Result<ErasedPayload, TaskError> {
    let branches = payloads
        .into_iter()
        .map(|payload| {
            payload.downcast::<Payload>().map_err(|_| {
                TaskError::invalid_state(format!(
                    "Join payload expected type {}",
                    std::any::type_name::<Payload>()
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Arc::new(JoinInput::<Payload>::new(branches)) as ErasedPayload)
}

fn any_join_input(payloads: Vec<ErasedPayload>) -> ErasedPayload {
    Arc::new(AnyJoinInput::new(payloads)) as ErasedPayload
}

/// A configured join destination for branches that should converge into a join node.
///
/// Build a `JoinTarget` from a join node with [`NodeId::join`](crate::tasks::NodeId::join),
/// and then register it through
/// [`Task::register_transition`](crate::tasks::Task::register_transition).
/// Users normally do not construct this type directly.
#[must_use]
pub struct JoinTarget<T: TaskNode<Input = JoinInput<Payload>> + ?Sized, Payload: NodeArg> {
    pub(crate) definition: JoinDefinition,
    _marker: PhantomData<(*const T, Payload)>,
}

/// A join target with a synchronous payload mapping step.
///
/// This is returned by [`JoinTarget::map`] and registered with
/// [`Task::register_transition`](crate::tasks::Task::register_transition).
#[must_use]
pub struct MappedJoinTarget<T: TaskNode<Input = JoinInput<Payload>> + ?Sized, Payload: NodeArg, F> {
    pub(crate) join_target: JoinTarget<T, Payload>,
    pub(crate) map: F,
}

/// A configured join destination for branches with mixed payload types.
///
/// Build an `AnyJoinTarget` from an [`AnyJoinInput`] node with
/// [`NodeId::join_any`](crate::tasks::NodeId::join_any).
#[must_use]
pub struct AnyJoinTarget<T: TaskNode<Input = AnyJoinInput> + ?Sized> {
    pub(crate) definition: JoinDefinition,
    _marker: PhantomData<T>,
}

/// A value accepted by [`FanOutTransition::join_with`].
#[doc(hidden)]
pub trait JoinDestination {
    #[doc(hidden)]
    fn into_definition(self) -> JoinDefinition;
}

impl<T, Payload> JoinDestination for JoinTarget<T, Payload>
where
    T: TaskNode<Input = JoinInput<Payload>> + ?Sized,
    Payload: NodeArg,
{
    fn into_definition(self) -> JoinDefinition {
        self.definition
    }
}

impl<T> JoinDestination for AnyJoinTarget<T>
where
    T: TaskNode<Input = AnyJoinInput> + ?Sized,
{
    fn into_definition(self) -> JoinDefinition {
        self.definition
    }
}

impl<T, Payload> JoinTarget<T, Payload>
where
    T: TaskNode<Input = JoinInput<Payload>> + ?Sized,
    Payload: NodeArg,
{
    pub(crate) fn new(node_id: NodeId<T>) -> Self {
        Self {
            definition: JoinDefinition::typed::<Payload>(node_id.id()),
            _marker: PhantomData,
        }
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
    /// let join = task.register_node_fn(|input: &JoinInput<i32>| -> Result<i32, NodeError> {
    ///     Ok(input.iter().copied().sum())
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
    pub fn map<F>(self, map: F) -> MappedJoinTarget<T, Payload, F>
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
    /// let join = task.register_node_fn(|input: &JoinInput<i32>| -> Result<i32, NodeError> {
    ///     Ok(input.iter().copied().sum())
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
    pub fn map_async<F>(self, map: F) -> MappedJoinTarget<T, Payload, F>
    where
        F: Send + Sync + 'static,
    {
        MappedJoinTarget {
            join_target: self,
            map,
        }
    }
}

impl<T: TaskNode<Input = AnyJoinInput> + ?Sized> AnyJoinTarget<T> {
    pub(crate) fn new(node_id: NodeId<T>) -> Self {
        Self {
            definition: JoinDefinition::any(node_id.id()),
            _marker: PhantomData,
        }
    }

    /// Overrides the concurrency model for the join branch that will be scheduled.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.definition.concurrency_model = Some(concurrency_model);
        self
    }
}

impl<T, Payload, F> MappedJoinTarget<T, Payload, F>
where
    T: TaskNode<Input = JoinInput<Payload>> + ?Sized,
    Payload: NodeArg,
{
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
///     |input: &swiftide_agents::tasks::JoinInput<i32>| -> Result<i32, NodeError> {
///         Ok(input.iter().copied().sum())
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
///
/// This is returned by [`Transition::next`] and [`NodeId::transitions_with`], then accepted by
/// [`Task::register_transition`](crate::tasks::Task::register_transition) closures.
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
    /// This defines the branch group and the join node that waits for every branch in that group.
    /// Each branch still needs its own registered transition to the same join target when it is
    /// ready to contribute its output.
    pub fn join_with<J>(self, join_target: J) -> Transition
    where
        J: JoinDestination,
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
    ///     |input: &swiftide_agents::tasks::JoinInput<i32>| -> Result<i32, NodeError> {
    ///         Ok(input.iter().copied().sum())
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

/// The typed aggregated view of branches that have reached a join node.
///
/// `JoinInput<T>` is the normal join input. It is only available when every branch contributes a
/// `T`, or maps its output into `T` before joining. For mixed payload types, use [`AnyJoinInput`].
#[derive(Clone)]
pub struct JoinInput<T: NodeArg> {
    branches: Vec<Arc<T>>,
}

impl<T: NodeArg> std::fmt::Debug for JoinInput<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinInput")
            .field("payload_type", &std::any::type_name::<T>())
            .field("len", &self.branches.len())
            .finish()
    }
}

impl<T: NodeArg> JoinInput<T> {
    pub(crate) fn new(branches: Vec<Arc<T>>) -> Self {
        Self { branches }
    }

    /// Iterates over every ready branch payload in stable branch creation order.
    ///
    /// This is the usual way to read join payloads.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.branches.iter().map(Arc::as_ref)
    }

    /// Returns the number of branch payloads in this join.
    pub fn len(&self) -> usize {
        self.branches.len()
    }

    /// Returns `true` when this join has no branch payloads.
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }
}

/// The type-erased aggregated view of branches that have reached a join node.
///
/// Use this for joins that intentionally collect mixed branch payload types. Homogeneous joins
/// should prefer [`JoinInput<T>`](JoinInput), which is checked when transitions are registered.
#[derive(Clone)]
pub struct AnyJoinInput {
    branches: Vec<ErasedPayload>,
}

impl std::fmt::Debug for AnyJoinInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyJoinInput")
            .field("len", &self.branches.len())
            .finish()
    }
}

impl AnyJoinInput {
    pub(crate) fn new(branches: Vec<ErasedPayload>) -> Self {
        Self { branches }
    }

    /// Iterates over every type-erased branch payload in stable branch creation order.
    pub fn iter_any(&self) -> impl Iterator<Item = &(dyn Any + Send + Sync)> {
        self.branches.iter().map(Arc::as_ref)
    }

    /// Iterates over every ready branch payload that can be downcast to `T`.
    ///
    /// Values are yielded in stable branch creation order. This is the usual way to read join
    /// payloads from a mixed join when you only need one payload type.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{AnyJoinInput, NodeError, Task, Transition};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    /// let left = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    /// let right = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 2) });
    /// let join = task.register_node_fn(|input: &AnyJoinInput| -> Result<i32, NodeError> {
    ///     Ok(input.iter::<i32>().copied().sum())
    /// });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, move |value| {
    ///     Transition::fan_out(&left, value)
    ///         .and(&right, value)
    ///         .join_with(join.join_any())
    /// })?;
    /// task.register_transition(left, join.join_any())?;
    /// task.register_transition(right, join.join_any())?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn iter<T: NodeArg>(&self) -> impl Iterator<Item = &T> {
        self.iter_any()
            .filter_map(|value| value.downcast_ref::<T>())
    }

    /// Returns the number of branch payloads in this join.
    pub fn len(&self) -> usize {
        self.branches.len()
    }

    /// Returns `true` when this join has no branch payloads.
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }
}
