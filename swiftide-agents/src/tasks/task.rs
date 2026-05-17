//! Tasks enable you to define a graph of interacting nodes.
//!
//! The nodes can be any type that implements the `TaskNode` trait, which defines how the node
//! will be evaluated with its input and output.
//!
//! Most swiftide primitives implement `TaskNode`, and it's easy to implement your own. Since how
//! agents interact is subject to taste, we recommend implementing your own.
//!
//! # Examples
//!
//! ```no_run
//! use swiftide_agents::tasks::{NodeError, Task, TaskRunState};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut task = Task::<i32, i32>::builder().build();
//!
//! let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
//! let finish =
//!     task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input * 2) });
//!
//! task.starts_with(start);
//! task.register_transition(start, move |value| finish.transitions_with(value))?;
//! task.register_transition(finish, task.transitions_to_finish())?;
//!
//! assert_eq!(task.run(2).await?, TaskRunState::Completed(6));
//! # Ok(())
//! # }
//! ```
//!
//! WARN: Here be dragons! This api is not stable yet. We are using it in production, and it is
//! subject to rapid change. However, do not hesitate to open an issue if you find anything.
use std::{any::Any, future::Future, pin::Pin, sync::Arc};

use super::node::NodeId;
use super::{
    adapters::{AsyncFn, SyncFn},
    errors::TaskError,
    executor::{JoinHandler, NodeExecutor, TransitionHandler},
    runtime::Runtime,
    traits::{
        AnyNodeExecutor, NodeArg, RegisterTransition, RegisterTransitionAsync, TaskNode,
        TransitionResult,
    },
    transition::{
        AnyJoinInput, AnyJoinTarget, ConcurrencyModel, JoinDefinition, JoinDestination, JoinInput,
        JoinTarget, MappedJoinTarget, Transition,
    },
};

/// The observable outcome of calling [`Task::run`] or [`Task::resume`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRunState<Output> {
    /// The task reached its finish transition and produced an output.
    Completed(Output),
    /// The task paused and can be continued with [`Task::resume`].
    Paused,
}

/// Configures default runtime behavior for a [`Task`].
#[derive(Debug)]
#[must_use]
pub struct TaskBuilder<Input: NodeArg, Output: NodeArg> {
    default_concurrency_model: ConcurrencyModel,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> TaskBuilder<Input, Output> {
    pub(crate) fn new() -> Self {
        Self {
            default_concurrency_model: ConcurrencyModel::Sequential,
            _marker: std::marker::PhantomData,
        }
    }

    /// Sets the default concurrency model for transitions that do not override it explicitly.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.default_concurrency_model = concurrency_model;
        self
    }

    /// Builds a new task with the configured defaults.
    pub fn build(self) -> Task<Input, Output> {
        Task::with_default_concurrency_model(self.default_concurrency_model)
    }
}

/// A typed task graph that can run sequential, branching, and joining workflows.
///
/// Register nodes with [`Task::register_node`] or [`Task::register_node_fn`], choose a start node
/// with [`Task::starts_with`], connect nodes with [`Task::register_transition`], and then execute
/// the task with [`Task::run`].
///
/// The task value stores runtime state as well as the graph definition, so reuse the same task
/// when you need pause and resume behavior. Clone a task when you want a fresh runtime with the
/// same graph definition.
///
/// # Examples
///
/// ```no_run
/// use swiftide_agents::tasks::{NodeError, Task, TaskRunState};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut task = Task::<i32, i32>::new();
///
/// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
/// let finish =
///     task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input * 3) });
///
/// task.starts_with(start);
/// task.register_transition(start, move |value| finish.transitions_with(value))?;
/// task.register_transition(finish, task.transitions_to_finish())?;
///
/// assert_eq!(task.run(2).await?, TaskRunState::Completed(9));
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Task<Input: NodeArg, Output: NodeArg> {
    pub(crate) nodes: Vec<Arc<dyn AnyNodeExecutor>>,
    pub(crate) start_node: Option<usize>,
    pub(crate) runtime: Runtime,
    pub(crate) default_concurrency_model: ConcurrencyModel,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<From, F, R> RegisterTransition<From> for F
where
    From: TaskNode + 'static + ?Sized,
    F: Fn(From::Output) -> R + Send + Sync + 'static,
    R: TransitionResult<From> + 'static,
{
    fn register<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError> {
        let transition = Arc::new(self);
        task.set_transition_handler(
            from,
            Arc::new(move |output: From::Output| {
                let transition = transition.clone();
                Box::pin(async move { transition(output).into_transition() })
            }),
        )
    }
}

impl<From, F, Fut, R> RegisterTransitionAsync<From> for F
where
    From: TaskNode + 'static + ?Sized,
    F: Fn(From::Output) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: TransitionResult<From> + 'static,
{
    fn register_async<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError> {
        let transition = Arc::new(self);
        task.set_transition_handler(
            from,
            Arc::new(move |output: From::Output| {
                let transition = transition.clone();
                Box::pin(async move { transition(output).await.into_transition() })
            }),
        )
    }
}

impl<From, To, Payload> RegisterTransition<From> for JoinTarget<To, Payload>
where
    From: TaskNode + 'static + ?Sized,
    From::Output: Into<Payload>,
    Payload: NodeArg,
    To: TaskNode<Input = JoinInput<Payload>> + 'static + ?Sized,
{
    fn register<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError> {
        task.set_join_handler(
            from,
            self.into_definition(),
            Arc::new(move |output: From::Output| {
                Box::pin(async move { Arc::new(output.into()) as Arc<dyn Any + Send + Sync> })
            }),
        )
    }
}

impl<From, To> RegisterTransition<From> for AnyJoinTarget<To>
where
    From: TaskNode + 'static + ?Sized,
    From::Output: NodeArg,
    To: TaskNode<Input = AnyJoinInput> + 'static + ?Sized,
{
    fn register<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError> {
        task.set_join_handler(
            from,
            self.into_definition(),
            Arc::new(move |output: From::Output| {
                Box::pin(async move { Arc::new(output) as Arc<dyn Any + Send + Sync> })
            }),
        )
    }
}

impl<From, To, Payload, F> RegisterTransition<From> for MappedJoinTarget<To, Payload, F>
where
    From: TaskNode + 'static + ?Sized,
    To: TaskNode<Input = JoinInput<Payload>> + 'static + ?Sized,
    F: Fn(From::Output) -> Payload + Send + Sync + 'static,
    Payload: NodeArg,
{
    fn register<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError> {
        let MappedJoinTarget { join_target, map } = self;
        let map = Arc::new(map);
        task.set_join_handler(
            from,
            join_target.into_definition(),
            Arc::new(move |output: From::Output| {
                let map = map.clone();
                Box::pin(async move { Arc::new(map(output)) as Arc<dyn Any + Send + Sync> })
            }),
        )
    }
}

impl<From, To, Payload, F, Fut> RegisterTransitionAsync<From> for MappedJoinTarget<To, Payload, F>
where
    From: TaskNode + 'static + ?Sized,
    To: TaskNode<Input = JoinInput<Payload>> + 'static + ?Sized,
    F: Fn(From::Output) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Payload> + Send + 'static,
    Payload: NodeArg,
{
    fn register_async<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError> {
        let MappedJoinTarget { join_target, map } = self;
        let map = Arc::new(map);
        task.set_join_handler(
            from,
            join_target.into_definition(),
            Arc::new(move |output: From::Output| {
                let map = map.clone();
                Box::pin(async move { Arc::new(map(output).await) as Arc<dyn Any + Send + Sync> })
            }),
        )
    }
}

impl<Input: NodeArg, Output: NodeArg> Clone for Task<Input, Output> {
    fn clone(&self) -> Self {
        Self {
            nodes: self
                .nodes
                .iter()
                .map(|node_executor| {
                    Arc::<dyn AnyNodeExecutor>::from(dyn_clone::clone_box(&**node_executor))
                })
                .collect(),
            start_node: self.start_node,
            runtime: Runtime::new(),
            default_concurrency_model: self.default_concurrency_model,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> Default for Task<Input, Output> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> Task<Input, Output> {
    /// Creates a builder for configuring task-wide defaults before constructing a [`Task`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{ConcurrencyModel, Task};
    ///
    /// let task = Task::<i32, i32>::builder()
    ///     .concurrency_model(ConcurrencyModel::Parallel)
    ///     .build();
    ///
    /// let _ = task;
    /// ```
    pub fn builder() -> TaskBuilder<Input, Output> {
        TaskBuilder::new()
    }

    /// Creates a new task with the default runtime behavior.
    pub fn new() -> Self {
        Self::with_default_concurrency_model(ConcurrencyModel::Sequential)
    }

    fn with_default_concurrency_model(default_concurrency_model: ConcurrencyModel) -> Self {
        Self {
            nodes: Vec::new(),
            start_node: None,
            runtime: Runtime::new(),
            default_concurrency_model,
            _marker: std::marker::PhantomData,
        }
    }

    /// Marks the node where execution should start.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    ///
    /// task.starts_with(start);
    /// ```
    pub fn starts_with<T: TaskNode<Input = Input> + Clone + 'static>(
        &mut self,
        node_id: NodeId<T>,
    ) {
        self.start_node = Some(node_id.id());
        self.runtime.clear_state();
    }

    /// Returns a typed transition closure that finishes the task with the final output.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task, TaskRunState};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    ///
    /// task.starts_with(start);
    /// task.register_transition(start, task.transitions_to_finish())?;
    ///
    /// assert_eq!(task.run(2).await?, TaskRunState::Completed(3));
    /// # Ok(())
    /// # }
    /// ```
    pub fn transitions_to_finish(&self) -> impl Fn(Output) -> Transition + Send + Sync + 'static {
        |output| Transition::finish(output)
    }

    /// Starts the task from its configured start node.
    ///
    /// Returns [`TaskRunState::Completed`] when the task reaches its finish transition, or
    /// [`TaskRunState::Paused`] when execution was intentionally paused.
    ///
    /// # Errors
    ///
    /// Returns an error when the task is already active, when the graph definition is incomplete,
    /// or when a node evaluation or transition fails while running the task.
    #[tracing::instrument(skip(self, input), name = "task.run", err)]
    pub async fn run(
        &mut self,
        input: impl Into<Input>,
    ) -> Result<TaskRunState<Output>, TaskError> {
        if self.runtime.is_live() {
            return Err(TaskError::TaskActive);
        }

        let start_node = self.validate_transitions()?;
        self.runtime
            .run(
                &self.nodes,
                start_node,
                self.default_concurrency_model,
                input.into(),
            )
            .await
    }

    /// Resets runtime state while keeping the graph definition and last start input.
    ///
    /// After calling `reset`, use [`Task::resume`] to rerun the task from the start node with the
    /// most recent input passed to [`Task::run`].
    pub fn reset(&mut self) {
        self.runtime
            .reset(self.start_node, self.default_concurrency_model);
    }

    /// Continues a paused or reset task.
    ///
    /// # Errors
    ///
    /// Returns an error when the task graph is invalid, when there is no paused or reset state to
    /// resume, or when a node evaluation or transition fails while continuing execution.
    #[tracing::instrument(skip(self), name = "task.resume", err)]
    pub async fn resume(&mut self) -> Result<TaskRunState<Output>, TaskError> {
        self.validate_transitions()?;

        self.runtime
            .resume(&self.nodes, self.default_concurrency_model)
            .await
    }

    /// Returns the node for the first paused or runnable branch when it has the requested type.
    pub fn current_node<T: TaskNode + Clone + 'static>(&self) -> Option<&T> {
        let node_id = self.runtime.current_node()?;

        self.nodes.get(node_id)?.node_as_any().downcast_ref::<T>()
    }

    /// Registers a node in the task graph and returns its typed identifier.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, SyncFn, Task};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let start = task.register_node(SyncFn::new(|input: &i32| -> Result<i32, NodeError> {
    ///     Ok(*input + 1)
    /// }));
    ///
    /// let _ = start;
    /// ```
    pub fn register_node<T>(&mut self, node: T) -> NodeId<T>
    where
        T: TaskNode + 'static + Clone,
    {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        self.nodes.push(Arc::new(NodeExecutor::new(node, node_id)));
        node_id
    }

    /// Registers a synchronous closure as a task node.
    ///
    /// This is the convenience entry point for examples, tests, and small bits of task glue.
    /// For reusable domain logic, prefer implementing [`TaskNode`] directly and calling
    /// [`Task::register_node`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let double = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> {
    ///     Ok(*input * 2)
    /// });
    ///
    /// let _ = double;
    /// ```
    pub fn register_node_fn<F, I, O, E>(&mut self, f: F) -> NodeId<SyncFn<F, I, O, E>>
    where
        F: Fn(&I) -> Result<O, E> + Send + Sync + Clone + 'static,
        I: NodeArg + Clone,
        O: NodeArg + Clone,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.register_node(SyncFn::new(f))
    }

    /// Registers an asynchronous closure as a task node.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use swiftide_agents::tasks::{NodeError, Task};
    ///
    /// let mut task = Task::<i32, i32>::new();
    /// let double = task.register_node_async_fn(|input: &i32| {
    ///     Box::pin(async move { Ok::<i32, NodeError>(*input * 2) })
    /// });
    ///
    /// let _ = double;
    /// ```
    pub fn register_node_async_fn<F, I, O, E>(&mut self, f: F) -> NodeId<AsyncFn<F, I, O, E>>
    where
        F: for<'a> Fn(&'a I) -> Pin<Box<dyn Future<Output = Result<O, E>> + Send + 'a>>
            + Send
            + Sync
            + Clone
            + 'static,
        I: NodeArg + Clone,
        O: NodeArg + Clone,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.register_node(AsyncFn::new(f))
    }

    /// Registers how execution should continue after `from` completes.
    ///
    /// The transition may be:
    /// - a closure returning a typed [`MarkedTransition`](crate::tasks::MarkedTransition)
    /// - a closure returning a raw [`Transition`] for advanced control flow
    /// - a [`JoinTarget`](crate::tasks::JoinTarget) built from a join node
    /// - a mapped join target produced by [`JoinTarget::map`](crate::tasks::JoinTarget::map)
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
    ///
    /// # Errors
    ///
    /// Returns an error when `from` is unknown, when the node already has a registered
    /// transition, or when the transition type does not match the registered node type.
    pub fn register_transition<From, R>(
        &mut self,
        from: NodeId<From>,
        transition: R,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        R: RegisterTransition<From>,
    {
        transition.register(self, from)
    }

    /// Registers an asynchronous transition or async join payload mapping for `from`.
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
    /// task.register_transition_async(start, move |value| async move { finish.transitions_with(value) })?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error when `from` is unknown, when the node already has a registered
    /// transition, or when the transition type does not match the registered node type.
    pub fn register_transition_async<From, R>(
        &mut self,
        from: NodeId<From>,
        transition: R,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        R: RegisterTransitionAsync<From>,
    {
        transition.register_async(self, from)
    }

    fn validate_transitions(&self) -> Result<usize, TaskError> {
        let start_node = self.start_node.ok_or(TaskError::NoSteps)?;

        for (index, node_executor) in self.nodes.iter().enumerate() {
            if !node_executor.transition_is_set() {
                return Err(TaskError::missing_transition(index));
            }
        }

        Ok(start_node)
    }

    fn set_transition_handler<From>(
        &mut self,
        from: NodeId<From>,
        transition: TransitionHandler<From::Output>,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
    {
        self.executor_mut(from)?.set_transition_handler(transition)
    }

    fn set_join_handler<From>(
        &mut self,
        from: NodeId<From>,
        definition: JoinDefinition,
        transition: JoinHandler<From::Output>,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
    {
        self.executor_mut(from)?
            .set_join_handler(definition, transition)
    }

    fn executor_mut<From>(
        &mut self,
        from: NodeId<From>,
    ) -> Result<&mut NodeExecutor<From::Input, From::Output, From::Error>, TaskError>
    where
        From: TaskNode + 'static + ?Sized,
    {
        let node_executor = self
            .nodes
            .get_mut(from.id())
            .ok_or_else(|| TaskError::missing_node(from.id()))?;
        let node_executor = Arc::get_mut(node_executor).ok_or_else(|| {
            TaskError::invalid_state(format!("Node {} is currently in use", from.id()))
        })?;

        let executor =
            (node_executor as &mut dyn Any)
                .downcast_mut::<NodeExecutor<From::Input, From::Output, From::Error>>();

        let Some(executor) = executor else {
            return Err(TaskError::invalid_state(format!(
                "Transition registration type mismatch for node {}",
                from.id()
            )));
        };

        Ok(executor)
    }
}
