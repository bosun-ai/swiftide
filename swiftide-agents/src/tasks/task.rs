//! Tasks enable you to to define a graph of interacting nodes
//!
//! The nodes can be any type that implements the `TaskNode` trait, which defines how the node
//! will be evaluated with its input and output.
//!
//! Most swiftide primitives implement `TaskNode`, and it's easy to implement your own. Since how
//! agents interact is subject to taste, we recommend implementing your own.
//!
//! WARN: Here be dragons! This api is not stable yet. We are using it in production, and is
//! subject to rapid change. However, do not hesitate to open an issue if you find anything.
use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    pin::Pin,
    sync::Arc,
};

use anyhow::anyhow;
use futures_util::{StreamExt as _, stream::FuturesUnordered};
use tracing::Instrument as _;

use crate::tasks::{errors::NodeError, transition::TransitionFn};

use super::{
    errors::TaskError,
    node::{NodeArg, NodeId, NoopNode, TaskNode},
    transition::{
        ActiveBranch, AnyNodeTransition, BranchEnvelope, BranchErrorBehavior, BranchGroupId,
        BranchId, BranchOutcome, BranchPauseBehavior, JoinInput, JoinLeftoverBehavior, JoinPolicy,
        MarkedTransitionPayload, RunLoopBehavior, SchedulerKind, Transition, TransitionAction,
        TransitionDirective, TransitionPolicies,
    },
};

#[derive(Debug, Clone)]
pub struct TaskOptions {
    pub scheduler: SchedulerKind,
    pub run_loop_behavior: RunLoopBehavior,
    pub branch_pause_behavior: BranchPauseBehavior,
    pub branch_error_behavior: BranchErrorBehavior,
}

impl Default for TaskOptions {
    fn default() -> Self {
        Self {
            scheduler: SchedulerKind::Fifo,
            run_loop_behavior: RunLoopBehavior::DrainRunnable,
            branch_pause_behavior: BranchPauseBehavior::Local,
            branch_error_behavior: BranchErrorBehavior::Local,
        }
    }
}

#[derive(Debug)]
pub struct TaskBuilder<Input: NodeArg, Output: NodeArg> {
    options: TaskOptions,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> TaskBuilder<Input, Output> {
    fn new() -> Self {
        Self {
            options: TaskOptions::default(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn scheduler(mut self, scheduler: SchedulerKind) -> Self {
        self.options.scheduler = scheduler;
        self
    }

    pub fn run_loop_behavior(mut self, run_loop_behavior: RunLoopBehavior) -> Self {
        self.options.run_loop_behavior = run_loop_behavior;
        self
    }

    pub fn branch_pause_behavior(mut self, branch_pause_behavior: BranchPauseBehavior) -> Self {
        self.options.branch_pause_behavior = branch_pause_behavior;
        self
    }

    pub fn branch_error_behavior(mut self, branch_error_behavior: BranchErrorBehavior) -> Self {
        self.options.branch_error_behavior = branch_error_behavior;
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
    policies: TransitionPolicies,
    join_group: Option<BranchGroupId>,
}

#[derive(Debug, Clone)]
enum JoinMemberState {
    Pending {
        node_id: usize,
    },
    Paused {
        node_id: usize,
    },
    Ready {
        node_id: usize,
        payload: Arc<dyn Any + Send + Sync>,
    },
    Failed {
        node_id: usize,
        message: String,
    },
    Cancelled {
        node_id: usize,
    },
    LateArrival {
        node_id: usize,
    },
}

impl JoinMemberState {
    fn node_id(&self) -> usize {
        match self {
            JoinMemberState::Pending { node_id }
            | JoinMemberState::Paused { node_id }
            | JoinMemberState::Ready { node_id, .. }
            | JoinMemberState::Failed { node_id, .. }
            | JoinMemberState::Cancelled { node_id }
            | JoinMemberState::LateArrival { node_id } => *node_id,
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self,
            JoinMemberState::Ready { .. }
                | JoinMemberState::Failed { .. }
                | JoinMemberState::Cancelled { .. }
                | JoinMemberState::LateArrival { .. }
        )
    }
}

#[derive(Debug, Clone)]
struct JoinGroupState {
    join_node_id: usize,
    policy: JoinPolicy,
    leftover_behavior: Option<JoinLeftoverBehavior>,
    members: HashMap<BranchId, JoinMemberState>,
    success_order: Vec<BranchId>,
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
    nodes: Vec<Box<dyn AnyNodeTransition>>,
    start_node: usize,
    fifo_branches: VecDeque<ExecutionBranch>,
    parallel_branches: VecDeque<ExecutionBranch>,
    paused_branches: HashMap<BranchId, ExecutionBranch>,
    join_groups: HashMap<BranchGroupId, JoinGroupState>,
    next_branch_id: usize,
    next_group_id: usize,
    last_output: Option<Arc<dyn Any + Send + Sync>>,
    last_start_context: Option<Arc<dyn Any + Send + Sync>>,
    options: TaskOptions,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<Input: NodeArg, Output: NodeArg> Clone for Task<Input, Output> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            start_node: self.start_node,
            fifo_branches: VecDeque::new(),
            parallel_branches: VecDeque::new(),
            paused_branches: HashMap::new(),
            join_groups: HashMap::new(),
            next_branch_id: 1,
            next_group_id: 1,
            last_output: None,
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
        let noop = NoopNode::<Output>::default();

        let node_id = NodeId::new(0, &noop).as_dyn();

        let noop_executor = Box::new(Transition {
            node: Box::new(noop),
            node_id: Box::new(node_id),
            r#fn: Arc::new(|_output| {
                Box::pin(async { unreachable!("Done node should never be evaluated.") })
            }),
            is_set: false,
        });

        Self {
            nodes: vec![noop_executor],
            start_node: 0,
            fifo_branches: VecDeque::new(),
            parallel_branches: VecDeque::new(),
            paused_branches: HashMap::new(),
            join_groups: HashMap::new(),
            next_branch_id: 1,
            next_group_id: 1,
            last_output: None,
            last_start_context: None,
            options,
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns the current context as the input type when the task only has one active branch.
    pub fn current_input(&self) -> Option<&Input> {
        let branch = self.single_branch()?;

        branch.context.downcast_ref::<Input>()
    }

    /// Returns the current context as the output type for linear tasks or the last task output.
    pub fn current_output(&self) -> Option<&Output> {
        if let Some(output) = &self.last_output {
            return output.downcast_ref::<Output>();
        }

        let branch = self.single_branch()?;

        branch.context.downcast_ref::<Output>()
    }

    /// Returns the `done` node for this task
    pub fn done(&self) -> NodeId<NoopNode<Output>> {
        NodeId::new(0, &NoopNode::default())
    }

    /// Creates a transition to the done node
    pub fn transitions_to_done(
        &self,
    ) -> impl Fn(Output) -> MarkedTransitionPayload<NoopNode<Output>> + Send + Sync + 'static {
        let done = self.done();
        move |context| done.transitions_with(context)
    }

    /// Defines the start node of the task
    pub fn starts_with<T: TaskNode<Input = Input> + Clone + 'static>(
        &mut self,
        node_id: NodeId<T>,
    ) {
        self.start_node = node_id.id;
        self.reset_runtime();
    }

    /// Validates that all nodes have transitions set
    pub fn validate_transitions(&self) -> Result<(), TaskError> {
        for node_executor in &self.nodes {
            if node_executor.node_id() == 0 {
                continue;
            }

            if !node_executor.transition_is_set() {
                return Err(TaskError::missing_transition(node_executor.node_id()));
            }
        }

        Ok(())
    }

    /// Runs the task with the given input
    #[tracing::instrument(skip(self, input), name = "task.run", err)]
    pub async fn run(&mut self, input: impl Into<Input>) -> Result<Option<Output>, TaskError> {
        self.validate_transitions()?;

        let context = Arc::new(input.into()) as Arc<dyn Any + Send + Sync>;
        self.last_start_context = Some(context.clone());
        self.reset_runtime();
        let branch_id = self.next_branch();
        let policies = self.default_policies();
        self.enqueue_branch(ExecutionBranch {
            id: branch_id,
            current_node: self.start_node,
            context,
            policies,
            join_group: None,
        });

        self.start_task().await
    }

    /// Resets the task to the start node with the last run input, if available.
    pub fn reset(&mut self) {
        let context = self.last_start_context.clone();
        self.reset_runtime();

        if let Some(context) = context {
            let branch_id = self.next_branch();
            let policies = self.default_policies();
            self.enqueue_branch(ExecutionBranch {
                id: branch_id,
                current_node: self.start_node,
                context,
                policies,
                join_group: None,
            });
        }
    }

    /// Resumes the task from its current active and paused branches.
    #[tracing::instrument(skip(self), name = "task.resume", err)]
    pub async fn resume(&mut self) -> Result<Option<Output>, TaskError> {
        self.start_task().await
    }

    pub fn active_branches(&self) -> Vec<ActiveBranch> {
        self.fifo_branches
            .iter()
            .chain(self.parallel_branches.iter())
            .map(|branch| ActiveBranch {
                branch_id: branch.id,
                node_id: branch.current_node,
            })
            .collect()
    }

    pub fn paused_branches(&self) -> Vec<ActiveBranch> {
        self.paused_branches
            .values()
            .map(|branch| ActiveBranch {
                branch_id: branch.id,
                node_id: branch.current_node,
            })
            .collect()
    }

    async fn start_task(&mut self) -> Result<Option<Output>, TaskError> {
        self.validate_transitions()?;

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

            let Some(branch) = self.fifo_branches.pop_front() else {
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

        if let Some(output) = self.take_last_output()? {
            return Ok(Some(output));
        }

        Ok(None)
    }

    async fn evaluate_branch(&self, branch: ExecutionBranch) -> Result<EvaluatedBranch, TaskError> {
        let mut span = tracing::info_span!(
            "task.step",
            node = branch.current_node,
            branch = branch.id.0
        );

        let node_transition = self
            .nodes
            .get(branch.current_node)
            .ok_or_else(|| TaskError::missing_node(branch.current_node))?;

        let directive = node_transition
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
        let policies = branch.policies.with_overrides(directive.options);

        match directive.action {
            TransitionAction::Next(next_node) => {
                if next_node.node_id == 0 {
                    self.last_output = Some(next_node.context.clone());
                    self.clear_runnable_state();
                    self.paused_branches.clear();
                    self.join_groups.clear();

                    let output = self
                        .take_last_output()?
                        .ok_or_else(|| TaskError::missing_output(0))?;
                    return Ok(LoopControl::Complete(output));
                }

                branch.current_node = next_node.node_id;
                branch.context = next_node.context;
                branch.policies = policies;
                self.set_join_member_state(
                    branch.join_group,
                    branch.id,
                    JoinMemberState::Pending {
                        node_id: branch.current_node,
                    },
                );
                self.enqueue_branch(branch);
            }
            TransitionAction::FanOut(fan_out) => {
                let join_group = fan_out.join.map(|join| {
                    let group_id = self.next_group();
                    self.join_groups.insert(
                        group_id,
                        JoinGroupState {
                            join_node_id: join.join_node_id,
                            policy: join.policy,
                            leftover_behavior: join.leftover_behavior,
                            members: HashMap::new(),
                            success_order: Vec::new(),
                            fired: false,
                        },
                    );
                    group_id
                });

                for target in fan_out.targets {
                    let child_id = self.next_branch();
                    let child = ExecutionBranch {
                        id: child_id,
                        current_node: target.node_id,
                        context: target.context,
                        policies: policies.clone(),
                        join_group,
                    };
                    self.set_join_member_state(
                        join_group,
                        child_id,
                        JoinMemberState::Pending {
                            node_id: child.current_node,
                        },
                    );
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
                    group.members.insert(
                        branch.id,
                        JoinMemberState::LateArrival {
                            node_id: branch.current_node,
                        },
                    );
                    return Ok(LoopControl::Continue);
                }

                group.members.insert(
                    branch.id,
                    JoinMemberState::Ready {
                        node_id: branch.current_node,
                        payload,
                    },
                );
                group.success_order.push(branch.id);

                if let Some(join_branch) = self.try_fire_join(group_id)? {
                    self.enqueue_branch(join_branch);
                }
            }
            TransitionAction::Pause => {
                self.set_join_member_state(
                    branch.join_group,
                    branch.id,
                    JoinMemberState::Paused {
                        node_id: branch.current_node,
                    },
                );
                self.paused_branches.insert(branch.id, branch);

                if self.options.run_loop_behavior == RunLoopBehavior::PauseOnBranchPause {
                    return Ok(LoopControl::Pause);
                }
            }
            TransitionAction::Error(error) => {
                if branch.join_group.is_none()
                    || policies.error_behavior == BranchErrorBehavior::FailTask
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
                    JoinMemberState::Failed {
                        node_id: branch.current_node,
                        message: error.to_string(),
                    },
                );

                if let Some(group_id) = branch.join_group {
                    if let Some(join_branch) = self.try_fire_join(group_id)? {
                        self.enqueue_branch(join_branch);
                    }
                }
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
            self.join_ready(group)
        };

        if !ready {
            return Ok(None);
        }

        let (join_node_id, leftover_behavior) = {
            let group = self.join_groups.get_mut(&group_id).unwrap();
            group.fired = true;
            (group.join_node_id, Self::leftover_behavior(group))
        };

        if let Some(behavior) = leftover_behavior {
            self.apply_leftover_behavior(group_id, behavior);
        }

        let join_input = self.build_join_input(group_id)?;

        Ok(Some(ExecutionBranch {
            id: self.next_branch(),
            current_node: join_node_id,
            context: Arc::new(join_input) as Arc<dyn Any + Send + Sync>,
            policies: self.default_policies(),
            join_group: None,
        }))
    }

    fn build_join_input(&self, group_id: BranchGroupId) -> Result<JoinInput, TaskError> {
        let group = self
            .join_groups
            .get(&group_id)
            .ok_or_else(|| TaskError::missing_node(group_id.0))?;

        let branches = group
            .members
            .iter()
            .map(|(branch_id, state)| BranchEnvelope {
                branch_id: *branch_id,
                node_id: state.node_id(),
                outcome: match state {
                    JoinMemberState::Pending { .. } => BranchOutcome::Pending,
                    JoinMemberState::Paused { .. } => BranchOutcome::Paused,
                    JoinMemberState::Ready { payload, .. } => BranchOutcome::Ready(payload.clone()),
                    JoinMemberState::Failed { message, .. } => {
                        BranchOutcome::Failed(message.clone())
                    }
                    JoinMemberState::Cancelled { .. } => BranchOutcome::Cancelled,
                    JoinMemberState::LateArrival { .. } => BranchOutcome::LateArrival,
                },
            })
            .collect();

        Ok(JoinInput { branches })
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

        self.fifo_branches
            .retain(|branch| !branch_ids.contains(&branch.id));
        self.parallel_branches
            .retain(|branch| !branch_ids.contains(&branch.id));
        for branch_id in branch_ids {
            self.paused_branches.remove(&branch_id);
            self.set_join_member_state(
                Some(group_id),
                branch_id,
                JoinMemberState::Cancelled { node_id: 0 },
            );
        }
    }

    fn join_ready(&self, group: &JoinGroupState) -> bool {
        match group.policy {
            JoinPolicy::All => group.members.values().all(JoinMemberState::is_terminal),
            JoinPolicy::AtLeast(n) | JoinPolicy::First(n) => group.success_order.len() >= n,
        }
    }

    fn leftover_behavior(group: &JoinGroupState) -> Option<JoinLeftoverBehavior> {
        match group.policy {
            JoinPolicy::All => None,
            JoinPolicy::First(_) => Some(JoinLeftoverBehavior::CancelRemaining),
            JoinPolicy::AtLeast(_) => Some(
                group
                    .leftover_behavior
                    .unwrap_or(JoinLeftoverBehavior::CancelRemaining),
            ),
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
        match branch.policies.scheduler {
            SchedulerKind::Fifo => self.fifo_branches.push_back(branch),
            SchedulerKind::Parallel => self.parallel_branches.push_back(branch),
        }
    }

    fn default_policies(&self) -> TransitionPolicies {
        TransitionPolicies {
            scheduler: self.options.scheduler,
            pause_behavior: self.options.branch_pause_behavior,
            error_behavior: self.options.branch_error_behavior,
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

    fn clear_runnable_state(&mut self) {
        self.fifo_branches.clear();
        self.parallel_branches.clear();
    }

    fn reset_runtime(&mut self) {
        self.clear_runnable_state();
        self.paused_branches.clear();
        self.join_groups.clear();
        self.last_output = None;
    }

    fn single_branch(&self) -> Option<&ExecutionBranch> {
        let active = self
            .fifo_branches
            .iter()
            .chain(self.parallel_branches.iter())
            .chain(self.paused_branches.values())
            .collect::<Vec<_>>();

        if active.len() == 1 {
            active.into_iter().next()
        } else {
            None
        }
    }

    fn take_last_output(&mut self) -> Result<Option<Output>, TaskError> {
        let Some(output) = self.last_output.take() else {
            return Ok(None);
        };

        let output = output
            .downcast::<Output>()
            .map_err(|e| TaskError::type_error(&e))?
            .as_ref()
            .clone();

        Ok(Some(output))
    }

    /// Gets the current node of the task for linear task states.
    pub fn current_node<T: TaskNode + 'static>(&self) -> Option<&T> {
        let branch = self.single_branch()?;
        self.node_at_index(branch.current_node)
    }

    /// Gets the node at the given `NodeId`
    pub fn node_at<T: TaskNode + 'static>(&self, node_id: NodeId<T>) -> Option<&T> {
        self.node_at_index(node_id.id)
    }

    /// Gets the node at the given index
    pub fn node_at_index<T: TaskNode + 'static>(&self, index: usize) -> Option<&T> {
        let transition = self.transition_at_index::<T>(index)?;

        let node = &*transition.node;

        (node as &dyn Any).downcast_ref::<T>()
    }

    #[allow(dead_code)]
    fn current_transition<T: TaskNode + 'static>(
        &self,
    ) -> Option<&Transition<T::Input, T::Output, T::Error>> {
        let branch = self.single_branch()?;
        self.transition_at_index::<T>(branch.current_node)
    }

    fn transition_at_index<T: TaskNode + 'static>(
        &self,
        index: usize,
    ) -> Option<&Transition<T::Input, T::Output, T::Error>> {
        let transition = self.nodes.get(index)?;

        (&**transition as &dyn Any).downcast_ref::<Transition<T::Input, T::Output, T::Error>>()
    }

    /// Registers a new node in the task
    pub fn register_node<T>(&mut self, node: T) -> NodeId<T>
    where
        T: TaskNode + 'static + Clone,
        <T as TaskNode>::Input: Clone,
        <T as TaskNode>::Output: Clone,
    {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        let node_executor = Box::new(Transition::<T::Input, T::Output, T::Error> {
            node_id: Box::new(node_id.as_dyn()),
            node: Box::new(node),
            r#fn: Arc::new(move |_output| unreachable!("No transition for node {}.", node_id.id)),
            is_set: false,
        });

        self.nodes.push(node_executor);

        node_id
    }

    /// Registers a transition from one node to another.
    pub fn register_transition<'a, From, To, F>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        To: TaskNode<Input = From::Output> + 'a + ?Sized,
        F: Fn(To::Input) -> MarkedTransitionPayload<To> + Send + Sync + 'static,
    {
        self.register_transition_directive(from, move |output| transition(output).into())
    }

    pub fn register_transition_directive<From, F>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        F: Fn(From::Output) -> TransitionDirective + Send + Sync + 'static,
    {
        let node_executor = self
            .nodes
            .get_mut(from.id)
            .ok_or_else(|| TaskError::missing_node(from.id))?;

        let any_executor: &mut dyn Any = node_executor.as_mut();

        let Some(exec) =
            any_executor.downcast_mut::<Transition<From::Input, From::Output, From::Error>>()
        else {
            unreachable!("Transition registration type mismatch");
        };

        let transition = Arc::new(transition);
        let wrapped: Arc<dyn TransitionFn<From::Output>> = Arc::new(move |output: From::Output| {
            let transition = transition.clone();
            Box::pin(async move { transition(output) })
        });

        exec.r#fn = wrapped;
        exec.is_set = true;

        Ok(())
    }

    /// Registers an async transition from one node to another.
    pub fn register_transition_async<'a, From, To, F>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        To: TaskNode<Input = From::Output> + 'a + ?Sized,
        F: Fn(To::Input) -> Pin<Box<dyn Future<Output = MarkedTransitionPayload<To>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        self.register_transition_directive_async(from, move |output| {
            let future = transition(output);
            Box::pin(async move { future.await.into() })
        })
    }

    pub fn register_transition_directive_async<From, F>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        F: Fn(From::Output) -> Pin<Box<dyn Future<Output = TransitionDirective> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let node_executor = self
            .nodes
            .get_mut(from.id)
            .ok_or_else(|| TaskError::missing_node(from.id))?;

        let any_executor: &mut dyn Any = node_executor.as_mut();

        let Some(exec) =
            any_executor.downcast_mut::<Transition<From::Input, From::Output, From::Error>>()
        else {
            unreachable!("Transition registration type mismatch");
        };

        let transition = Arc::new(transition);
        let wrapped: Arc<dyn TransitionFn<From::Output>> = Arc::new(move |output: From::Output| {
            let transition = transition.clone();
            Box::pin(async move { transition(output).await })
        });

        exec.r#fn = wrapped;
        exec.is_set = true;

        Ok(())
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

    #[test_log::test(tokio::test)]
    async fn sequential_3_node_task_reset_works() {
        let mut task: Task<i32, i32> = Task::new();

        let node1 = task.register_node(IntNode);
        let node2 = task.register_node(IntNode);
        let node3 = task.register_node(IntNode);

        task.starts_with(node1);

        task.register_transition::<_, _, _>(node1, move |input| node2.transitions_with(input))
            .unwrap();
        task.register_transition::<_, _, _>(node2, move |input| node3.transitions_with(input))
            .unwrap();
        task.register_transition::<_, _, _>(node3, task.transitions_to_done())
            .unwrap();

        let res = task.run(1).await.unwrap();
        assert_eq!(res, Some(4));

        task.reset();

        let n1_transition = task.transition_at_index::<IntNode>(1);

        assert!(n1_transition.is_some());
        assert!(task.current_transition::<IntNode>().is_some());
        assert!(task.current_node::<IntNode>().is_some());
    }

    #[test_log::test(tokio::test)]
    async fn fan_out_can_join_multiple_branches() {
        let mut task: Task<i32, i32> = Task::new();

        let start = task.register_node(IntNode);
        let branch_a = task.register_node(IntNode);
        let branch_b = task.register_node(IntNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition_directive(start, move |input| {
            TransitionDirective::fan_out([branch_a.target_with(input), branch_b.target_with(input)])
                .with_join(join, JoinPolicy::All)
        })
        .unwrap();

        task.register_transition_directive(branch_a, move |output| {
            TransitionDirective::join(output)
        })
        .unwrap();
        task.register_transition_directive(branch_b, move |output| {
            TransitionDirective::join(output)
        })
        .unwrap();
        task.register_transition::<_, _, _>(join, task.transitions_to_done())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, Some(6));
    }

    #[test_log::test(tokio::test)]
    async fn paused_branch_keeps_other_branches_running() {
        let mut task: Task<i32, i32> = Task::builder()
            .run_loop_behavior(RunLoopBehavior::DrainRunnable)
            .build();

        let start = task.register_node(IntNode);
        let active = task.register_node(IntNode);
        let paused = task.register_node(PauseOnceNode);
        let join = task.register_node(SumJoinNode);

        task.starts_with(start);

        task.register_transition_directive(start, move |input| {
            TransitionDirective::fan_out([active.target_with(input), paused.target_with(input)])
                .with_join(join, JoinPolicy::AtLeast(1))
                .leftover_behavior(JoinLeftoverBehavior::Continue)
        })
        .unwrap();
        task.register_transition_directive(active, move |output| TransitionDirective::join(output))
            .unwrap();
        task.register_transition_directive(paused, move |_output| TransitionDirective::pause())
            .unwrap();
        task.register_transition::<_, _, _>(join, task.transitions_to_done())
            .unwrap();

        let result = task.run(1).await.unwrap();
        assert_eq!(result, Some(3));
        assert!(task.paused_branches().is_empty());
    }
}
