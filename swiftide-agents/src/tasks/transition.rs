use std::{any::Any, pin::Pin, sync::Arc};

use async_trait::async_trait;
use dyn_clone::DynClone;

use super::{
    errors::NodeError,
    node::{NodeArg, NodeId, TaskNode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchGroupId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchedulerKind {
    #[default]
    Fifo,
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunLoopBehavior {
    #[default]
    DrainRunnable,
    PauseOnBranchPause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BranchPauseBehavior {
    #[default]
    Local,
    PauseTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BranchErrorBehavior {
    #[default]
    Local,
    FailTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinPolicy {
    All,
    AtLeast(usize),
    First(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinLeftoverBehavior {
    CancelRemaining,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransitionOptions {
    pub scheduler: Option<SchedulerKind>,
    pub pause_behavior: Option<BranchPauseBehavior>,
    pub error_behavior: Option<BranchErrorBehavior>,
}

#[derive(Debug, Clone)]
pub struct TransitionPolicies {
    pub scheduler: SchedulerKind,
    pub pause_behavior: BranchPauseBehavior,
    pub error_behavior: BranchErrorBehavior,
}

impl TransitionPolicies {
    pub fn with_overrides(&self, overrides: TransitionOptions) -> Self {
        Self {
            scheduler: overrides.scheduler.unwrap_or(self.scheduler),
            pause_behavior: overrides.pause_behavior.unwrap_or(self.pause_behavior),
            error_behavior: overrides.error_behavior.unwrap_or(self.error_behavior),
        }
    }
}

pub trait TransitionFn<Input: Send + Sync>:
    for<'a> Fn(Input) -> Pin<Box<dyn Future<Output = TransitionDirective> + Send>> + Send + Sync
{
}

impl<Input: Send + Sync, F> TransitionFn<Input> for F where
    F: for<'a> Fn(Input) -> Pin<Box<dyn Future<Output = TransitionDirective> + Send>> + Send + Sync
{
}

pub(crate) struct Transition<
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
> {
    pub(crate) node: Box<dyn TaskNode<Input = Input, Output = Output, Error = Error> + Send + Sync>,
    pub(crate) node_id: Box<NodeId<dyn TaskNode<Input = Input, Output = Output, Error = Error>>>,
    pub(crate) r#fn: Arc<dyn TransitionFn<Output> + Send>,
    pub(crate) is_set: bool,
}

impl<Input, Output, Error> Clone for Transition<Input, Output, Error>
where
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Transition {
            node: self.node.clone(),
            node_id: self.node_id.clone(),
            r#fn: self.r#fn.clone(),
            is_set: self.is_set,
        }
    }
}

impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    std::fmt::Debug for Transition<Input, Output, Error>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transition")
            .field("node_id", &self.node_id.id)
            .field("is_set", &self.is_set)
            .finish()
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
        let context = Arc::new(context) as Arc<dyn Any + Send + Sync>;

        NextNode {
            node_id: node_id.id,
            context,
        }
    }
}

impl From<NextNode> for TransitionPayload {
    fn from(next_node: NextNode) -> Self {
        TransitionPayload::NextNode(next_node)
    }
}

#[derive(Debug, Clone)]
pub struct JoinConfig {
    pub(crate) join_node_id: usize,
    pub(crate) policy: JoinPolicy,
    pub(crate) leftover_behavior: Option<JoinLeftoverBehavior>,
}

impl JoinConfig {
    pub fn new<T: TaskNode + ?Sized>(join_node_id: NodeId<T>, policy: JoinPolicy) -> Self {
        Self {
            join_node_id: join_node_id.id,
            policy,
            leftover_behavior: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FanOutDirective {
    pub(crate) targets: Vec<NextNode>,
    pub(crate) join: Option<JoinConfig>,
}

#[derive(Debug)]
pub enum TransitionAction {
    Next(NextNode),
    FanOut(FanOutDirective),
    Join(Arc<dyn Any + Send + Sync>),
    Pause,
    Error(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug)]
pub struct TransitionDirective {
    pub(crate) action: TransitionAction,
    pub(crate) options: TransitionOptions,
}

impl TransitionDirective {
    pub fn next(next_node: NextNode) -> Self {
        Self {
            action: TransitionAction::Next(next_node),
            options: TransitionOptions::default(),
        }
    }

    pub fn fan_out(targets: impl IntoIterator<Item = NextNode>) -> Self {
        Self {
            action: TransitionAction::FanOut(FanOutDirective {
                targets: targets.into_iter().collect(),
                join: None,
            }),
            options: TransitionOptions::default(),
        }
    }

    pub fn join<T: NodeArg>(context: T) -> Self {
        Self {
            action: TransitionAction::Join(Arc::new(context) as Arc<dyn Any + Send + Sync>),
            options: TransitionOptions::default(),
        }
    }

    pub fn pause() -> Self {
        Self {
            action: TransitionAction::Pause,
            options: TransitionOptions::default(),
        }
    }

    pub fn error(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            action: TransitionAction::Error(error.into()),
            options: TransitionOptions::default(),
        }
    }

    pub fn with_options(mut self, options: TransitionOptions) -> Self {
        self.options = options;
        self
    }

    pub fn scheduler(mut self, scheduler: SchedulerKind) -> Self {
        self.options.scheduler = Some(scheduler);
        self
    }

    pub fn pause_behavior(mut self, pause_behavior: BranchPauseBehavior) -> Self {
        self.options.pause_behavior = Some(pause_behavior);
        self
    }

    pub fn error_behavior(mut self, error_behavior: BranchErrorBehavior) -> Self {
        self.options.error_behavior = Some(error_behavior);
        self
    }

    pub fn with_join<T: TaskNode + ?Sized>(
        mut self,
        join_node_id: NodeId<T>,
        policy: JoinPolicy,
    ) -> Self {
        if let TransitionAction::FanOut(fan_out) = &mut self.action {
            fan_out.join = Some(JoinConfig::new(join_node_id, policy));
        }

        self
    }

    pub fn leftover_behavior(mut self, leftover_behavior: JoinLeftoverBehavior) -> Self {
        if let TransitionAction::FanOut(fan_out) = &mut self.action {
            if let Some(join) = &mut fan_out.join {
                join.leftover_behavior = Some(leftover_behavior);
            }
        }

        self
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
        TransitionPayload::Pause
    }

    pub fn error(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        TransitionPayload::Error(error.into())
    }
}

pub struct MarkedTransitionPayload<To: TaskNode + ?Sized>(
    TransitionPayload,
    std::marker::PhantomData<To>,
);

impl<To: TaskNode + ?Sized> MarkedTransitionPayload<To> {
    pub fn new(payload: TransitionPayload) -> Self {
        MarkedTransitionPayload(payload, std::marker::PhantomData)
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

impl From<TransitionPayload> for TransitionDirective {
    fn from(value: TransitionPayload) -> Self {
        match value {
            TransitionPayload::NextNode(next_node) => TransitionDirective::next(next_node),
            TransitionPayload::Pause => TransitionDirective::pause(),
            TransitionPayload::Error(error) => TransitionDirective::error(error),
        }
    }
}

impl<T: TaskNode> std::ops::Deref for MarkedTransitionPayload<T> {
    type Target = TransitionPayload;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct JoinInput {
    pub branches: Vec<BranchEnvelope>,
}

impl JoinInput {
    pub fn ready_values<T: NodeArg>(&self) -> Vec<&T> {
        self.branches
            .iter()
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveBranch {
    pub branch_id: BranchId,
    pub node_id: usize,
}

#[async_trait]
pub(crate) trait AnyNodeTransition: Any + Send + Sync + std::fmt::Debug + DynClone {
    fn transition_is_set(&self) -> bool;

    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<TransitionDirective, NodeError>;

    fn node_id(&self) -> usize;
}

dyn_clone::clone_trait_object!(AnyNodeTransition);

#[async_trait]
impl<Input: NodeArg, Output: NodeArg, Error: std::error::Error + Send + Sync + 'static>
    AnyNodeTransition for Transition<Input, Output, Error>
{
    async fn evaluate_next(
        &self,
        context: Arc<dyn Any + Send + Sync>,
    ) -> Result<TransitionDirective, NodeError> {
        let context = context.downcast::<Input>().unwrap();

        match self.node.evaluate(&self.node_id.as_dyn(), &context).await {
            Ok(output) => Ok((self.r#fn)(output).await),
            Err(error) => Err(NodeError::new(error, self.node_id.id, None)),
        }
    }

    fn transition_is_set(&self) -> bool {
        self.is_set
    }

    fn node_id(&self) -> usize {
        self.node_id.id
    }
}
