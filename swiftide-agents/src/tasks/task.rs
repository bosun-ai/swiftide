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
        ActiveBranch, BranchId, ConcurrencyModel, ErrorBehavior, JoinDefinition, JoinInput,
        JoinTarget, MappedJoinTarget, MarkedTransition, PauseBehavior, Transition,
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

/// Restorable transition settings for a seeded runtime branch or join group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeBranchSettings {
    /// Concurrency model applied when the branch or join schedules follow-up work.
    pub concurrency_model: ConcurrencyModel,
    /// Pause behavior applied when the branch pauses again after restore.
    pub pause_behavior: PauseBehavior,
    /// Error behavior applied after restore.
    pub error_behavior: ErrorBehavior,
}

impl Default for RuntimeBranchSettings {
    fn default() -> Self {
        Self {
            concurrency_model: ConcurrencyModel::Sequential,
            pause_behavior: PauseBehavior::DrainRunnable,
            error_behavior: ErrorBehavior::Local,
        }
    }
}

/// Serializable state for one queued or paused runtime branch.
#[derive(Debug, Clone)]
pub struct RestoredBranch<Context> {
    /// Runtime branch identifier to preserve across restore.
    pub branch_id: BranchId,
    /// Node where the branch should continue.
    pub node_id: usize,
    /// Typed input that will be provided to `node_id`.
    pub context: Context,
    /// Effective transition settings already attached to the branch.
    pub settings: RuntimeBranchSettings,
    /// Optional join group this branch currently belongs to.
    pub join_group_id: Option<usize>,
}

/// Serializable state for one branch tracked by a seeded join group.
#[derive(Debug, Clone)]
pub struct RestoredJoinMember<Context> {
    /// Runtime branch identifier for the join member.
    pub branch_id: BranchId,
    /// Node that produced or currently owns this join member state.
    pub node_id: usize,
    /// Current join-visible state for the member.
    pub outcome: RestoredJoinMemberOutcome<Context>,
}

/// Serializable join-member outcome used to seed task runtime state.
#[derive(Debug, Clone)]
pub enum RestoredJoinMemberOutcome<Context> {
    /// The branch is still expected to reach the join.
    Pending,
    /// The branch is paused before reaching the join.
    Paused,
    /// The branch produced a ready payload for the join.
    Ready(Context),
    /// The branch failed locally with the provided message.
    Failed(String),
    /// The branch was cancelled before producing a join payload.
    Cancelled,
    /// The branch completed after the join had already fired.
    LateArrival,
}

/// Serializable state for one join group that still exists in the runtime frontier.
#[derive(Debug, Clone)]
pub struct RestoredJoinGroup<Context> {
    /// Stable numeric identifier for the join group.
    pub group_id: usize,
    /// Join node that should fire when the policy is satisfied.
    pub join_node_id: usize,
    /// Join policy that was active when the task paused.
    pub policy: super::transition::JoinPolicy,
    /// Effective settings the join branch should inherit once scheduled.
    pub settings: RuntimeBranchSettings,
    /// Members observed by the join group in stable order.
    pub members: Vec<RestoredJoinMember<Context>>,
}

/// Complete runtime state used to restore or reshape a paused task frontier.
#[derive(Debug, Clone)]
pub struct TaskRuntimeState<Context> {
    /// Original start input, used to preserve `Task::reset` semantics after restore.
    pub last_start_context: Option<Context>,
    /// Runnable branches that should be queued immediately.
    pub runnable_branches: Vec<RestoredBranch<Context>>,
    /// Paused branches that should remain paused until `Task::resume`.
    pub paused_branches: Vec<RestoredBranch<Context>>,
    /// Join groups that still own branch state.
    pub join_groups: Vec<RestoredJoinGroup<Context>>,
    /// Whether `Task::resume` should automatically promote paused branches into runnable work.
    ///
    /// Tasks paused by normal execution use `true`. External orchestrators that selectively unlock
    /// only part of the paused frontier should set this to `false`, place the unlocked branches in
    /// `runnable_branches`, and leave the rest in `paused_branches`.
    pub resume_paused_branches: bool,
    /// Next runtime branch identifier to allocate.
    pub next_branch_id: usize,
    /// Next join-group identifier to allocate.
    pub next_group_id: usize,
}

impl<Context> Default for TaskRuntimeState<Context> {
    fn default() -> Self {
        Self {
            last_start_context: None,
            runnable_branches: Vec::new(),
            paused_branches: Vec::new(),
            join_groups: Vec::new(),
            resume_paused_branches: true,
            next_branch_id: 1,
            next_group_id: 1,
        }
    }
}

/// Backward-compatible alias for [`TaskRuntimeState`].
#[deprecated(note = "use TaskRuntimeState instead")]
pub type TaskRuntimeSeed<Context> = TaskRuntimeState<Context>;

/// Configures default runtime behavior for a [`Task`].
#[derive(Debug)]
#[must_use]
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
    pub(crate) resume_paused_branches: bool,
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

trait TransitionResult<From: TaskNode + ?Sized> {
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

impl<From, To, F, Fut, Payload> RegisterTransitionAsync<From> for MappedJoinTarget<To, F>
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
            nodes: self.nodes.clone(),
            start_node: self.start_node,
            runnable_branches: VecDeque::new(),
            paused_branches: HashMap::new(),
            join_groups: HashMap::new(),
            next_branch_id: 1,
            next_group_id: 1,
            last_start_context: None,
            resume_paused_branches: true,
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::num::NonZeroUsize;
    ///
    /// use swiftide_agents::tasks::{ConcurrencyModel, Task};
    ///
    /// let task = Task::<i32, i32>::builder()
    ///     .concurrency_model(ConcurrencyModel::Parallel)
    ///     .max_parallelism(NonZeroUsize::new(4).expect("non-zero"))
    ///     .build();
    ///
    /// let _ = task;
    /// ```
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
            resume_paused_branches: true,
            options,
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
        self.clear_runtime_state();
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
        if self.has_live_state() {
            return Err(TaskError::TaskActive);
        }

        let start_node = self.validate_transitions()?;
        let context = Arc::new(input.into()) as Arc<dyn Any + Send + Sync>;
        self.last_start_context = Some(context.clone());
        self.resume_paused_branches = true;
        self.clear_runtime_state();
        self.enqueue_start_branch(start_node, context);

        self.start_task().await
    }

    /// Resets runtime state while keeping the graph definition and last start input.
    ///
    /// After calling `reset`, use [`Task::resume`] to rerun the task from the start node with the
    /// most recent input passed to [`Task::run`].
    pub fn reset(&mut self) {
        let Some(start_node) = self.start_node else {
            self.clear_runtime_state();
            return;
        };

        let context = self.last_start_context.clone();
        self.resume_paused_branches = true;
        self.clear_runtime_state();

        if let Some(context) = context {
            self.enqueue_start_branch(start_node, context);
        }
    }

    /// Replaces the runtime frontier with a restored task state while keeping the task graph.
    ///
    /// Call this before [`Task::resume`] when an external system has reconstructed the paused task
    /// frontier and wants Swiftide to continue from that exact state.
    ///
    /// Ready join groups are rehydrated as join-group state first and then promoted back into
    /// runnable join work before this method returns, so callers only need to seed join members
    /// and not synthetic join-node branches.
    pub fn restore_runtime_state(
        &mut self,
        state: TaskRuntimeState<Input>,
    ) -> Result<(), TaskError> {
        if self.has_live_state() {
            return Err(TaskError::TaskActive);
        }

        self.clear_runtime_state();
        self.last_start_context = state
            .last_start_context
            .map(|context| Arc::new(context) as Arc<dyn Any + Send + Sync>);
        self.resume_paused_branches = state.resume_paused_branches;

        self.next_branch_id = state.next_branch_id.max(1);
        self.next_group_id = state.next_group_id.max(1);

        self.join_groups = state
            .join_groups
            .into_iter()
            .map(|group| {
                let group_id = BranchGroupId(group.group_id);
                let mut ready_count = 0;
                let mut member_order = Vec::with_capacity(group.members.len());
                let mut members = HashMap::with_capacity(group.members.len());

                for member in group.members {
                    member_order.push(member.branch_id);
                    let state = match member.outcome {
                        RestoredJoinMemberOutcome::Pending => {
                            super::runtime::JoinMemberState::Pending {
                                node_id: member.node_id,
                            }
                        }
                        RestoredJoinMemberOutcome::Paused => {
                            super::runtime::JoinMemberState::Paused {
                                node_id: member.node_id,
                            }
                        }
                        RestoredJoinMemberOutcome::Ready(context) => {
                            ready_count += 1;
                            super::runtime::JoinMemberState::Ready {
                                node_id: member.node_id,
                                payload: Arc::new(context) as Arc<dyn Any + Send + Sync>,
                            }
                        }
                        RestoredJoinMemberOutcome::Failed(message) => {
                            super::runtime::JoinMemberState::Failed {
                                node_id: member.node_id,
                                message,
                            }
                        }
                        RestoredJoinMemberOutcome::Cancelled => {
                            super::runtime::JoinMemberState::Cancelled {
                                node_id: member.node_id,
                            }
                        }
                        RestoredJoinMemberOutcome::LateArrival => {
                            super::runtime::JoinMemberState::LateArrival {
                                node_id: member.node_id,
                            }
                        }
                    };
                    members.insert(member.branch_id, state);
                }

                (
                    group_id,
                    JoinGroupState {
                        join_node_id: group.join_node_id,
                        policy: group.policy,
                        settings: super::transition::EffectiveTransitionSettings {
                            concurrency_model: group.settings.concurrency_model,
                            pause_behavior: group.settings.pause_behavior,
                            error_behavior: group.settings.error_behavior,
                        },
                        members,
                        member_order,
                        ready_count,
                        fired: false,
                    },
                )
            })
            .collect();

        self.runnable_branches = state
            .runnable_branches
            .into_iter()
            .map(|branch| ExecutionBranch {
                id: branch.branch_id,
                current_node: branch.node_id,
                context: Arc::new(branch.context) as Arc<dyn Any + Send + Sync>,
                settings: super::transition::EffectiveTransitionSettings {
                    concurrency_model: branch.settings.concurrency_model,
                    pause_behavior: branch.settings.pause_behavior,
                    error_behavior: branch.settings.error_behavior,
                },
                join_group: branch.join_group_id.map(BranchGroupId),
            })
            .collect();

        self.paused_branches = state
            .paused_branches
            .into_iter()
            .map(|branch| {
                (
                    branch.branch_id,
                    ExecutionBranch {
                        id: branch.branch_id,
                        current_node: branch.node_id,
                        context: Arc::new(branch.context) as Arc<dyn Any + Send + Sync>,
                        settings: super::transition::EffectiveTransitionSettings {
                            concurrency_model: branch.settings.concurrency_model,
                            pause_behavior: branch.settings.pause_behavior,
                            error_behavior: branch.settings.error_behavior,
                        },
                        join_group: branch.join_group_id.map(BranchGroupId),
                    },
                )
            })
            .collect();

        let ready_groups = self.join_groups.keys().copied().collect::<Vec<_>>();
        for group_id in ready_groups {
            if let Some(join_branch) = self.try_fire_join(group_id)? {
                self.enqueue_branch(join_branch);
            }
        }

        Ok(())
    }

    /// Exports the current runtime frontier into a [`TaskRuntimeState`].
    ///
    /// This is the public counterpart to [`Task::restore_runtime_state`]. It lets external
    /// orchestrators capture the task's current runnable branches, paused branches, and join state,
    /// then restore that state into a freshly built task before calling [`Task::resume`].
    ///
    /// The returned seed is a detached copy. Mutating it does not affect the current task.
    pub fn runtime_state(&self) -> Result<TaskRuntimeState<Input>, TaskError> {
        let last_start_context = self
            .last_start_context
            .as_ref()
            .map(|context| clone_arc_value::<Input>(context, "last_start_context"))
            .transpose()?;

        let runnable_branches = self
            .runnable_branches
            .iter()
            .map(export_branch::<Input>)
            .collect::<Result<Vec<_>, _>>()?;

        let mut paused = self.paused_branches.values().collect::<Vec<_>>();
        paused.sort_by_key(|branch| branch.id.0);
        let paused_branches = paused
            .into_iter()
            .map(export_branch::<Input>)
            .collect::<Result<Vec<_>, _>>()?;

        let mut join_groups = self.join_groups.iter().collect::<Vec<_>>();
        join_groups.sort_by_key(|(group_id, _)| group_id.0);
        let join_groups = join_groups
            .into_iter()
            .map(|(group_id, group)| export_join_group::<Input>(*group_id, group))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TaskRuntimeState {
            last_start_context,
            runnable_branches,
            paused_branches,
            join_groups,
            resume_paused_branches: self.resume_paused_branches,
            next_branch_id: self.next_branch_id,
            next_group_id: self.next_group_id,
        })
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

        if self.runnable_branches.is_empty() && self.paused_branches.is_empty() {
            return Err(TaskError::NotResumable);
        }

        if self.resume_paused_branches {
            self.restore_paused_branches();
        }
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
        self.nodes.push(Box::new(NodeExecutor::new(node, node_id)));
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

    fn enqueue_start_branch(&mut self, start_node: usize, context: Arc<dyn Any + Send + Sync>) {
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

fn export_branch<Input: NodeArg + Clone>(
    branch: &ExecutionBranch,
) -> Result<RestoredBranch<Input>, TaskError> {
    Ok(RestoredBranch {
        branch_id: branch.id,
        node_id: branch.current_node,
        context: clone_arc_value::<Input>(&branch.context, "branch context")?,
        settings: RuntimeBranchSettings {
            concurrency_model: branch.settings.concurrency_model,
            pause_behavior: branch.settings.pause_behavior,
            error_behavior: branch.settings.error_behavior,
        },
        join_group_id: branch.join_group.map(|group| group.0),
    })
}

fn export_join_group<Input: NodeArg + Clone>(
    group_id: BranchGroupId,
    group: &JoinGroupState,
) -> Result<RestoredJoinGroup<Input>, TaskError> {
    let members = group
        .member_order
        .iter()
        .filter_map(|branch_id| {
            group.members.get(branch_id).map(|member| {
                Ok(RestoredJoinMember {
                    branch_id: *branch_id,
                    node_id: member.node_id(),
                    outcome: match member {
                        super::runtime::JoinMemberState::Pending { .. } => {
                            RestoredJoinMemberOutcome::Pending
                        }
                        super::runtime::JoinMemberState::Paused { .. } => {
                            RestoredJoinMemberOutcome::Paused
                        }
                        super::runtime::JoinMemberState::Ready { payload, .. } => {
                            RestoredJoinMemberOutcome::Ready(clone_arc_value::<Input>(
                                payload,
                                "join member payload",
                            )?)
                        }
                        super::runtime::JoinMemberState::Failed { message, .. } => {
                            RestoredJoinMemberOutcome::Failed(message.clone())
                        }
                        super::runtime::JoinMemberState::Cancelled { .. } => {
                            RestoredJoinMemberOutcome::Cancelled
                        }
                        super::runtime::JoinMemberState::LateArrival { .. } => {
                            RestoredJoinMemberOutcome::LateArrival
                        }
                    },
                })
            })
        })
        .collect::<Result<Vec<_>, TaskError>>()?;

    Ok(RestoredJoinGroup {
        group_id: group_id.0,
        join_node_id: group.join_node_id,
        policy: group.policy,
        settings: RuntimeBranchSettings {
            concurrency_model: group.settings.concurrency_model,
            pause_behavior: group.settings.pause_behavior,
            error_behavior: group.settings.error_behavior,
        },
        members,
    })
}

fn clone_arc_value<T: Clone + Send + Sync + 'static>(
    value: &Arc<dyn Any + Send + Sync>,
    label: &str,
) -> Result<T, TaskError> {
    value
        .downcast_ref::<T>()
        .cloned()
        .ok_or_else(|| TaskError::invalid_state(format!("Could not export {label}")))
}
