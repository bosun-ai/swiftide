use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use futures_util::{StreamExt as _, stream::FuturesUnordered};
use tracing::Instrument as _;

use super::{
    errors::{NodeError, TaskError},
    executor::EvaluatedTransition,
    task::TaskRunState,
    traits::{AnyNodeExecutor, NodeArg},
    transition::{
        BranchId, ConcurrencyModel, JoinDefinition, JoinInput, NextNode, Transition,
        TransitionAction,
    },
};

type RunningBranchFuture = Pin<Box<dyn Future<Output = Result<EvaluatedBranch, TaskError>> + Send>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BranchGroupId(pub(crate) usize);

#[derive(Debug, Clone)]
pub(crate) struct TaskOptions {
    pub(crate) concurrency_model: ConcurrencyModel,
}

impl Default for TaskOptions {
    fn default() -> Self {
        Self {
            concurrency_model: ConcurrencyModel::Sequential,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionBranch {
    pub(crate) id: BranchId,
    pub(crate) current_node: usize,
    pub(crate) context: Arc<dyn Any + Send + Sync>,
    pub(crate) concurrency_model: ConcurrencyModel,
    pub(crate) join_group: Option<BranchGroupId>,
}

#[derive(Debug, Default)]
pub(crate) struct Runtime {
    state: Option<RunState>,
    last_start_context: Option<Arc<dyn Any + Send + Sync>>,
}

#[derive(Debug)]
struct RunState {
    runnable_branches: VecDeque<ExecutionBranch>,
    paused_branches: HashMap<BranchId, ExecutionBranch>,
    join_groups: HashMap<BranchGroupId, JoinGroupState>,
    next_branch_id: usize,
    next_group_id: usize,
}

#[derive(Debug, Clone)]
enum JoinMemberState {
    Pending,
    Ready { payload: Arc<dyn Any + Send + Sync> },
}

impl JoinMemberState {
    fn is_terminal(&self) -> bool {
        matches!(self, JoinMemberState::Ready { .. })
    }
}

#[derive(Debug, Clone)]
struct JoinGroupState {
    join_node_id: usize,
    concurrency_model: ConcurrencyModel,
    members: HashMap<BranchId, JoinMemberState>,
    member_order: Vec<BranchId>,
}

#[derive(Debug)]
struct EvaluatedBranch {
    branch: ExecutionBranch,
    next_step: EvaluatedTransition,
}

#[derive(Debug)]
enum LoopControl<Output> {
    Continue,
    PauseRequested,
    Complete(Output),
}

impl Runtime {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_live(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(RunState::has_resumable_work)
    }

    pub(crate) fn clear_state(&mut self) {
        self.state = None;
    }

    pub(crate) fn current_node(&self) -> Option<usize> {
        self.state.as_ref().and_then(RunState::current_node)
    }

    pub(crate) async fn run<Input, Output>(
        &mut self,
        nodes: &[Arc<dyn AnyNodeExecutor>],
        start_node: usize,
        default_concurrency_model: ConcurrencyModel,
        input: Input,
    ) -> Result<TaskRunState<Output>, TaskError>
    where
        Input: NodeArg,
        Output: NodeArg + Clone,
    {
        if self.is_live() {
            return Err(TaskError::TaskActive);
        }

        let context = Arc::new(input) as Arc<dyn Any + Send + Sync>;
        self.last_start_context = Some(context.clone());
        self.state = Some(RunState::new(
            start_node,
            context,
            default_concurrency_model,
        ));

        self.start(nodes, default_concurrency_model).await
    }

    pub(crate) fn reset(
        &mut self,
        start_node: Option<usize>,
        default_concurrency_model: ConcurrencyModel,
    ) {
        self.clear_state();

        let (Some(start_node), Some(context)) = (start_node, self.last_start_context.clone())
        else {
            return;
        };

        self.state = Some(RunState::new(
            start_node,
            context,
            default_concurrency_model,
        ));
    }

    pub(crate) async fn resume<Output>(
        &mut self,
        nodes: &[Arc<dyn AnyNodeExecutor>],
        default_concurrency_model: ConcurrencyModel,
    ) -> Result<TaskRunState<Output>, TaskError>
    where
        Output: NodeArg + Clone,
    {
        let Some(state) = self.state.as_mut() else {
            return Err(TaskError::NotResumable);
        };

        if !state.has_resumable_work() {
            return Err(TaskError::NotResumable);
        }

        state.restore_paused_branches();
        self.start(nodes, default_concurrency_model).await
    }

    async fn start<Output>(
        &mut self,
        nodes: &[Arc<dyn AnyNodeExecutor>],
        default_concurrency_model: ConcurrencyModel,
    ) -> Result<TaskRunState<Output>, TaskError>
    where
        Output: NodeArg + Clone,
    {
        let mut state = self.state.take().ok_or(TaskError::NotResumable)?;
        let mut in_flight = FuturesUnordered::<RunningBranchFuture>::new();
        let mut pause_requested = false;

        loop {
            if !pause_requested {
                while let Some(branch) = state.runnable_branches.pop_front() {
                    match branch.concurrency_model {
                        ConcurrencyModel::Sequential => {
                            let execution_result = match Self::branch_future(nodes, branch)?.await {
                                Ok(result) => result,
                                Err(error) => return Err(error),
                            };

                            match state
                                .apply_branch_result(execution_result, default_concurrency_model)?
                            {
                                LoopControl::Continue => {}
                                LoopControl::PauseRequested => {
                                    pause_requested = true;
                                    break;
                                }
                                LoopControl::Complete(output) => {
                                    return Ok(TaskRunState::Completed(output));
                                }
                            }
                        }
                        ConcurrencyModel::Parallel => {
                            in_flight.push(Self::branch_future(nodes, branch)?);
                        }
                    }
                }
            }

            if let Some(result) = in_flight.next().await {
                let execution_result = result?;

                match state.apply_branch_result(execution_result, default_concurrency_model)? {
                    LoopControl::Continue => continue,
                    LoopControl::PauseRequested => {
                        pause_requested = true;
                        continue;
                    }
                    LoopControl::Complete(output) => return Ok(TaskRunState::Completed(output)),
                }
            }

            if pause_requested {
                self.state = Some(state);
                return Ok(TaskRunState::Paused);
            }

            if state.runnable_branches.is_empty() {
                break;
            }
        }

        if !state.paused_branches.is_empty() {
            self.state = Some(state);
            return Ok(TaskRunState::Paused);
        }

        Err(TaskError::Incomplete)
    }

    fn branch_future(
        nodes: &[Arc<dyn AnyNodeExecutor>],
        branch: ExecutionBranch,
    ) -> Result<RunningBranchFuture, TaskError> {
        let node_executor = nodes
            .get(branch.current_node)
            .ok_or_else(|| TaskError::missing_node(branch.current_node))?
            .clone();

        Ok(Box::pin(async move {
            let span = tracing::info_span!(
                "task.step",
                node = branch.current_node,
                branch = branch.id.0
            );

            let next_step = node_executor
                .evaluate_next(branch.context.clone())
                .instrument(span)
                .await;

            tracing::info!(
                node = branch.current_node,
                branch = branch.id.0,
                "task.step.done"
            );

            next_step.map(|next_step| EvaluatedBranch { branch, next_step })
        }))
    }
}

impl RunState {
    fn new(
        start_node: usize,
        context: Arc<dyn Any + Send + Sync>,
        default_concurrency_model: ConcurrencyModel,
    ) -> Self {
        let mut state = Self {
            runnable_branches: VecDeque::new(),
            paused_branches: HashMap::new(),
            join_groups: HashMap::new(),
            next_branch_id: 1,
            next_group_id: 1,
        };
        let branch_id = state.next_branch();
        state.enqueue_branch(ExecutionBranch {
            id: branch_id,
            current_node: start_node,
            context,
            concurrency_model: default_concurrency_model,
            join_group: None,
        });
        state
    }

    fn has_resumable_work(&self) -> bool {
        !self.runnable_branches.is_empty() || !self.paused_branches.is_empty()
    }

    fn current_node(&self) -> Option<usize> {
        self.paused_branches
            .values()
            .next()
            .map(|branch| branch.current_node)
            .or_else(|| {
                self.runnable_branches
                    .front()
                    .map(|branch| branch.current_node)
            })
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

    fn enqueue_branch(&mut self, branch: ExecutionBranch) {
        self.runnable_branches.push_back(branch);
    }

    fn restore_paused_branches(&mut self) {
        let mut paused = self
            .paused_branches
            .drain()
            .map(|(_, branch)| branch)
            .collect::<Vec<_>>();
        paused.sort_by_key(|branch| branch.id.0);

        for branch in paused {
            self.set_join_member_state(branch.join_group, branch.id, JoinMemberState::Pending);
            self.enqueue_branch(branch);
        }
    }

    fn apply_branch_result<Output: NodeArg + Clone>(
        &mut self,
        evaluated: EvaluatedBranch,
        default_concurrency_model: ConcurrencyModel,
    ) -> Result<LoopControl<Output>, TaskError> {
        let EvaluatedBranch { branch, next_step } = evaluated;
        match next_step {
            EvaluatedTransition::Flow(transition) => {
                self.apply_flow_transition(branch, transition, default_concurrency_model)
            }
            EvaluatedTransition::Join {
                definition,
                payload,
            } => self.apply_join_payload(&branch, definition, payload),
        }
    }

    fn apply_flow_transition<Output: NodeArg + Clone>(
        &mut self,
        mut branch: ExecutionBranch,
        transition: Transition,
        default_concurrency_model: ConcurrencyModel,
    ) -> Result<LoopControl<Output>, TaskError> {
        let concurrency_model = transition
            .concurrency_model
            .unwrap_or(branch.concurrency_model);

        match transition.action {
            TransitionAction::Next(next_node) => {
                branch.current_node = next_node.node_id;
                branch.context = next_node.context;
                branch.concurrency_model = concurrency_model;
                self.set_join_member_state(branch.join_group, branch.id, JoinMemberState::Pending);
                self.enqueue_branch(branch);
                Ok(LoopControl::Continue)
            }
            TransitionAction::FanOut { targets, join } => {
                if branch.join_group.is_some() {
                    return Err(TaskError::invalid_state(format!(
                        "Node {} cannot fan out while it belongs to an active join group",
                        branch.current_node
                    )));
                }

                self.enqueue_fan_out_branches(
                    targets,
                    concurrency_model,
                    join,
                    default_concurrency_model,
                );
                Ok(LoopControl::Continue)
            }
            TransitionAction::Pause => Ok(self.pause_branch(branch)),
            TransitionAction::Error(error) => Err(TaskError::NodeError(NodeError::new(
                error,
                branch.current_node,
                None,
            ))),
            TransitionAction::Finish(output) => self.apply_finish_transition(&branch, output),
        }
    }

    fn enqueue_fan_out_branches(
        &mut self,
        targets: Vec<NextNode>,
        concurrency_model: ConcurrencyModel,
        join: JoinDefinition,
        default_concurrency_model: ConcurrencyModel,
    ) {
        if targets.is_empty() {
            return;
        }

        let join_group = self.next_group();
        let mut members = HashMap::with_capacity(targets.len());
        let mut member_order = Vec::with_capacity(targets.len());

        for target in targets {
            let child_id = self.next_branch();
            let child = ExecutionBranch {
                id: child_id,
                current_node: target.node_id,
                context: target.context,
                concurrency_model,
                join_group: Some(join_group),
            };

            member_order.push(child_id);
            members.insert(child_id, JoinMemberState::Pending);
            self.enqueue_branch(child);
        }

        self.join_groups.insert(
            join_group,
            JoinGroupState {
                join_node_id: join.join_node_id,
                concurrency_model: join.concurrency_model.unwrap_or(default_concurrency_model),
                members,
                member_order,
            },
        );
    }

    fn pause_branch<Output>(&mut self, branch: ExecutionBranch) -> LoopControl<Output> {
        self.paused_branches.insert(branch.id, branch);
        LoopControl::PauseRequested
    }

    fn finish_with_output<Output: NodeArg + Clone>(
        &mut self,
        output: Arc<dyn Any + Send + Sync>,
    ) -> Result<LoopControl<Output>, TaskError> {
        let output = output
            .downcast::<Output>()
            .map_err(|error| TaskError::type_error(&error))?
            .as_ref()
            .clone();
        Ok(LoopControl::Complete(output))
    }

    fn apply_finish_transition<Output: NodeArg + Clone>(
        &mut self,
        branch: &ExecutionBranch,
        output: Arc<dyn Any + Send + Sync>,
    ) -> Result<LoopControl<Output>, TaskError> {
        let Some(group_id) = branch.join_group else {
            return self.finish_with_output(output);
        };

        self.apply_join_arrival(group_id, branch, output)
    }

    fn apply_join_payload<Output>(
        &mut self,
        branch: &ExecutionBranch,
        definition: JoinDefinition,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<LoopControl<Output>, TaskError> {
        let Some(group_id) = branch.join_group else {
            return Err(TaskError::invalid_state(format!(
                "Node {} used join without an attached join group",
                branch.current_node
            )));
        };

        {
            let group = self
                .join_groups
                .get_mut(&group_id)
                .ok_or_else(|| TaskError::invalid_state("Missing join group"))?;

            if group.join_node_id != definition.join_node_id {
                return Err(TaskError::invalid_state(format!(
                    "Node {} used join for an unexpected join target",
                    branch.current_node
                )));
            }
        }

        self.apply_join_arrival(group_id, branch, payload)
    }

    fn apply_join_arrival<Output>(
        &mut self,
        group_id: BranchGroupId,
        branch: &ExecutionBranch,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<LoopControl<Output>, TaskError> {
        {
            let group = self
                .join_groups
                .get_mut(&group_id)
                .ok_or_else(|| TaskError::invalid_state("Missing join group"))?;

            group
                .members
                .insert(branch.id, JoinMemberState::Ready { payload });
        }

        self.enqueue_join_if_ready(group_id)?;
        Ok(LoopControl::Continue)
    }

    fn enqueue_join_if_ready(&mut self, group_id: BranchGroupId) -> Result<(), TaskError> {
        if let Some(join_branch) = self.try_fire_join(group_id)? {
            self.enqueue_branch(join_branch);
        }

        Ok(())
    }

    fn try_fire_join(
        &mut self,
        group_id: BranchGroupId,
    ) -> Result<Option<ExecutionBranch>, TaskError> {
        let ready = {
            let Some(group) = self.join_groups.get(&group_id) else {
                return Ok(None);
            };

            group.members.values().all(JoinMemberState::is_terminal)
        };

        if !ready {
            return Ok(None);
        }

        let mut group = self
            .join_groups
            .remove(&group_id)
            .ok_or_else(|| TaskError::invalid_state("Missing join group"))?;
        let branches = group
            .member_order
            .into_iter()
            .filter_map(|branch_id| {
                group
                    .members
                    .remove(&branch_id)
                    .and_then(|state| match state {
                        JoinMemberState::Ready { payload } => Some(payload),
                        JoinMemberState::Pending => None,
                    })
            })
            .collect();
        let join_input = JoinInput::new(branches);

        Ok(Some(ExecutionBranch {
            id: self.next_branch(),
            current_node: group.join_node_id,
            context: Arc::new(join_input) as Arc<dyn Any + Send + Sync>,
            concurrency_model: group.concurrency_model,
            join_group: None,
        }))
    }

    fn set_join_member_state(
        &mut self,
        group_id: Option<BranchGroupId>,
        branch_id: BranchId,
        state: JoinMemberState,
    ) {
        if let Some(group_id) = group_id
            && let Some(group) = self.join_groups.get_mut(&group_id)
        {
            group.members.insert(branch_id, state);
        }
    }
}
