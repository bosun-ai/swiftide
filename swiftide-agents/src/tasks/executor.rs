use std::{any::Any, future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;

use super::{
    errors::{NodeError, TaskError},
    node::NodeId,
    traits::{AnyNodeExecutor, NodeArg, TaskNode},
    transition::{JoinDefinition, Transition},
};

pub(crate) type BoxedTransitionFuture = Pin<Box<dyn Future<Output = Transition> + Send>>;
pub(crate) type TransitionHandler<Output> =
    Arc<dyn Fn(Output) -> BoxedTransitionFuture + Send + Sync + 'static>;
pub(crate) type BoxedJoinFuture = Pin<Box<dyn Future<Output = Arc<dyn Any + Send + Sync>> + Send>>;
pub(crate) type JoinHandler<Output> =
    Arc<dyn Fn(Output) -> BoxedJoinFuture + Send + Sync + 'static>;

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

#[derive(Debug)]
pub(crate) enum EvaluatedTransition {
    Flow(Transition),
    Join {
        definition: JoinDefinition,
        payload: Arc<dyn Any + Send + Sync>,
    },
}

pub(crate) struct NodeExecutor<
    Input: NodeArg,
    Output: NodeArg,
    Error: std::error::Error + Send + Sync + 'static,
> {
    pub(crate) node: Box<dyn TaskNode<Input = Input, Output = Output, Error = Error> + Send + Sync>,
    pub(crate) node_id: NodeId<dyn TaskNode<Input = Input, Output = Output, Error = Error>>,
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
            node_id: node_id.as_dyn(),
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
            node_id: self.node_id,
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

        match self.node.evaluate(&self.node_id, &context).await {
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
