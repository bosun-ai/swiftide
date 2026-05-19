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
        BranchId, ConcurrencyModel, JoinDefinition, NextNode, Transition, TransitionAction,
    },
};

type RunningBranchFuture = Pin<Box<dyn Future<Output = Result<EvaluatedBranch, TaskError>> + Send>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BranchGroupId(pub(crate) usize);

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

#[derive(Debug)]
struct JoinGroupState {
    definition: JoinDefinition,
    concurrency_model: ConcurrencyModel,
    first_branch_id: usize,
    payloads: Vec<Option<Arc<dyn Any + Send + Sync>>>,
}

impl JoinGroupState {
    fn new(
        definition: JoinDefinition,
        default_concurrency_model: ConcurrencyModel,
        first_branch_id: BranchId,
        branch_count: usize,
    ) -> Self {
        let concurrency_model = definition
            .concurrency_model
            .unwrap_or(default_concurrency_model);
        Self {
            definition,
            concurrency_model,
            first_branch_id: first_branch_id.0,
            payloads: vec![None; branch_count],
        }
    }

    fn set_payload(
        &mut self,
        branch_id: BranchId,
        payload: Arc<dyn Any + Send + Sync>,
    ) -> Result<(), TaskError> {
        let index = branch_id
            .0
            .checked_sub(self.first_branch_id)
            .ok_or_else(|| TaskError::invalid_state("Branch does not belong to join group"))?;
        let slot = self
            .payloads
            .get_mut(index)
            .ok_or_else(|| TaskError::invalid_state("Branch does not belong to join group"))?;
        if slot.is_some() {
            return Err(TaskError::invalid_state(
                "Branch already completed this join group",
            ));
        }

        *slot = Some(payload);
        Ok(())
    }

    fn is_ready(&self) -> bool {
        self.payloads.iter().all(Option::is_some)
    }

    fn into_join_input(self) -> Result<Arc<dyn Any + Send + Sync>, TaskError> {
        self.definition
            .into_input(self.payloads.into_iter().flatten().collect())
    }
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

        let branch_count = targets.len();
        let join_group = self.next_group();
        let first_branch_id = BranchId(self.next_branch_id);
        self.join_groups.insert(
            join_group,
            JoinGroupState::new(
                join,
                default_concurrency_model,
                first_branch_id,
                branch_count,
            ),
        );

        for target in targets {
            let child_id = self.next_branch();
            let child = ExecutionBranch {
                id: child_id,
                current_node: target.node_id,
                context: target.context,
                concurrency_model,
                join_group: Some(join_group),
            };

            self.enqueue_branch(child);
        }
    }

    fn pause_branch<Output>(&mut self, branch: ExecutionBranch) -> LoopControl<Output> {
        self.paused_branches.insert(branch.id, branch);
        LoopControl::PauseRequested
    }

    fn finish_with_output<Output: NodeArg + Clone>(
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
            return Self::finish_with_output(output);
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

            if group.definition.join_node_id != definition.join_node_id {
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

            group.set_payload(branch.id, payload)?;
        }

        if let Some(join_branch) = self.try_fire_join(group_id)? {
            self.enqueue_branch(join_branch);
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

            group.is_ready()
        };

        if !ready {
            return Ok(None);
        }

        let group = self
            .join_groups
            .remove(&group_id)
            .ok_or_else(|| TaskError::invalid_state("Missing join group"))?;
        let join_node_id = group.definition.join_node_id;
        let concurrency_model = group.concurrency_model;
        let join_input = group.into_join_input()?;

        Ok(Some(ExecutionBranch {
            id: self.next_branch(),
            current_node: join_node_id,
            context: join_input,
            concurrency_model,
            join_group: None,
        }))
    }
}
