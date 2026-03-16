use std::{any::Any, marker::PhantomData, sync::Arc};

use super::node::{NodeArg, NodeId, TaskNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveBranch {
    pub branch_id: BranchId,
    pub node_id: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ConcurrencyModel {
    #[default]
    Sequential,
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum PauseBehavior {
    #[default]
    DrainRunnable,
    PauseTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ErrorBehavior {
    #[default]
    Local,
    FailTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinPolicy {
    All,
    AtLeast {
        count: usize,
        leftovers: JoinLeftoverBehavior,
    },
}

impl JoinPolicy {
    pub(crate) fn leftover_behavior(self) -> Option<JoinLeftoverBehavior> {
        match self {
            JoinPolicy::All => None,
            JoinPolicy::AtLeast { leftovers, .. } => Some(leftovers),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinLeftoverBehavior {
    CancelRemaining,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum JoinScope {
    #[default]
    ExplicitBranches,
    AllFanOutBranches,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct TransitionSettings {
    pub(crate) concurrency_model: Option<ConcurrencyModel>,
    pub(crate) pause_behavior: Option<PauseBehavior>,
    pub(crate) error_behavior: Option<ErrorBehavior>,
}

#[derive(Debug, Clone)]
pub(crate) struct EffectiveTransitionSettings {
    pub(crate) concurrency_model: ConcurrencyModel,
    pub(crate) pause_behavior: PauseBehavior,
    pub(crate) error_behavior: ErrorBehavior,
}

impl EffectiveTransitionSettings {
    pub(crate) fn with_overrides(&self, overrides: TransitionSettings) -> Self {
        Self {
            concurrency_model: overrides
                .concurrency_model
                .unwrap_or(self.concurrency_model),
            pause_behavior: overrides.pause_behavior.unwrap_or(self.pause_behavior),
            error_behavior: overrides.error_behavior.unwrap_or(self.error_behavior),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NextNode {
    pub(crate) node_id: usize,
    pub(crate) context: Arc<dyn Any + Send + Sync>,
}

impl NextNode {
    pub fn new<T: TaskNode + ?Sized>(node_id: NodeId<T>, context: T::Input) -> Self
    where
        <T as TaskNode>::Input: 'static,
    {
        Self {
            node_id: node_id.id(),
            context: Arc::new(context) as Arc<dyn Any + Send + Sync>,
        }
    }
}

impl From<NextNode> for Transition {
    fn from(next_node: NextNode) -> Self {
        Transition::next(next_node)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct JoinDefinition {
    pub(crate) join_node_id: usize,
    pub(crate) policy: JoinPolicy,
    pub(crate) scope: JoinScope,
    pub(crate) settings: TransitionSettings,
}

pub struct JoinTarget<T: TaskNode<Input = JoinInput> + ?Sized> {
    pub(crate) definition: JoinDefinition,
    _marker: PhantomData<T>,
}

pub struct AtLeastJoin<T: TaskNode<Input = JoinInput> + ?Sized> {
    node_id: NodeId<T>,
    count: usize,
}

pub struct MappedJoinTarget<T: TaskNode<Input = JoinInput> + ?Sized, F> {
    pub(crate) join_target: JoinTarget<T>,
    pub(crate) map: F,
}

pub struct AsyncMappedJoinTarget<T: TaskNode<Input = JoinInput> + ?Sized, F> {
    pub(crate) join_target: JoinTarget<T>,
    pub(crate) map: F,
}

impl<T: TaskNode<Input = JoinInput> + ?Sized> JoinTarget<T> {
    pub(crate) fn new(node_id: NodeId<T>, policy: JoinPolicy) -> Self {
        Self {
            definition: JoinDefinition {
                join_node_id: node_id.id(),
                policy,
                scope: JoinScope::ExplicitBranches,
                settings: TransitionSettings::default(),
            },
            _marker: PhantomData,
        }
    }

    pub(crate) fn into_definition(self) -> JoinDefinition {
        self.definition
    }

    pub fn all_fanout_branches(mut self) -> Self {
        self.definition.scope = JoinScope::AllFanOutBranches;
        self
    }

    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.definition.settings.concurrency_model = Some(concurrency_model);
        self
    }

    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.definition.settings.pause_behavior = Some(pause_behavior);
        self
    }

    pub fn error_behavior(mut self, error_behavior: ErrorBehavior) -> Self {
        self.definition.settings.error_behavior = Some(error_behavior);
        self
    }

    pub fn map<F>(self, map: F) -> MappedJoinTarget<T, F>
    where
        F: Send + Sync + 'static,
    {
        MappedJoinTarget {
            join_target: self,
            map,
        }
    }

    pub fn map_async<F>(self, map: F) -> AsyncMappedJoinTarget<T, F>
    where
        F: Send + Sync + 'static,
    {
        AsyncMappedJoinTarget {
            join_target: self,
            map,
        }
    }
}

impl<T: TaskNode<Input = JoinInput> + ?Sized> AtLeastJoin<T> {
    pub(crate) fn new(node_id: NodeId<T>, count: usize) -> Self {
        Self { node_id, count }
    }

    pub fn cancel_remaining(self) -> JoinTarget<T> {
        JoinTarget::new(
            self.node_id,
            JoinPolicy::AtLeast {
                count: self.count,
                leftovers: JoinLeftoverBehavior::CancelRemaining,
            },
        )
    }

    pub fn continue_remaining(self) -> JoinTarget<T> {
        JoinTarget::new(
            self.node_id,
            JoinPolicy::AtLeast {
                count: self.count,
                leftovers: JoinLeftoverBehavior::Continue,
            },
        )
    }
}

impl<T: TaskNode<Input = JoinInput> + ?Sized, F> MappedJoinTarget<T, F> {
    pub fn all_fanout_branches(mut self) -> Self {
        self.join_target = self.join_target.all_fanout_branches();
        self
    }

    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.join_target = self.join_target.concurrency_model(concurrency_model);
        self
    }

    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.join_target = self.join_target.pause_behavior(pause_behavior);
        self
    }

    pub fn error_behavior(mut self, error_behavior: ErrorBehavior) -> Self {
        self.join_target = self.join_target.error_behavior(error_behavior);
        self
    }
}

impl<T: TaskNode<Input = JoinInput> + ?Sized, F> AsyncMappedJoinTarget<T, F> {
    pub fn all_fanout_branches(mut self) -> Self {
        self.join_target = self.join_target.all_fanout_branches();
        self
    }

    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.join_target = self.join_target.concurrency_model(concurrency_model);
        self
    }

    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.join_target = self.join_target.pause_behavior(pause_behavior);
        self
    }

    pub fn error_behavior(mut self, error_behavior: ErrorBehavior) -> Self {
        self.join_target = self.join_target.error_behavior(error_behavior);
        self
    }
}

#[derive(Debug)]
pub struct Transition {
    pub(crate) action: TransitionAction,
    pub(crate) settings: TransitionSettings,
}

pub struct MarkedTransition<To: TaskNode + ?Sized>(Transition, std::marker::PhantomData<To>);

impl<To: TaskNode + ?Sized> MarkedTransition<To> {
    pub fn new(transition: Transition) -> Self {
        Self(transition, std::marker::PhantomData)
    }

    pub fn into_inner(self) -> Transition {
        self.0
    }
}

impl<T: TaskNode + ?Sized> From<MarkedTransition<T>> for Transition {
    fn from(value: MarkedTransition<T>) -> Self {
        value.into_inner()
    }
}

impl<T: TaskNode> std::ops::Deref for MarkedTransition<T> {
    type Target = Transition;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub(crate) enum TransitionAction {
    Next(NextNode),
    FanOut { targets: Vec<NextNode> },
    Pause,
    Error(Box<dyn std::error::Error + Send + Sync>),
    Finish(Arc<dyn Any + Send + Sync>),
}

impl Transition {
    pub fn next(next_node: NextNode) -> Self {
        Self {
            action: TransitionAction::Next(next_node),
            settings: TransitionSettings::default(),
        }
    }

    pub fn next_node<T: TaskNode + ?Sized>(node_id: &NodeId<T>, context: T::Input) -> Self {
        NextNode::new(*node_id, context).into()
    }

    pub fn fan_out(targets: impl IntoIterator<Item = NextNode>) -> Self {
        Self {
            action: TransitionAction::FanOut {
                targets: targets.into_iter().collect(),
            },
            settings: TransitionSettings::default(),
        }
    }

    pub fn pause() -> Self {
        Self {
            action: TransitionAction::Pause,
            settings: TransitionSettings::default(),
        }
    }

    pub fn error(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            action: TransitionAction::Error(error.into()),
            settings: TransitionSettings::default(),
        }
    }

    pub(crate) fn finish<T: NodeArg>(output: T) -> Self {
        Self {
            action: TransitionAction::Finish(Arc::new(output) as Arc<dyn Any + Send + Sync>),
            settings: TransitionSettings::default(),
        }
    }

    pub fn concurrency_model(mut self, concurrency_model: ConcurrencyModel) -> Self {
        self.settings.concurrency_model = Some(concurrency_model);
        self
    }

    pub fn pause_behavior(mut self, pause_behavior: PauseBehavior) -> Self {
        self.settings.pause_behavior = Some(pause_behavior);
        self
    }

    pub fn error_behavior(mut self, error_behavior: ErrorBehavior) -> Self {
        self.settings.error_behavior = Some(error_behavior);
        self
    }
}

#[derive(Debug, Clone)]
pub struct JoinInput {
    branches: Vec<BranchEnvelope>,
}

impl JoinInput {
    pub(crate) fn new(branches: Vec<BranchEnvelope>) -> Self {
        Self { branches }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, BranchEnvelope> {
        self.branches.iter()
    }

    pub fn ready_values<T: NodeArg>(&self) -> Vec<&T> {
        self.iter()
            .filter_map(BranchEnvelope::ready_value::<T>)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct BranchEnvelope {
    pub branch_id: BranchId,
    pub node_id: usize,
    pub outcome: BranchOutcome,
}

impl BranchEnvelope {
    pub fn ready_value<T: NodeArg>(&self) -> Option<&T> {
        self.outcome.ready_value()
    }
}

#[derive(Debug, Clone)]
pub enum BranchOutcome {
    Ready(Arc<dyn Any + Send + Sync>),
    Pending,
    Paused,
    Failed(String),
    Cancelled,
    LateArrival,
}

impl BranchOutcome {
    pub fn ready_value<T: NodeArg>(&self) -> Option<&T> {
        match self {
            BranchOutcome::Ready(value) => value.downcast_ref::<T>(),
            _ => None,
        }
    }
}
