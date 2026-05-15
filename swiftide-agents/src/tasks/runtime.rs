use std::{any::Any, collections::HashMap, future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use dyn_clone::DynClone;
use futures_util::{StreamExt as _, stream::FuturesUnordered};
use tracing::Instrument as _;

use super::{
    errors::{NodeError, TaskError},
    node::{NodeArg, NodeId, TaskNode},
    task::{Task, TaskRunState},
    transition::{
        BranchId, ConcurrencyModel, EffectiveTransitionSettings, JoinDefinition, JoinInput,
        Transition, TransitionAction,
    },
};

pub(crate) type BoxedTransitionFuture = Pin<Box<dyn Future<Output = Transition> + Send>>;
pub(crate) type TransitionHandler<Output> =
    Arc<dyn Fn(Output) -> BoxedTransitionFuture + Send + Sync + 'static>;
pub(crate) type BoxedJoinFuture = Pin<Box<dyn Future<Output = Arc<dyn Any + Send + Sync>> + Send>>;
pub(crate) type JoinHandler<Output> =
    Arc<dyn Fn(Output) -> BoxedJoinFuture + Send + Sync + 'static>;
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
    pub(crate) settings: EffectiveTransitionSettings,
    pub(crate) join_group: Option<BranchGroupId>,
}

pub(crate) enum RegisteredTransition<Output> {
    Missing,
    Flow(TransitionHandler<Output>),
    Join {
        definition: JoinDefinition,
        handler: JoinHandler<Output>,
    },
}

impl<Output> Clone for RegisteredTransition<Output> {
    fn clone(&self) -> Self {
        match self {
            RegisteredTransition::Missing => Self::Missing,
            RegisteredTransition::Flow(handler) => Self::Flow(handler.clone()),
            RegisteredTransition::Join {
                definition,
                handler,
            } => Self::Join {
                definition: *definition,
                handler: handler.clone(),
            },
        }
    }
}

impl<Output> std::fmt::Debug for RegisteredTransition<Output> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisteredTransition::Missing => f.write_str("RegisteredTransition::Missing"),
            RegisteredTransition::Flow(_) => f.write_str("RegisteredTransition::Flow(..)"),
            RegisteredTransition::Join { definition, .. } => f
                .debug_struct("RegisteredTransition::Join")
                .field("definition", definition)
                .finish_non_exhaustive(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum JoinMemberState {
    Pending,
    Paused,
    Ready { payload: Arc<dyn Any + Send + Sync> },
}

impl JoinMemberState {
    fn is_terminal(&self) -> bool {
        matches!(self, JoinMemberState::Ready { .. })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct JoinGroupState {
    pub(crate) join_node_id: usize,
    pub(crate) settings: EffectiveTransitionSettings,
    pub(crate) members: HashMap<BranchId, JoinMemberState>,
    pub(crate) member_order: Vec<BranchId>,
}

#[derive(Debug)]
struct EvaluatedBranch {
    branch: ExecutionBranch,
    next_step: EvaluatedTransition,
}

#[derive(Debug)]
pub(crate) enum EvaluatedTransition {
    Flow(Transition),
    Join {
        definition: JoinDefinition,
        payload: Arc<dyn Any + Send + Sync>,
    },
}

#[derive(Debug)]
enum LoopControl<Output> {
    Continue,
    PauseRequested,
    Complete(Output),
}

#[async_trait]
pub(crate) trait AnyNodeExecutor: Any + Send + Sync + std::fmt::Debug + DynClone {
    fn node_as_any(&self) -> &dyn Any;

    fn transition_is_set(&self) -> bool;

    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<EvaluatedTransition, TaskError>;
}

dyn_clone::clone_trait_object!(AnyNodeExecutor);

pub(crate) struct NodeExecutor<
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
> {
    pub(crate) node: Box<dyn TaskNode<Input = Input, Output = Output, Error = Error> + Send + Sync>,
    pub(crate) node_id: Box<NodeId<dyn TaskNode<Input = Input, Output = Output, Error = Error>>>,
    pub(crate) registration: RegisteredTransition<Output>,
}

impl<Input, Output, Error> NodeExecutor<Input, Output, Error>
where
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
{
    pub(crate) fn new<T>(node: T, node_id: NodeId<T>) -> Self
    where
        T: TaskNode<Input = Input, Output = Output, Error = Error> + Send + Sync + Clone + 'static,
    {
        Self {
            node: Box::new(node),
            node_id: Box::new(node_id.as_dyn()),
            registration: RegisteredTransition::Missing,
        }
    }

    pub(crate) fn set_transition_handler(
        &mut self,
        transition: TransitionHandler<Output>,
    ) -> Result<(), TaskError> {
        self.set_registration(RegisteredTransition::Flow(transition))
    }

    pub(crate) fn set_join_handler(
        &mut self,
        definition: JoinDefinition,
        transition: JoinHandler<Output>,
    ) -> Result<(), TaskError> {
        self.set_registration(RegisteredTransition::Join {
            definition,
            handler: transition,
        })
    }

    fn set_registration(
        &mut self,
        registration: RegisteredTransition<Output>,
    ) -> Result<(), TaskError> {
        if !matches!(self.registration, RegisteredTransition::Missing) {
            return Err(TaskError::invalid_state(format!(
                "Node {} already has a registered transition",
                self.node_id.id()
            )));
        }

        self.registration = registration;
        Ok(())
    }
}

impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    std::fmt::Debug for NodeExecutor<Input, Output, Error>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeExecutor")
            .field("node_id", &self.node_id.id())
            .field(
                "transition_is_set",
                &!matches!(self.registration, RegisteredTransition::Missing),
            )
            .finish()
    }
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
            registration: self.registration.clone(),
        }
    }
}

#[async_trait]
impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    AnyNodeExecutor for NodeExecutor<Input, Output, Error>
{
    fn node_as_any(&self) -> &dyn Any {
        self.node.as_ref()
    }

    fn transition_is_set(&self) -> bool {
        !matches!(self.registration, RegisteredTransition::Missing)
    }

    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<EvaluatedTransition, TaskError> {
        let context = context.downcast::<Input>().map_err(|_| {
            TaskError::invalid_state(format!(
                "Node {} expected input type {}",
                self.node_id.id(),
                std::any::type_name::<Input>()
            ))
        })?;

        match self.node.evaluate(&self.node_id.as_dyn(), &context).await {
            Ok(output) => match &self.registration {
                RegisteredTransition::Missing => Err(TaskError::invalid_state(format!(
                    "Node {} is missing a registered transition",
                    self.node_id.id()
                ))),
                RegisteredTransition::Flow(transition) => {
                    Ok(EvaluatedTransition::Flow((transition)(output).await))
                }
                RegisteredTransition::Join {
                    definition,
                    handler,
                } => Ok(EvaluatedTransition::Join {
                    definition: *definition,
                    payload: (handler)(output).await,
                }),
            },
            Err(error) => Err(TaskError::NodeError(NodeError::new(
                error,
                self.node_id.id(),
                None,
            ))),
        }
    }
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> Task<Input, Output> {
    pub(crate) fn has_live_state(&self) -> bool {
        !self.runnable_branches.is_empty()
            || !self.paused_branches.is_empty()
            || !self.join_groups.is_empty()
    }

    pub(crate) fn default_settings(&self) -> EffectiveTransitionSettings {
        EffectiveTransitionSettings {
            concurrency_model: self.options.concurrency_model,
        }
    }

    pub(crate) fn next_branch(&mut self) -> BranchId {
        let id = BranchId(self.next_branch_id);
        self.next_branch_id += 1;
        id
    }

    pub(crate) fn next_group(&mut self) -> BranchGroupId {
        let id = BranchGroupId(self.next_group_id);
        self.next_group_id += 1;
        id
    }

    pub(crate) fn enqueue_branch(&mut self, branch: ExecutionBranch) {
        self.runnable_branches.push_back(branch);
    }

    pub(crate) fn clear_runtime_state(&mut self) {
        self.runnable_branches.clear();
        self.paused_branches.clear();
        self.join_groups.clear();
    }

    pub(crate) fn restore_paused_branches(&mut self) {
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

    pub(crate) fn validate_transitions(&self) -> Result<usize, TaskError> {
        let start_node = self.start_node.ok_or(TaskError::NoSteps)?;

        for (index, node_executor) in self.nodes.iter().enumerate() {
            if !node_executor.transition_is_set() {
                return Err(TaskError::missing_transition(index));
            }
        }

        Ok(start_node)
    }

    pub(crate) async fn start_task(&mut self) -> Result<TaskRunState<Output>, TaskError> {
        let execution_nodes = self.execution_nodes();
        let mut in_flight = FuturesUnordered::<RunningBranchFuture>::new();
        let mut pause_requested = false;

        loop {
            if !pause_requested {
                while let Some(branch) = self.runnable_branches.pop_front() {
                    match branch.settings.concurrency_model {
                        ConcurrencyModel::Sequential => {
                            let execution_result =
                                match Self::branch_future(&execution_nodes, branch)?.await {
                                    Ok(result) => result,
                                    Err(error) => return self.fail_task(error),
                                };

                            match self.apply_branch_result(execution_result)? {
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
                            in_flight.push(Self::branch_future(&execution_nodes, branch)?);
                        }
                    }
                }
            }

            if let Some(result) = in_flight.next().await {
                let execution_result = match result {
                    Ok(result) => result,
                    Err(error) => return self.fail_task(error),
                };

                match self.apply_branch_result(execution_result)? {
                    LoopControl::Continue => continue,
                    LoopControl::PauseRequested => {
                        pause_requested = true;
                        continue;
                    }
                    LoopControl::Complete(output) => return Ok(TaskRunState::Completed(output)),
                }
            }

            if pause_requested {
                return Ok(TaskRunState::Paused);
            }

            if self.runnable_branches.is_empty() {
                break;
            }
        }

        if !self.paused_branches.is_empty() {
            return Ok(TaskRunState::Paused);
        }

        self.clear_runtime_state();
        Err(TaskError::Incomplete)
    }

    fn execution_nodes(&self) -> Vec<Arc<dyn AnyNodeExecutor>> {
        self.nodes
            .iter()
            .map(|node_executor| {
                Arc::<dyn AnyNodeExecutor>::from(dyn_clone::clone_box(&**node_executor))
            })
            .collect()
    }

    fn branch_future(
        execution_nodes: &[Arc<dyn AnyNodeExecutor>],
        branch: ExecutionBranch,
    ) -> Result<RunningBranchFuture, TaskError> {
        let node_executor = execution_nodes
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

    fn apply_branch_result(
        &mut self,
        evaluated: EvaluatedBranch,
    ) -> Result<LoopControl<Output>, TaskError> {
        let EvaluatedBranch { branch, next_step } = evaluated;
        match next_step {
            EvaluatedTransition::Flow(transition) => self.apply_flow_transition(branch, transition),
            EvaluatedTransition::Join {
                definition,
                payload,
            } => self.apply_join_payload(&branch, definition, payload),
        }
    }

    fn apply_flow_transition(
        &mut self,
        mut branch: ExecutionBranch,
        transition: Transition,
    ) -> Result<LoopControl<Output>, TaskError> {
        let settings = branch.settings.with_overrides(transition.settings);

        match transition.action {
            TransitionAction::Next(next_node) => {
                branch.current_node = next_node.node_id;
                branch.context = next_node.context;
                branch.settings = settings;
                self.set_join_member_state(branch.join_group, branch.id, JoinMemberState::Pending);
                self.enqueue_branch(branch);
                Ok(LoopControl::Continue)
            }
            TransitionAction::FanOut { targets, join } => {
                if branch.join_group.is_some() {
                    return self.fail_task(TaskError::invalid_state(format!(
                        "Node {} cannot fan out while it belongs to an active join group",
                        branch.current_node
                    )));
                }

                self.enqueue_fan_out_branches(targets, settings, join)?;
                Ok(LoopControl::Continue)
            }
            TransitionAction::Pause => Ok(self.pause_branch(branch)),
            TransitionAction::Error(error) => self.fail_task(TaskError::NodeError(NodeError::new(
                error,
                branch.current_node,
                None,
            ))),
            TransitionAction::Finish(output) => self.apply_finish_transition(&branch, output),
        }
    }

    fn enqueue_fan_out_branches(
        &mut self,
        targets: Vec<super::transition::NextNode>,
        settings: EffectiveTransitionSettings,
        join: JoinDefinition,
    ) -> Result<(), TaskError> {
        let join_group = if targets.is_empty() {
            None
        } else {
            Some(self.insert_join_group(join))
        };

        for target in targets {
            let child_id = self.next_branch();
            let child = ExecutionBranch {
                id: child_id,
                current_node: target.node_id,
                context: target.context,
                settings,
                join_group,
            };

            if let Some(group_id) = join_group {
                let group = self
                    .join_groups
                    .get_mut(&group_id)
                    .ok_or_else(|| TaskError::invalid_state("Missing join group"))?;
                group.member_order.push(child_id);
                group.members.insert(child_id, JoinMemberState::Pending);
            }

            self.enqueue_branch(child);
        }

        Ok(())
    }

    fn pause_branch(&mut self, branch: ExecutionBranch) -> LoopControl<Output> {
        self.set_join_member_state(branch.join_group, branch.id, JoinMemberState::Paused);
        self.paused_branches.insert(branch.id, branch);
        LoopControl::PauseRequested
    }

    fn finish_with_output(
        &mut self,
        output: Arc<dyn Any + Send + Sync>,
    ) -> Result<LoopControl<Output>, TaskError> {
        self.clear_runtime_state();
        let output = output
            .downcast::<Output>()
            .map_err(|error| TaskError::type_error(&error))?
            .as_ref()
            .clone();
        Ok(LoopControl::Complete(output))
    }

    fn apply_finish_transition(
        &mut self,
        branch: &ExecutionBranch,
        output: Arc<dyn Any + Send + Sync>,
    ) -> Result<LoopControl<Output>, TaskError> {
        let Some(group_id) = branch.join_group else {
            return self.finish_with_output(output);
        };

        self.apply_join_arrival(group_id, branch, output)
    }

    fn apply_join_payload(
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

    fn apply_join_arrival(
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

        self.enqueue_join_if_ready(Some(group_id))?;
        Ok(LoopControl::Continue)
    }

    fn insert_join_group(&mut self, definition: JoinDefinition) -> BranchGroupId {
        let group_id = self.next_group();
        self.join_groups.insert(
            group_id,
            JoinGroupState {
                join_node_id: definition.join_node_id,
                settings: self.default_settings().with_overrides(definition.settings),
                members: HashMap::new(),
                member_order: Vec::new(),
            },
        );
        group_id
    }

    fn enqueue_join_if_ready(&mut self, group_id: Option<BranchGroupId>) -> Result<(), TaskError> {
        if let Some(group_id) = group_id
            && let Some(join_branch) = self.try_fire_join(group_id)?
        {
            self.enqueue_branch(join_branch);
        }

        Ok(())
    }

    pub(crate) fn try_fire_join(
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

        let (join_node_id, settings) = {
            let group = self
                .join_groups
                .get(&group_id)
                .ok_or_else(|| TaskError::invalid_state("Missing join group"))?;
            (group.join_node_id, group.settings)
        };
        let join_input = self.build_join_input(group_id)?;
        self.join_groups.remove(&group_id);

        Ok(Some(ExecutionBranch {
            id: self.next_branch(),
            current_node: join_node_id,
            context: Arc::new(join_input) as Arc<dyn Any + Send + Sync>,
            settings,
            join_group: None,
        }))
    }

    fn build_join_input(&self, group_id: BranchGroupId) -> Result<JoinInput, TaskError> {
        let group = self
            .join_groups
            .get(&group_id)
            .ok_or_else(|| TaskError::invalid_state("Missing join group"))?;

        let branches = group
            .member_order
            .iter()
            .filter_map(|branch_id| {
                group.members.get(branch_id).and_then(|state| match state {
                    JoinMemberState::Ready { payload } => Some(payload.clone()),
                    JoinMemberState::Pending | JoinMemberState::Paused => None,
                })
            })
            .collect();

        Ok(JoinInput::new(branches))
    }

    fn fail_task<T>(&mut self, error: TaskError) -> Result<T, TaskError> {
        self.clear_runtime_state();
        Err(error)
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
