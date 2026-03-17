//! Tasks enable you to define a graph of interacting nodes.
//!
//! The nodes can be any type that implements the `TaskNode` trait, which defines how the node
//! will be evaluated with its input and output.
//!
//! Most swiftide primitives implement `TaskNode`, and it's easy to implement your own. Since how
//! agents interact is subject to taste, we recommend implementing your own.
//!
//! WARN: Here be dragons! This api is not stable yet. We are using it in production, and it is
//! subject to rapid change. However, do not hesitate to open an issue if you find anything.
use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    future::Future,
    num::NonZeroUsize,
    pin::Pin,
    sync::Arc,
};

use super::{
    adapters::{AsyncFn, SyncFn},
    errors::TaskError,
    node::{NodeArg, NodeId, TaskNode},
    runtime::{
        AnyNodeExecutor, BranchGroupId, ExecutionBranch, JoinGroupState, JoinHandler, NodeExecutor,
        TaskOptions, TransitionHandler,
    },
    transition::{
        ActiveBranch, AsyncMappedJoinTarget, BranchId, ConcurrencyModel, ErrorBehavior,
        JoinDefinition, JoinInput, JoinTarget, MappedJoinTarget, PauseBehavior, Transition,
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
pub struct TaskBuilder<Input: NodeArg, Output: NodeArg> {
    options: TaskOptions,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> TaskBuilder<Input, Output> {
    pub(crate) fn new() -> Self {
        Self {
            options: TaskOptions::default(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Sets the default concurrency model for transitions that do not override it explicitly.
    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.options.concurrency_model = concurrency_model;
        self
    }

    /// Sets the default pause behavior for transitions that do not override it explicitly.
    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.options.pause_behavior = pause_behavior;
        self
    }

    /// Sets the default error behavior for transitions that do not override it explicitly.
    pub fn error_behavior(mut self, error_behavior: ErrorBehavior) -> Self {
        self.options.error_behavior = error_behavior;
        self
    }

    /// Caps how many parallel branches may execute at the same time.
    pub fn max_parallelism(mut self, max_parallelism: NonZeroUsize) -> Self {
        self.options.max_parallelism = max_parallelism.get();
        self
    }

    /// Builds a new task with the configured defaults.
    pub fn build(self) -> Task<Input, Output> {
        Task::with_options(self.options)
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
    pub(crate) nodes: Vec<Box<dyn AnyNodeExecutor>>,
    pub(crate) start_node: Option<usize>,
    pub(crate) runnable_branches: VecDeque<ExecutionBranch>,
    pub(crate) paused_branches: HashMap<BranchId, ExecutionBranch>,
    pub(crate) join_groups: HashMap<BranchGroupId, JoinGroupState>,
    pub(crate) next_branch_id: usize,
    pub(crate) next_group_id: usize,
    pub(crate) last_start_context: Option<Arc<dyn Any + Send + Sync>>,
    pub(crate) options: TaskOptions,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

#[doc(hidden)]
pub trait RegisterTransition<From: TaskNode + ?Sized>: 'static {
    fn register<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError>;
}

#[doc(hidden)]
pub trait RegisterTransitionAsync<From: TaskNode + ?Sized>: 'static {
    fn register_async<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError>;
}

impl<From, F, R> RegisterTransition<From> for F
where
    From: TaskNode + 'static + ?Sized,
    F: Fn(From::Output) -> R + Send + Sync + 'static,
    R: Into<Transition> + 'static,
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
                Box::pin(async move { transition(output).into() })
            }),
        )
    }
}

impl<From, F, Fut, R> RegisterTransitionAsync<From> for F
where
    From: TaskNode + 'static + ?Sized,
    F: Fn(From::Output) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: Into<Transition> + 'static,
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
                Box::pin(async move { transition(output).await.into() })
            }),
        )
    }
}

impl<From, To> RegisterTransition<From> for JoinTarget<To>
where
    From: TaskNode + 'static + ?Sized,
    From::Output: NodeArg,
    To: TaskNode<Input = JoinInput> + 'static + ?Sized,
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

impl<From, To, F, Payload> RegisterTransition<From> for MappedJoinTarget<To, F>
where
    From: TaskNode + 'static + ?Sized,
    To: TaskNode<Input = JoinInput> + 'static + ?Sized,
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

impl<From, To, F, Fut, Payload> RegisterTransitionAsync<From> for AsyncMappedJoinTarget<To, F>
where
    From: TaskNode + 'static + ?Sized,
    To: TaskNode<Input = JoinInput> + 'static + ?Sized,
    F: Fn(From::Output) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Payload> + Send + 'static,
    Payload: NodeArg,
{
    fn register_async<Input: NodeArg + Clone, Output: NodeArg + Clone>(
        self,
        task: &mut Task<Input, Output>,
        from: NodeId<From>,
    ) -> Result<(), TaskError> {
        let AsyncMappedJoinTarget { join_target, map } = self;
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
            nodes: self.nodes.clone(),
            start_node: self.start_node,
            runnable_branches: VecDeque::new(),
            paused_branches: HashMap::new(),
            join_groups: HashMap::new(),
            next_branch_id: 1,
            next_group_id: 1,
            last_start_context: None,
            options: self.options.clone(),
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
    pub fn builder() -> TaskBuilder<Input, Output> {
        TaskBuilder::new()
    }

    /// Creates a new task with the default runtime behavior.
    pub fn new() -> Self {
        Self::with_options(TaskOptions::default())
    }

    fn with_options(options: TaskOptions) -> Self {
        Self {
            nodes: Vec::new(),
            start_node: None,
            runnable_branches: VecDeque::new(),
            paused_branches: HashMap::new(),
            join_groups: HashMap::new(),
            next_branch_id: 1,
            next_group_id: 1,
            last_start_context: None,
            options,
            _marker: std::marker::PhantomData,
        }
    }

    /// Marks the node where execution should start.
    pub fn starts_with<T: TaskNode<Input = Input> + Clone + 'static>(
        &mut self,
        node_id: NodeId<T>,
    ) {
        self.start_node = Some(node_id.id());
        self.reset_runtime();
    }

    /// Returns a typed transition closure that finishes the task with the final output.
    pub fn transitions_to_finish(&self) -> impl Fn(Output) -> Transition + Send + Sync + 'static {
        |output| Transition::finish(output)
    }

    /// Starts the task from its configured start node.
    ///
    /// Returns [`TaskRunState::Completed`] when the task reaches its finish transition, or
    /// [`TaskRunState::Paused`] when execution was intentionally paused.
    #[tracing::instrument(skip(self, input), name = "task.run", err)]
    pub async fn run(
        &mut self,
        input: impl Into<Input>,
    ) -> Result<TaskRunState<Output>, TaskError> {
        if self.has_live_state() {
            return Err(TaskError::TaskActive);
        }

        let start_node = self.validate_transitions()?;
        let context = Arc::new(input.into()) as Arc<dyn Any + Send + Sync>;
        self.last_start_context = Some(context.clone());
        self.reset_runtime();
        let branch_id = self.next_branch();
        let settings = self.default_settings();
        self.enqueue_branch(ExecutionBranch {
            id: branch_id,
            current_node: start_node,
            context,
            settings,
            join_group: None,
        });

        self.start_task().await
    }

    /// Resets runtime state while keeping the graph definition and last start input.
    ///
    /// After calling `reset`, use [`Task::resume`] to rerun the task from the start node with the
    /// most recent input passed to [`Task::run`].
    pub fn reset(&mut self) {
        let Some(start_node) = self.start_node else {
            self.reset_runtime();
            return;
        };

        let context = self.last_start_context.clone();
        self.reset_runtime();

        if let Some(context) = context {
            let branch_id = self.next_branch();
            let settings = self.default_settings();
            self.enqueue_branch(ExecutionBranch {
                id: branch_id,
                current_node: start_node,
                context,
                settings,
                join_group: None,
            });
        }
    }

    /// Continues a paused or reset task.
    #[tracing::instrument(skip(self), name = "task.resume", err)]
    pub async fn resume(&mut self) -> Result<TaskRunState<Output>, TaskError> {
        self.validate_transitions()?;

        if self.runnable_branches.is_empty() && self.paused_branches.is_empty() {
            return Err(TaskError::NotResumable);
        }

        self.restore_paused_branches();
        self.start_task().await
    }

    /// Returns the currently queued branches that have not started running yet.
    pub fn active_branches(&self) -> Vec<ActiveBranch> {
        self.runnable_branches
            .iter()
            .map(|branch| ActiveBranch {
                branch_id: branch.id,
                node_id: branch.current_node,
            })
            .collect()
    }

    /// Returns the branches that paused and can be resumed.
    pub fn paused_branches(&self) -> Vec<ActiveBranch> {
        let mut branches = self
            .paused_branches
            .values()
            .map(|branch| ActiveBranch {
                branch_id: branch.id,
                node_id: branch.current_node,
            })
            .collect::<Vec<_>>();
        branches.sort_by_key(|branch| branch.branch_id.0);
        branches
    }

    /// Registers a node in the task graph and returns its typed identifier.
    pub fn register_node<T>(&mut self, node: T) -> NodeId<T>
    where
        T: TaskNode + 'static + Clone,
    {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        self.nodes.push(Box::new(NodeExecutor::new(node, node_id)));
        node_id
    }

    /// Registers a synchronous closure as a task node.
    ///
    /// This is the convenience entry point for examples, tests, and small bits of task glue.
    /// For reusable domain logic, prefer implementing [`TaskNode`] directly and calling
    /// [`Task::register_node`].
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
    /// - a closure returning a [`Transition`] or
    ///   [`MarkedTransition`](crate::tasks::MarkedTransition)
    /// - a [`JoinTarget`](crate::tasks::JoinTarget) built from a join node
    /// - a mapped join target produced by [`JoinTarget::map`](crate::tasks::JoinTarget::map)
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

    fn set_transition_handler<From>(
        &mut self,
        from: NodeId<From>,
        transition: TransitionHandler<From::Output>,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
    {
        let node_executor = self
            .nodes
            .get_mut(from.id())
            .ok_or_else(|| TaskError::missing_node(from.id()))?;

        let executor = (&mut **node_executor as &mut dyn Any).downcast_mut::<NodeExecutor<
            From::Input,
            From::Output,
            From::Error,
        >>();

        let Some(executor) = executor else {
            return Err(TaskError::invalid_state(format!(
                "Transition registration type mismatch for node {}",
                from.id()
            )));
        };

        executor.set_transition_handler(transition)
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
        let node_executor = self
            .nodes
            .get_mut(from.id())
            .ok_or_else(|| TaskError::missing_node(from.id()))?;

        let executor = (&mut **node_executor as &mut dyn Any).downcast_mut::<NodeExecutor<
            From::Input,
            From::Output,
            From::Error,
        >>();

        let Some(executor) = executor else {
            return Err(TaskError::invalid_state(format!(
                "Transition registration type mismatch for node {}",
                from.id()
            )));
        };

        executor.set_join_handler(definition, transition)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        num::NonZeroUsize,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::time::sleep;

    use super::*;
    use crate::tasks::JoinInput;

    #[derive(thiserror::Error, Debug)]
    struct Error(String);

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[derive(Clone, Default, Debug)]
    struct IntNode;

    #[async_trait]
    impl TaskNode for IntNode {
        type Input = i32;
        type Output = i32;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(input + 1)
        }
    }

    #[derive(Clone, Debug)]
    struct OffsetNode(i32);

    #[async_trait]
    impl TaskNode for OffsetNode {
        type Input = i32;
        type Output = i32;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(*input + self.0)
        }
    }

    #[derive(Clone, Default, Debug)]
    struct SumJoinNode;

    #[async_trait]
    impl TaskNode for SumJoinNode {
        type Input = JoinInput;
        type Output = i32;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(input.ready_values::<i32>().into_iter().copied().sum())
        }
    }

    #[derive(Clone, Default, Debug)]
    struct CollectJoinNode;

    #[async_trait]
    impl TaskNode for CollectJoinNode {
        type Input = JoinInput;
        type Output = Vec<i32>;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(input.ready_values::<i32>().into_iter().copied().collect())
        }
    }

    #[derive(Clone, Default, Debug)]
    struct PauseOnceNode;

    #[async_trait]
    impl TaskNode for PauseOnceNode {
        type Input = i32;
        type Output = i32;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(*input)
        }
    }

    #[derive(Clone, Default, Debug)]
    struct FailingNode;

    #[async_trait]
    impl TaskNode for FailingNode {
        type Input = i32;
        type Output = i32;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            _input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Err(Error("boom".into()))
        }
    }

    #[derive(Clone, Debug)]
    struct TrackingNode {
        current: Arc<AtomicUsize>,
        max: Arc<AtomicUsize>,
        delay: Duration,
    }

    #[async_trait]
    impl TaskNode for TrackingNode {
        type Input = i32;
        type Output = i32;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            let running = self.current.fetch_add(1, Ordering::SeqCst) + 1;
            let mut observed = self.max.load(Ordering::SeqCst);

            while running > observed {
                match self.max.compare_exchange(
                    observed,
                    running,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => observed = actual,
                }
            }

            sleep(self.delay).await;
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(*input)
        }
    }

    #[test_log::test(tokio::test)]
    async fn sequential_3_node_task_reset_works() {
        let mut task: Task<i32, i32> = Task::new();

        let node1 = task.register_node(IntNode);
        let node2 = task.register_node(IntNode);
        let node3 = task.register_node(IntNode);

        task.starts_with(node1);

        task.register_transition(node1, move |input| node2.transitions_with(input))
            .unwrap();
        task.register_transition(node2, move |input| node3.transitions_with(input))
            .unwrap();
        task.register_transition(node3, task.transitions_to_finish())
            .unwrap();

        let res = task.run(1).await.unwrap();
        assert_eq!(res, TaskRunState::Completed(4));

        task.reset();

        let rerun = task.resume().await.unwrap();
        assert_eq!(rerun, TaskRunState::Completed(4));
    }

    #[test_log::test(tokio::test)]
    async fn fan_out_can_join_multiple_branches() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let branch_a = task.register_node(IntNode);
        let branch_b = task.register_node(IntNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([branch_a.target_with(input), branch_b.target_with(input)])
        })
        .unwrap();
        task.register_transition(branch_a, join.join()).unwrap();
        task.register_transition(branch_b, join.join()).unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(6));
    }

    #[test_log::test(tokio::test)]
    async fn paused_branch_keeps_other_branches_running() {
        let mut task: Task<i32, i32> = Task::builder()
            .pause_behavior(PauseBehavior::DrainRunnable)
            .build();

        let start = task.register_node(IntNode);
        let active = task.register_node(IntNode);
        let paused = task.register_node(PauseOnceNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([active.target_with(input), paused.target_with(input)])
                .concurrency_model(ConcurrencyModel::Parallel)
        })
        .unwrap();
        task.register_transition(active, join.join_at_least(1).continue_remaining())
            .unwrap();
        task.register_transition(paused, move |_output| Transition::pause())
            .unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(3));
    }

    #[test_log::test(tokio::test)]
    async fn explicit_joiners_can_share_a_fan_out_with_normal_branches() {
        let mut task: Task<i32, i32> = Task::builder()
            .pause_behavior(PauseBehavior::DrainRunnable)
            .build();

        let start = task.register_node(IntNode);
        let joining = task.register_node(IntNode);
        let paused = task.register_node(PauseOnceNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([joining.target_with(input), paused.target_with(input)])
                .concurrency_model(ConcurrencyModel::Parallel)
        })
        .unwrap();
        task.register_transition(joining, join.join_at_least(1).continue_remaining())
            .unwrap();
        task.register_transition(paused, move |_output| Transition::pause())
            .unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(3));
    }

    #[test_log::test(tokio::test)]
    async fn pause_behavior_can_pause_task() {
        let mut task: Task<i32, i32> = Task::builder()
            .pause_behavior(PauseBehavior::PauseTask)
            .build();

        let start = task.register_node(PauseOnceNode);
        task.starts_with(start);

        task.register_transition(start, move |_output| Transition::pause())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Paused);
    }

    #[test_log::test(tokio::test)]
    async fn run_rejects_overwriting_active_state() {
        let mut task: Task<i32, i32> = Task::builder()
            .pause_behavior(PauseBehavior::PauseTask)
            .build();

        let start = task.register_node(PauseOnceNode);
        task.starts_with(start);
        task.register_transition(start, move |_output| Transition::pause())
            .unwrap();

        assert_eq!(task.run(1).await.unwrap(), TaskRunState::Paused);
        assert!(matches!(
            task.run(2).await.unwrap_err(),
            TaskError::TaskActive
        ));
    }

    #[test_log::test(tokio::test)]
    async fn resume_requires_resumable_state() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        task.starts_with(start);
        task.register_transition(start, task.transitions_to_finish())
            .unwrap();

        assert_eq!(task.run(1).await.unwrap(), TaskRunState::Completed(2));
        assert!(matches!(
            task.resume().await.unwrap_err(),
            TaskError::NotResumable
        ));
    }

    #[test_log::test(tokio::test)]
    async fn task_without_finish_is_incomplete() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        task.starts_with(start);
        task.register_transition(start, move |_output| Transition::fan_out(Vec::new()))
            .unwrap();

        assert!(matches!(
            task.run(1).await.unwrap_err(),
            TaskError::Incomplete
        ));
    }

    #[test_log::test(tokio::test)]
    async fn error_behavior_can_fail_task() {
        let mut task: Task<i32, i32> = Task::builder()
            .error_behavior(ErrorBehavior::FailTask)
            .build();

        let start = task.register_node(FailingNode);
        task.starts_with(start);

        task.register_transition(start, task.transitions_to_finish())
            .unwrap();

        let error = task.run(1).await.unwrap_err();
        assert!(matches!(error, TaskError::NodeError(_)));
    }

    #[test_log::test(tokio::test)]
    async fn join_input_keeps_branch_creation_order() {
        let mut task: Task<i32, Vec<i32>> = Task::new();

        let start = task.register_node(OffsetNode(0));
        let first = task.register_node(OffsetNode(1));
        let second = task.register_node(OffsetNode(10));
        let join = task.register_node(CollectJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([first.target_with(input), second.target_with(input)])
                .concurrency_model(ConcurrencyModel::Parallel)
        })
        .unwrap();
        task.register_transition(first, join.join()).unwrap();
        task.register_transition(second, join.join()).unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(vec![2, 11]));
    }

    #[test_log::test(tokio::test)]
    async fn all_fanout_branches_scope_preserves_full_fanout_join() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let first = task.register_node(IntNode);
        let second = task.register_node(IntNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([first.target_with(input), second.target_with(input)])
        })
        .unwrap();
        task.register_transition(first, join.join().all_fanout_branches())
            .unwrap();
        task.register_transition(second, join.join().all_fanout_branches())
            .unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(6));
    }

    #[test_log::test(tokio::test)]
    async fn all_fanout_branches_scope_rejects_mixed_fan_outs() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let joining = task.register_node(IntNode);
        let normal = task.register_node(IntNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([joining.target_with(input), normal.target_with(input)])
        })
        .unwrap();
        task.register_transition(joining, join.join().all_fanout_branches())
            .unwrap();
        task.register_transition(normal, task.transitions_to_finish())
            .unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let error = task.run(1).await.unwrap_err();
        assert!(matches!(error, TaskError::InvalidState(_)));
    }

    #[test_log::test(tokio::test)]
    async fn register_transition_async_accepts_future_without_boxing() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let next = task.register_node(IntNode);

        task.starts_with(start);

        task.register_transition_async(
            start,
            move |input| async move { next.transitions_with(input) },
        )
        .unwrap();
        task.register_transition(next, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(3));
    }

    #[test_log::test(tokio::test)]
    async fn register_transition_maps_join_payload() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let branch = task.register_node(IntNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([branch.target_with(input)])
        })
        .unwrap();
        task.register_transition(branch, join.join().map(|output| output * 2))
            .unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(6));
    }

    #[test_log::test(tokio::test)]
    async fn register_transition_async_maps_join_payload() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let branch = task.register_node(IntNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition(start, move |input| {
            Transition::fan_out([branch.target_with(input)])
        })
        .unwrap();
        task.register_transition_async(
            branch,
            join.join_at_least(1)
                .continue_remaining()
                .map_async(|output| async move { output * 2 }),
        )
        .unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(6));
    }

    #[test_log::test(tokio::test)]
    async fn max_parallelism_is_enforced() {
        let current = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let tracking = TrackingNode {
            current: current.clone(),
            max: max.clone(),
            delay: Duration::from_millis(25),
        };

        let mut task: Task<i32, i32> = Task::builder()
            .max_parallelism(NonZeroUsize::new(2).unwrap())
            .build();

        let start = task.register_node(OffsetNode(0));
        let first = task.register_node(tracking.clone());
        let second = task.register_node(tracking.clone());
        let third = task.register_node(tracking);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);
        task.register_transition(start, move |input| {
            Transition::fan_out([
                first.target_with(input),
                second.target_with(input),
                third.target_with(input),
            ])
            .concurrency_model(ConcurrencyModel::Parallel)
        })
        .unwrap();
        task.register_transition(first, join.join()).unwrap();
        task.register_transition(second, join.join()).unwrap();
        task.register_transition(third, join.join()).unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(2).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(6));
        assert!(max.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn conflicting_transition_registrations_are_rejected() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let next = task.register_node(IntNode);
        let join = task.register_node(SumJoinNode);

        task.register_transition(start, move |input| next.transitions_with(input))
            .unwrap();

        let error = task.register_transition(start, join.join()).unwrap_err();

        assert!(matches!(error, TaskError::InvalidState(_)));
    }
}
