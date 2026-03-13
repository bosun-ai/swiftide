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
    pin::Pin,
    sync::Arc,
};

use anyhow::anyhow;
use async_trait::async_trait;
use dyn_clone::DynClone;
use futures_util::{StreamExt as _, stream::FuturesUnordered};
use tracing::Instrument as _;

use super::{
    errors::{NodeError, TaskError},
    node::{NodeArg, NodeId, TaskNode},
    transition::{
        BranchEnvelope, BranchId, BranchOutcome, ConcurrencyModel, EffectiveTransitionSettings,
        ErrorBehavior, JoinInput, JoinLeftoverBehavior, JoinPolicy, PauseBehavior,
        TransitionAction, TransitionDirective,
    },
};

type BoxedTransitionFuture = Pin<Box<dyn Future<Output = TransitionDirective> + Send>>;
type TransitionHandler<Output> =
    Arc<dyn Fn(Output) -> BoxedTransitionFuture + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BranchGroupId(usize);

#[derive(Debug, Clone)]
struct TaskOptions {
    concurrency_model: ConcurrencyModel,
    pause_behavior: PauseBehavior,
    error_behavior: ErrorBehavior,
}

impl Default for TaskOptions {
    fn default() -> Self {
        Self {
            concurrency_model: ConcurrencyModel::Sequential,
            pause_behavior: PauseBehavior::DrainRunnable,
            error_behavior: ErrorBehavior::Local,
        }
    }
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

    pub fn build(self) -> Task<Input, Output> {
        Task::with_options(self.options)
    }
}

#[derive(Debug, Clone)]
struct ExecutionBranch {
    id: BranchId,
    current_node: usize,
    context: Arc<dyn Any + Send + Sync>,
    settings: EffectiveTransitionSettings,
    join_group: Option<BranchGroupId>,
}

#[derive(Debug, Clone)]
enum JoinMemberState {
    Pending,
    Paused,
    Ready(Arc<dyn Any + Send + Sync>),
    Failed(String),
    Cancelled,
    LateArrival,
}

impl JoinMemberState {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            JoinMemberState::Ready(_)
                | JoinMemberState::Failed(_)
                | JoinMemberState::Cancelled
                | JoinMemberState::LateArrival
        )
    }

    fn outcome(&self) -> BranchOutcome {
        match self {
            JoinMemberState::Pending => BranchOutcome::Pending,
            JoinMemberState::Paused => BranchOutcome::Paused,
            JoinMemberState::Ready(payload) => BranchOutcome::Ready(payload.clone()),
            JoinMemberState::Failed(message) => BranchOutcome::Failed(message.clone()),
            JoinMemberState::Cancelled => BranchOutcome::Cancelled,
            JoinMemberState::LateArrival => BranchOutcome::LateArrival,
        }
    }
}

#[derive(Debug, Clone)]
struct JoinGroupState {
    join_node_id: usize,
    policy: JoinPolicy,
    members: HashMap<BranchId, JoinMemberState>,
    member_order: Vec<BranchId>,
    ready_count: usize,
    fired: bool,
}

#[derive(Debug)]
struct EvaluatedBranch {
    branch: ExecutionBranch,
    directive: TransitionDirective,
}

#[derive(Debug)]
enum LoopControl<Output> {
    Continue,
    Pause,
    Complete(Output),
}

#[derive(Debug)]
pub struct Task<Input: NodeArg, Output: NodeArg> {
    nodes: Vec<Box<dyn AnyNodeExecutor>>,
    start_node: Option<usize>,
    sequential_branches: VecDeque<ExecutionBranch>,
    parallel_branches: VecDeque<ExecutionBranch>,
    paused_branches: HashMap<BranchId, ExecutionBranch>,
    join_groups: HashMap<BranchGroupId, JoinGroupState>,
    next_branch_id: usize,
    next_group_id: usize,
    last_start_context: Option<Arc<dyn Any + Send + Sync>>,
    options: TaskOptions,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<Input: NodeArg, Output: NodeArg> Clone for Task<Input, Output> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            start_node: self.start_node,
            sequential_branches: VecDeque::new(),
            parallel_branches: VecDeque::new(),
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
            sequential_branches: VecDeque::new(),
            parallel_branches: VecDeque::new(),
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

    #[tracing::instrument(skip(self, input), name = "task.run", err)]
    pub async fn run(&mut self, input: impl Into<Input>) -> Result<Option<Output>, TaskError> {
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
    pub async fn resume(&mut self) -> Result<Option<Output>, TaskError> {
        self.validate_transitions()?;
        self.start_task().await
    }

    pub fn register_node<T>(&mut self, node: T) -> NodeId<T>
    where
        T: TaskNode + 'static + Clone,
    {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        let executor = Box::new(NodeExecutor::<T::Input, T::Output, T::Error> {
            node: Box::new(node),
            node_id: Box::new(node_id.as_dyn()),
            transition_fn: Arc::new(move |_output| {
                Box::pin(async move { unreachable!("No transition registered for node {id}.") })
            }),
            transition_is_set: false,
        });

        self.nodes.push(executor);
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
        R: Into<TransitionDirective> + 'static,
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
        R: Into<TransitionDirective> + 'static,
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

        let Some(executor) = (&mut **node_executor as &mut dyn Any).downcast_mut::<NodeExecutor<
            From::Input,
            From::Output,
            From::Error,
        >>() else {
            unreachable!("Transition registration type mismatch");
        };

        executor.transition_fn = transition;
        executor.transition_is_set = true;
        Ok(())
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

    async fn start_task(&mut self) -> Result<Option<Output>, TaskError> {
        loop {
            if !self.parallel_branches.is_empty() {
                let branches = self.parallel_branches.drain(..).collect::<Vec<_>>();
                let results = {
                    let mut futures = FuturesUnordered::new();
                    for branch in branches {
                        futures.push(self.evaluate_branch(branch));
                    }

                    let mut results = Vec::new();
                    while let Some(result) = futures.next().await {
                        results.push(result);
                    }
                    results
                };

                for result in results {
                    match self.apply_branch_result(result?).await? {
                        LoopControl::Continue => {}
                        LoopControl::Pause => return Ok(None),
                        LoopControl::Complete(output) => return Ok(Some(output)),
                    }
                }
                continue;
            }

            let Some(branch) = self.sequential_branches.pop_front() else {
                break;
            };

            match self
                .apply_branch_result(self.evaluate_branch(branch).await?)
                .await?
            {
                LoopControl::Continue => {}
                LoopControl::Pause => return Ok(None),
                LoopControl::Complete(output) => return Ok(Some(output)),
            }
        }

        if !self.paused_branches.is_empty() {
            return Ok(None);
        }

        Ok(None)
    }

    async fn evaluate_branch(&self, branch: ExecutionBranch) -> Result<EvaluatedBranch, TaskError> {
        let mut span = tracing::info_span!(
            "task.step",
            node = branch.current_node,
            branch = branch.id.0
        );

        let node_executor = self
            .nodes
            .get(branch.current_node)
            .ok_or_else(|| TaskError::missing_node(branch.current_node))?;

        let directive = node_executor
            .evaluate_next(branch.context.clone())
            .instrument(span.clone())
            .await?;

        span = tracing::info_span!(
            "task.step.done",
            node = branch.current_node,
            branch = branch.id.0
        )
        .or_current();
        span.follows_from(span.id().clone());

        Ok(EvaluatedBranch { branch, directive })
    }

    async fn apply_branch_result(
        &mut self,
        evaluated: EvaluatedBranch,
    ) -> Result<LoopControl<Output>, TaskError> {
        let EvaluatedBranch {
            mut branch,
            directive,
        } = evaluated;
        let settings = branch.settings.with_overrides(directive.settings);

        match directive.action {
            TransitionAction::Next(next_node) => {
                branch.current_node = next_node.node_id;
                branch.context = next_node.context;
                branch.settings = settings;
                self.set_join_member_state(branch.join_group, branch.id, JoinMemberState::Pending);
                self.enqueue_branch(branch);
            }
            TransitionAction::FanOut { targets, join } => {
                let join_group = join.map(|(join_node_id, policy)| {
                    let group_id = self.next_group();
                    self.join_groups.insert(
                        group_id,
                        JoinGroupState {
                            join_node_id,
                            policy,
                            members: HashMap::new(),
                            member_order: Vec::new(),
                            ready_count: 0,
                            fired: false,
                        },
                    );
                    group_id
                });

                for target in targets {
                    let child_id = self.next_branch();
                    let child = ExecutionBranch {
                        id: child_id,
                        current_node: target.node_id,
                        context: target.context,
                        settings: settings.clone(),
                        join_group,
                    };

                    if let Some(group_id) = join_group {
                        let group = self.join_groups.get_mut(&group_id).unwrap();
                        group.member_order.push(child_id);
                        group.members.insert(child_id, JoinMemberState::Pending);
                    }

                    self.enqueue_branch(child);
                }
            }
            TransitionAction::Join(payload) => {
                let Some(group_id) = branch.join_group else {
                    return Err(TaskError::NodeError(NodeError::new(
                        anyhow!("Join directive used without an attached join group"),
                        branch.current_node,
                        None,
                    )));
                };

                let group = self
                    .join_groups
                    .get_mut(&group_id)
                    .ok_or_else(|| TaskError::missing_node(branch.current_node))?;

                if group.fired {
                    group
                        .members
                        .insert(branch.id, JoinMemberState::LateArrival);
                    return Ok(LoopControl::Continue);
                }

                group
                    .members
                    .insert(branch.id, JoinMemberState::Ready(payload));
                group.ready_count += 1;

                if let Some(join_branch) = self.try_fire_join(group_id)? {
                    self.enqueue_branch(join_branch);
                }
            }
            TransitionAction::Pause => {
                self.set_join_member_state(branch.join_group, branch.id, JoinMemberState::Paused);
                self.paused_branches.insert(branch.id, branch);

                if settings.pause_behavior == PauseBehavior::PauseTask {
                    return Ok(LoopControl::Pause);
                }
            }
            TransitionAction::Error(error) => {
                if branch.join_group.is_none() || settings.error_behavior == ErrorBehavior::FailTask
                {
                    return Err(TaskError::NodeError(NodeError::new(
                        error,
                        branch.current_node,
                        None,
                    )));
                }

                self.set_join_member_state(
                    branch.join_group,
                    branch.id,
                    JoinMemberState::Failed(error.to_string()),
                );

                if let Some(group_id) = branch.join_group {
                    if let Some(join_branch) = self.try_fire_join(group_id)? {
                        self.enqueue_branch(join_branch);
                    }
                }
            }
            TransitionAction::Finish(output) => {
                self.clear_runtime_state();
                let output = output
                    .downcast::<Output>()
                    .map_err(|error| TaskError::type_error(&error))?
                    .as_ref()
                    .clone();
                return Ok(LoopControl::Complete(output));
            }
        }

        Ok(LoopControl::Continue)
    }

    fn try_fire_join(
        &mut self,
        group_id: BranchGroupId,
    ) -> Result<Option<ExecutionBranch>, TaskError> {
        let ready = {
            let Some(group) = self.join_groups.get(&group_id) else {
                return Ok(None);
            };

            if group.fired {
                return Ok(None);
            }

            match group.policy {
                JoinPolicy::All => group.members.values().all(JoinMemberState::is_terminal),
                JoinPolicy::AtLeast { count, .. } => group.ready_count >= count,
            }
        };

        if !ready {
            return Ok(None);
        }

        let (join_node_id, leftover_behavior) = {
            let group = self.join_groups.get_mut(&group_id).unwrap();
            group.fired = true;
            (group.join_node_id, group.policy.leftover_behavior())
        };

        if let Some(leftover_behavior) = leftover_behavior {
            self.apply_leftover_behavior(group_id, leftover_behavior);
        }

        let join_input = self.build_join_input(group_id)?;

        Ok(Some(ExecutionBranch {
            id: self.next_branch(),
            current_node: join_node_id,
            context: Arc::new(join_input) as Arc<dyn Any + Send + Sync>,
            settings: self.default_settings(),
            join_group: None,
        }))
    }

    fn build_join_input(&self, group_id: BranchGroupId) -> Result<JoinInput, TaskError> {
        let group = self
            .join_groups
            .get(&group_id)
            .ok_or_else(|| TaskError::missing_node(group_id.0))?;

        let branches = group
            .member_order
            .iter()
            .filter_map(|branch_id| {
                group.members.get(branch_id).map(|state| BranchEnvelope {
                    branch_id: *branch_id,
                    outcome: state.outcome(),
                })
            })
            .collect();

        Ok(JoinInput::new(branches))
    }

    fn apply_leftover_behavior(
        &mut self,
        group_id: BranchGroupId,
        leftover_behavior: JoinLeftoverBehavior,
    ) {
        if leftover_behavior != JoinLeftoverBehavior::CancelRemaining {
            return;
        }

        let branch_ids = self
            .join_groups
            .get(&group_id)
            .map(|group| {
                group
                    .members
                    .iter()
                    .filter_map(|(branch_id, state)| (!state.is_terminal()).then_some(*branch_id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        self.sequential_branches
            .retain(|branch| !branch_ids.contains(&branch.id));
        self.parallel_branches
            .retain(|branch| !branch_ids.contains(&branch.id));

        for branch_id in branch_ids {
            self.paused_branches.remove(&branch_id);
            self.set_join_member_state(Some(group_id), branch_id, JoinMemberState::Cancelled);
        }
    }

    fn set_join_member_state(
        &mut self,
        group_id: Option<BranchGroupId>,
        branch_id: BranchId,
        state: JoinMemberState,
    ) {
        if let Some(group_id) = group_id {
            if let Some(group) = self.join_groups.get_mut(&group_id) {
                group.members.insert(branch_id, state);
            }
        }
    }

    fn enqueue_branch(&mut self, branch: ExecutionBranch) {
        match branch.settings.concurrency_model {
            ConcurrencyModel::Sequential => self.sequential_branches.push_back(branch),
            ConcurrencyModel::Parallel => self.parallel_branches.push_back(branch),
        }
    }

    fn default_settings(&self) -> EffectiveTransitionSettings {
        EffectiveTransitionSettings {
            concurrency_model: self.options.concurrency_model,
            pause_behavior: self.options.pause_behavior,
            error_behavior: self.options.error_behavior,
        }
    }

    fn next_branch(&mut self) -> BranchId {
        let id = BranchId(self.next_branch_id);
        self.next_branch_id += 1;
        id
    }

    fn next_group(&mut self) -> BranchGroupId {
        let id = BranchGroupId(self.next_group_id);
        self.next_group_id += 1;
        id
    }

    fn clear_runtime_state(&mut self) {
        self.sequential_branches.clear();
        self.parallel_branches.clear();
        self.paused_branches.clear();
        self.join_groups.clear();
    }

    fn reset_runtime(&mut self) {
        self.clear_runtime_state();
    }
}

#[async_trait]
trait AnyNodeExecutor: Any + Send + Sync + std::fmt::Debug + DynClone {
    fn transition_is_set(&self) -> bool;

    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<TransitionDirective, NodeError>;
}

dyn_clone::clone_trait_object!(AnyNodeExecutor);

struct NodeExecutor<
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
> {
    node: Box<dyn TaskNode<Input = Input, Output = Output, Error = Error> + Send + Sync>,
    node_id: Box<NodeId<dyn TaskNode<Input = Input, Output = Output, Error = Error>>>,
    transition_fn: TransitionHandler<Output>,
    transition_is_set: bool,
}

impl<Input, Output, Error> Clone for NodeExecutor<Input, Output, Error>
where
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            node_id: self.node_id.clone(),
            transition_fn: self.transition_fn.clone(),
            transition_is_set: self.transition_is_set,
        }
    }
}

impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    std::fmt::Debug for NodeExecutor<Input, Output, Error>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeExecutor")
            .field("node_id", &self.node_id.id())
            .field("transition_is_set", &self.transition_is_set)
            .finish()
    }
}

#[async_trait]
impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    AnyNodeExecutor for NodeExecutor<Input, Output, Error>
{
    fn transition_is_set(&self) -> bool {
        self.transition_is_set
    }

    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<TransitionDirective, NodeError> {
        let context = context.downcast::<Input>().unwrap();

        match self.node.evaluate(&self.node_id.as_dyn(), &context).await {
            Ok(output) => Ok((self.transition_fn)(output).await),
            Err(error) => Err(NodeError::new(error, self.node_id.id(), None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

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
        task.register_transition(node3, TransitionDirective::finish)
            .unwrap();

        let res = task.run(1).await.unwrap();
        assert_eq!(res, Some(4));

        task.reset();

        let rerun = task.resume().await.unwrap();
        assert_eq!(rerun, Some(4));
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
            TransitionDirective::fan_out_join(
                [branch_a.target_with(input), branch_b.target_with(input)],
                join,
                JoinPolicy::All,
            )
        })
        .unwrap();

        task.register_transition(branch_a, TransitionDirective::join)
            .unwrap();
        task.register_transition(branch_b, TransitionDirective::join)
            .unwrap();
        task.register_transition(join, TransitionDirective::finish)
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, Some(6));
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
            TransitionDirective::fan_out_join(
                [active.target_with(input), paused.target_with(input)],
                join,
                JoinPolicy::AtLeast {
                    count: 1,
                    leftovers: JoinLeftoverBehavior::Continue,
                },
            )
        })
        .unwrap();
        task.register_transition(active, TransitionDirective::join)
            .unwrap();
        task.register_transition(paused, move |_output| TransitionDirective::pause())
            .unwrap();
        task.register_transition(join, TransitionDirective::finish)
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, Some(3));
    }

    #[test_log::test(tokio::test)]
    async fn pause_behavior_can_pause_task() {
        let mut task: Task<i32, i32> = Task::builder()
            .pause_behavior(PauseBehavior::PauseTask)
            .build();

        let start = task.register_node(PauseOnceNode);
        task.starts_with(start);

        task.register_transition(start, move |_output| TransitionDirective::pause())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, None);
    }

    #[test_log::test(tokio::test)]
    async fn error_behavior_can_fail_task() {
        let mut task: Task<i32, i32> = Task::builder()
            .error_behavior(ErrorBehavior::FailTask)
            .build();

        let start = task.register_node(FailingNode);
        task.starts_with(start);

        task.register_transition(start, TransitionDirective::finish)
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
            TransitionDirective::fan_out_join(
                [first.target_with(input), second.target_with(input)],
                join,
                JoinPolicy::All,
            )
        })
        .unwrap();
        task.register_transition(first, TransitionDirective::join)
            .unwrap();
        task.register_transition(second, TransitionDirective::join)
            .unwrap();
        task.register_transition(join, TransitionDirective::finish)
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, Some(vec![2, 11]));
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
        task.register_transition(next, TransitionDirective::finish)
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, Some(3));
    }
}
