use std::any::Any;

use super::{
    errors::TaskError,
    node::{NodeArg, NodeId, NoopNode, TaskNode},
    transition::{AnyNodeTransition, Transition, TransitionPayload},
};

pub struct Task<Input: NodeArg, Output: NodeArg, E: std::error::Error + Send + Sync> {
    nodes: Vec<Box<dyn AnyNodeTransition>>,
    start_node: usize,
    _marker: std::marker::PhantomData<(Input, Output, E)>,
}

impl Default for Task<(), (), std::convert::Infallible> {
    fn default() -> Self {
        Task::new()
    }
}

impl<
    Input: NodeArg + 'static,
    Output: NodeArg + 'static,
    E: std::error::Error + Send + Sync + 'static,
> Task<Input, Output, E>
{
    pub fn new() -> Self {
        let noop = NoopNode::<Output, E>::default();

        let node_id = NodeId::new(0, &noop);

        let noop_executor = Box::new(Transition {
            node: Box::new(noop),
            node_id,
            transition_fn: Box::new(|_output| unreachable!("Done node should never be evaluated.")),
            transition_is_set: false,
        });
        Self {
            nodes: vec![noop_executor],
            start_node: 0,
            _marker: std::marker::PhantomData,
        }
    }

    // // unused
    // TODO: We can make the api nicer
    pub fn done_node_id(&self) -> NodeId<NoopNode<Output, E>> {
        NodeId::new(0, &NoopNode::<Output, E>::default())
    }

    // TODO: We can make the api nicer
    pub fn set_start_node<T: TaskNode<Input = Input> + 'static>(&mut self, node_id: &NodeId<T>) {
        self.start_node = node_id.id;
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

    pub async fn run(&self, input: Input) -> Result<Output, TaskError> {
        self.validate_transitions()?;

        let mut node_id = self.start_node;
        let mut input = Box::new(input) as Box<dyn Any + Send>;

        loop {
            if node_id == 0 {
                break;
            }
            let node_executor = self.nodes.get(node_id).expect("Node not found");
            let transition = node_executor.evaluate(input).await?;

            node_id = transition.node_id;
            input = transition.context;
        }

        Ok(*input.downcast::<Output>().unwrap())
    }

    pub fn register_node<T: TaskNode + 'static>(&mut self, node: T) -> NodeId<T> {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        let node_executor = Box::new(Transition::<T> {
            node_id: node_id.clone(),
            node: Box::new(node),
            transition_fn: Box::new(move |_output| {
                unreachable!("No transition for node {}.", node_id.id)
            }),
            transition_is_set: false,
        });
        self.nodes.push(node_executor);

        node_id
    }

    pub fn register_transition<
        From: TaskNode + 'static,
        To: TaskNode<Input = From::Output> + 'static,
    >(
        &mut self,
        from: &NodeId<From>,
        transition: impl Fn(&From::Output) -> NodeId<To> + Send + Sync + 'static,
    ) {
        let node_executor = self
            .nodes
            .get_mut(from.id)
            .expect("Impossible transition for node not in workflow.");

        let any_executor: &mut dyn Any = node_executor.as_mut();
        //
        let node_executor = any_executor.downcast_mut::<Transition<From>>().unwrap();

        let wrapped = move |output: From::Output| {
            let next_node = transition(&output);

            TransitionPayload::new(&next_node, output)
        };

        node_executor.transition_fn = Box::new(wrapped);
        node_executor.transition_is_set = true;
    }
}
