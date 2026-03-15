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
    sync::Arc,
};

use super::{
    errors::TaskError,
    node::{NodeArg, NodeId, TaskNode},
    runtime::{
        AnyNodeExecutor, BranchGroupId, ExecutionBranch, JoinGroupState, NodeExecutor, TaskOptions,
        TransitionHandler,
    },
    transition::{
        ActiveBranch, BranchId, ConcurrencyModel, ErrorBehavior, PauseBehavior, Transition,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRunState<Output> {
    Completed(Output),
    Paused,
}

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

    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.options.concurrency_model = concurrency_model;
        self
    }

    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.options.pause_behavior = pause_behavior;
        self
    }

    pub fn error_behavior(mut self, error_behavior: ErrorBehavior) -> Self {
        self.options.error_behavior = error_behavior;
        self
    }

    pub fn max_parallelism(mut self, max_parallelism: NonZeroUsize) -> Self {
        self.options.max_parallelism = max_parallelism.get();
        self
    }

    pub fn build(self) -> Task<Input, Output> {
        Task::with_options(self.options)
    }
}

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
    pub fn builder() -> TaskBuilder<Input, Output> {
        TaskBuilder::new()
    }

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

    pub fn starts_with<T: TaskNode<Input = Input> + Clone + 'static>(
        &mut self,
        node_id: NodeId<T>,
    ) {
        self.start_node = Some(node_id.id());
        self.reset_runtime();
    }

    pub fn transitions_to_finish(&self) -> impl Fn(Output) -> Transition + Send + Sync + 'static {
        |output| Transition::finish(output)
    }

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

    #[tracing::instrument(skip(self), name = "task.resume", err)]
    pub async fn resume(&mut self) -> Result<TaskRunState<Output>, TaskError> {
        self.validate_transitions()?;

        if self.runnable_branches.is_empty() && self.paused_branches.is_empty() {
            return Err(TaskError::NotResumable);
        }

        self.restore_paused_branches();
        self.start_task().await
    }

    pub fn active_branches(&self) -> Vec<ActiveBranch> {
        self.runnable_branches
            .iter()
            .map(|branch| ActiveBranch {
                branch_id: branch.id,
                node_id: branch.current_node,
            })
            .collect()
    }

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

    pub fn register_node<T>(&mut self, node: T) -> NodeId<T>
    where
        T: TaskNode + 'static + Clone,
    {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        self.nodes.push(Box::new(NodeExecutor::new(node, node_id)));
        node_id
    }

    pub fn register_transition<From, F, R>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        F: Fn(From::Output) -> R + Send + Sync + 'static,
        R: Into<Transition> + 'static,
    {
        let transition = Arc::new(transition);
        self.set_transition_handler(
            from,
            Arc::new(move |output: From::Output| {
                let transition = transition.clone();
                Box::pin(async move { transition(output).into() })
            }),
        )
    }

    pub fn register_transition_async<From, F, Fut, R>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        F: Fn(From::Output) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
        R: Into<Transition> + 'static,
    {
        let transition = Arc::new(transition);
        self.set_transition_handler(
            from,
            Arc::new(move |output: From::Output| {
                let transition = transition.clone();
                Box::pin(async move { transition(output).await.into() })
            }),
        )
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

        executor.transition_fn = transition;
        executor.transition_is_set = true;
        Ok(())
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
    use crate::tasks::{JoinInput, JoinLeftoverBehavior, JoinPolicy};

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
            Transition::fan_out_join(
                [branch_a.target_with(input), branch_b.target_with(input)],
                join,
                JoinPolicy::All,
            )
        })
        .unwrap();

        task.register_transition(branch_a, Transition::join)
            .unwrap();
        task.register_transition(branch_b, Transition::join)
            .unwrap();
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
            Transition::fan_out_join(
                [active.target_with(input), paused.target_with(input)],
                join,
                JoinPolicy::AtLeast {
                    count: 1,
                    leftovers: JoinLeftoverBehavior::Continue,
                },
            )
            .concurrency_model(ConcurrencyModel::Parallel)
        })
        .unwrap();
        task.register_transition(active, Transition::join).unwrap();
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
            Transition::fan_out_join(
                [first.target_with(input), second.target_with(input)],
                join,
                JoinPolicy::All,
            )
            .concurrency_model(ConcurrencyModel::Parallel)
        })
        .unwrap();
        task.register_transition(first, Transition::join).unwrap();
        task.register_transition(second, Transition::join).unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(vec![2, 11]));
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
            Transition::fan_out_join(
                [
                    first.target_with(input),
                    second.target_with(input),
                    third.target_with(input),
                ],
                join,
                JoinPolicy::All,
            )
            .concurrency_model(ConcurrencyModel::Parallel)
        })
        .unwrap();
        task.register_transition(first, Transition::join).unwrap();
        task.register_transition(second, Transition::join).unwrap();
        task.register_transition(third, Transition::join).unwrap();
        task.register_transition(join, task.transitions_to_finish())
            .unwrap();

        let result = task.run(2).await.unwrap();
        assert_eq!(result, TaskRunState::Completed(6));
        assert!(max.load(Ordering::SeqCst) <= 2);
    }
}
