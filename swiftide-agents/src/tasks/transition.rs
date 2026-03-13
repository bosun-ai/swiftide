use std::{any::Any, sync::Arc};

use super::node::{NodeArg, NodeId, TaskNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConcurrencyModel {
    #[default]
    Sequential,
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PauseBehavior {
    #[default]
    DrainRunnable,
    PauseTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorBehavior {
    #[default]
    Local,
    FailTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinLeftoverBehavior {
    CancelRemaining,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

impl From<NextNode> for TransitionPayload {
    fn from(next_node: NextNode) -> Self {
        TransitionPayload::NextNode(next_node)
    }
}

#[derive(Debug)]
pub enum TransitionPayload {
    NextNode(NextNode),
    Pause,
    Error(Box<dyn std::error::Error + Send + Sync>),
}

impl TransitionPayload {
    pub fn next_node<T: TaskNode + ?Sized>(node_id: &NodeId<T>, context: T::Input) -> Self {
        NextNode::new(*node_id, context).into()
    }

    pub fn pause() -> Self {
        Self::Pause
    }

    pub fn error(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Error(error.into())
    }
}

pub struct MarkedTransitionPayload<To: TaskNode + ?Sized>(
    TransitionPayload,
    std::marker::PhantomData<To>,
);

impl<To: TaskNode + ?Sized> MarkedTransitionPayload<To> {
    pub fn new(payload: TransitionPayload) -> Self {
        Self(payload, std::marker::PhantomData)
    }

    pub fn into_inner(self) -> TransitionPayload {
        self.0
    }
}

impl<T: TaskNode + ?Sized> From<MarkedTransitionPayload<T>> for TransitionDirective {
    fn from(value: MarkedTransitionPayload<T>) -> Self {
        value.into_inner().into()
    }
}

impl<T: TaskNode> std::ops::Deref for MarkedTransitionPayload<T> {
    type Target = TransitionPayload;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub(crate) enum TransitionAction {
    Next(NextNode),
    FanOut {
        targets: Vec<NextNode>,
        join: Option<(usize, JoinPolicy)>,
    },
    Join(Arc<dyn Any + Send + Sync>),
    Pause,
    Error(Box<dyn std::error::Error + Send + Sync>),
    Finish(Arc<dyn Any + Send + Sync>),
}

#[derive(Debug)]
pub struct TransitionDirective {
    pub(crate) action: TransitionAction,
    pub(crate) settings: TransitionSettings,
}

impl TransitionDirective {
    pub fn next(next_node: NextNode) -> Self {
        Self {
            action: TransitionAction::Next(next_node),
            settings: TransitionSettings::default(),
        }
    }

    pub fn fan_out(targets: impl IntoIterator<Item = NextNode>) -> Self {
        Self {
            action: TransitionAction::FanOut {
                targets: targets.into_iter().collect(),
                join: None,
            },
            settings: TransitionSettings::default(),
        }
    }

    pub fn fan_out_join<T: TaskNode + ?Sized>(
        targets: impl IntoIterator<Item = NextNode>,
        join_node_id: NodeId<T>,
        policy: JoinPolicy,
    ) -> Self {
        Self {
            action: TransitionAction::FanOut {
                targets: targets.into_iter().collect(),
                join: Some((join_node_id.id(), policy)),
            },
            settings: TransitionSettings::default(),
        }
    }

    pub fn join<T: NodeArg>(context: T) -> Self {
        Self {
            action: TransitionAction::Join(Arc::new(context) as Arc<dyn Any + Send + Sync>),
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

    pub fn finish<T: NodeArg>(output: T) -> Self {
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

impl From<TransitionPayload> for TransitionDirective {
    fn from(value: TransitionPayload) -> Self {
        match value {
            TransitionPayload::NextNode(next_node) => TransitionDirective::next(next_node),
            TransitionPayload::Pause => TransitionDirective::pause(),
            TransitionPayload::Error(error) => TransitionDirective::error(error),
        }
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
