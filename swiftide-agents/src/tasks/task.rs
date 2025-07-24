use std::any::Any;

use super::{
    errors::TaskError,
    node::{NodeArg, NodeId, NoopNode, TaskNode},
    transition::{AnyNodeTransition, MarkedTransitionPayload, Transition, TransitionPayload},
};

pub struct Task<Input: NodeArg, Output: NodeArg> {
    nodes: Vec<Box<dyn AnyNodeTransition>>,
    current_node: usize,
    current_context: Option<Box<dyn Any + Send + Sync>>,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<Input: NodeArg + 'static, Output: NodeArg + Clone + 'static> Default for Task<Input, Output> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Input: NodeArg + 'static, Output: NodeArg + Clone + 'static> Task<Input, Output> {
    pub fn new() -> Self {
        let noop = NoopNode::<Output>::default();

        let node_id = NodeId::new(0, &noop);

        let noop_executor = Box::new(Transition {
            node: noop,
            node_id,
            r#fn: Box::new(|_output| unreachable!("Done node should never be evaluated.")),
            is_set: false,
        });
        Self {
            nodes: vec![noop_executor],
            current_node: 0,
            current_context: None,
            _marker: std::marker::PhantomData,
        }
    }

    // // unused
    // TODO: We can make the api nicer
    pub fn done(&self) -> NodeId<NoopNode<()>> {
        NodeId::new(0, &NoopNode::<()>::default())
    }

    // TODO: We can make the api nicer, i.e. default to the first node added
    pub fn set_start_node<T: TaskNode<Input = Input> + Clone + 'static>(
        &mut self,
        node_id: NodeId<T>,
    ) {
        self.current_node = node_id.id;
    }

    fn validate_transitions(&self) -> Result<(), TaskError> {
        for node_executor in &self.nodes {
            // Skip the done node (index 0)
            if node_executor.node_id() == 0 {
                continue;
            }

            if !node_executor.transition_is_set() {
                return Err(TaskError::missing_transition(node_executor.node_id()));
            }
        }
        Ok(())
    }

    pub async fn run(&mut self, input: impl Into<Input>) -> Result<Option<Output>, TaskError> {
        self.validate_transitions()?;

        self.current_context = Some(Box::new(input.into()) as Box<dyn Any + Send + Sync>);

        self.resume().await
    }

    // TODO: Use type state to avoid calling this accidentally?
    pub async fn resume(&mut self) -> Result<Option<Output>, TaskError> {
        self.validate_transitions()?;

        loop {
            if self.current_node == 0 {
                break;
            }
            let node_transition = self
                .nodes
                .get(self.current_node)
                .ok_or_else(|| TaskError::missing_node(self.current_node))?;

            let input = self
                .current_context
                .take()
                .ok_or_else(|| TaskError::missing_input(self.current_node))?;

            let transition_payload = node_transition.evaluate_next(input).await?;

            match transition_payload {
                TransitionPayload::Pause => {
                    tracing::info!("Task paused at node {}", self.current_node);
                    return Ok(None);
                }
                TransitionPayload::NextNode(transition_payload) => {
                    self.current_node = transition_payload.node_id;
                    self.current_context = Some(transition_payload.context);
                }
            }
        }

        let output = self
            .current_context
            .take()
            .ok_or_else(|| TaskError::missing_output(self.current_node))?;
        let output = *output.downcast::<Output>().map_err(TaskError::type_error)?;

        Ok(Some(output))
    }

    pub fn current_node<T: TaskNode + 'static>(&self) -> Option<&T> {
        self.nodes
            .get(self.current_node)
            .and_then(|node| (node as &dyn Any).downcast_ref::<Transition<T>>())
            .map(|transition| &transition.node)
    }

    pub fn register_node<T: TaskNode + Clone + 'static>(&mut self, node: T) -> NodeId<T> {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        let node_executor = Box::new(Transition::<T> {
            node_id,
            node,
            r#fn: Box::new(move |_output| unreachable!("No transition for node {}.", node_id.id)),
            is_set: false,
        });
        self.nodes.push(node_executor);

        node_id
    }

    pub fn register_transition<
        From: TaskNode + Clone + 'static,
        To: TaskNode<Input = From::Output> + Clone + 'static,
    >(
        &mut self,
        from: NodeId<From>,
        transition: impl Fn(To::Input) -> MarkedTransitionPayload<To> + Send + Sync + 'static,
    ) -> Result<(), TaskError> {
        let node_executor = self
            .nodes
            .get_mut(from.id)
            .ok_or_else(|| TaskError::missing_node(from.id))?;

        let any_executor: &mut dyn Any = node_executor.as_mut();
        //
        let Some(node_executor) = any_executor.downcast_mut::<Transition<From>>() else {
            unreachable!(
                "Node executor at index {} is not a Transition<From>; Mismatched types, should not never happen.",
                from.id
            );
        };

        let wrapped = move |output: From::Output| {
            let payload = transition(output);
            payload.into_inner()
        };

        node_executor.r#fn = Box::new(wrapped);
        node_executor.is_set = true;

        Ok(())
    }
}
