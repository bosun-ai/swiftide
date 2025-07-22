use std::any::Any;

use async_trait::async_trait;

pub struct Task<Input: NodeArg, Output: NodeArg, E: std::error::Error + Send + Sync> {
    nodes: Vec<Box<dyn AnyNodeExecutor>>,
    start_node: usize,
    _marker: std::marker::PhantomData<(Input, Output, E)>,
}

pub trait NodeArg: Clone + Send + Sync + 'static {}
impl<T: Clone + Send + Sync + std::fmt::Debug + 'static> NodeArg for T {}

#[derive(Debug)]
pub struct NoopNode<Context: NodeArg, E: std::error::Error + Send + Sync> {
    _marker: std::marker::PhantomData<(Context, E)>,
}

#[async_trait]
impl<Context: NodeArg, E: std::error::Error + Send + Sync + 'static> TaskNode
    for NoopNode<Context, E>
{
    type Output = ();
    type Input = Context;
    type Error = E;

    async fn evaluate(&self, _context: &Context) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TaskError {
    #[error(transparent)]
    NodeError(#[from] NodeError),

    #[error("MissingTransition: {0}")]
    MissingTransition(String),
}

impl TaskError {
    pub fn missing_transition(node_id: usize) -> Self {
        TaskError::MissingTransition(format!("Node {node_id} is missing a transition"))
    }
}

#[derive(Debug, thiserror::Error)]
pub struct NodeError {
    pub node_error: Box<dyn std::error::Error + Send + Sync>,
    pub transition_payload: Option<TransitionPayload>,
    pub node_id: usize,
}

impl std::fmt::Display for NodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node error in node {}: {:?}",
            self.node_id, self.node_error
        )
    }
}

impl NodeError {
    pub fn new<E: std::error::Error + Send + Sync + 'static>(
        node_error: E,
        node_id: usize,
        transition_payload: Option<TransitionPayload>,
    ) -> Self {
        Self {
            node_error: Box::new(node_error),
            transition_payload,
            node_id,
        }
    }
}

#[async_trait]
pub trait TaskNode: Send + Sync {
    type Input: NodeArg;
    type Output: NodeArg;
    type Error: std::error::Error + Send + Sync + 'static;

    async fn evaluate(&self, input: &Self::Input) -> Result<Self::Output, Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct NodeId<T: TaskNode> {
    id: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T: TaskNode> Clone for NodeId<T> {
    fn clone(&self) -> Self {
        NodeId {
            id: self.id,
            _marker: std::marker::PhantomData,
        }
    }
}

struct NodeExecutor<T: TaskNode> {
    node: Box<dyn TaskNode<Input = T::Input, Output = T::Output, Error = T::Error>>,
    node_id: NodeId<T>,
    transition_fn: Box<dyn Fn(T::Output) -> TransitionPayload + Send + Sync>,
    transition_is_set: bool,
}

#[derive(Debug)]
pub struct TransitionPayload {
    // If we make this an enum instead, we can support spawning many nodes as well
    node_id: usize,
    context: Box<dyn Any + Send>,
}

impl TransitionPayload {
    fn new<T: TaskNode + 'static>(node_id: &NodeId<T>, context: T::Input) -> Self {
        TransitionPayload {
            node_id: node_id.id,
            context: Box::new(context),
        }
    }
}

#[async_trait]
pub trait AnyNodeExecutor: Any + Send + Sync {
    fn transition_is_set(&self) -> bool;

    async fn evaluate(&self, context: Box<dyn Any + Send>) -> Result<TransitionPayload, NodeError>;

    fn node_id(&self) -> usize;
}

// The implementation of AnyPassthroughNodeExecutor for PassthroughNodeExecutor
// enforces that the context is of type Input and guarantees that the output wrapped
// in the transition payload is of type Output.
#[async_trait]
impl<T: TaskNode + 'static> AnyNodeExecutor for NodeExecutor<T> {
    async fn evaluate(&self, context: Box<dyn Any + Send>) -> Result<TransitionPayload, NodeError> {
        let context = context.downcast::<T::Input>().unwrap();

        match self.node.evaluate(&context).await {
            Ok(output) => Ok((self.transition_fn)(output)),
            Err(error) => Err(NodeError::new(error, 0, None)), // node_id will be set by caller
        }
        // match self.evaluate(*context).await {
        //     Ok(payload) => Ok(payload),
        //     Err(workflow_error) => {
        //         Err(Box::new(workflow_error.node_error) as Box<dyn std::error::Error + Send + Sync>)
        //     }
        // }
    }

    fn transition_is_set(&self) -> bool {
        self.transition_is_set
    }

    fn node_id(&self) -> usize {
        self.node_id.id
    }
}

impl<
    Input: NodeArg + 'static,
    Output: NodeArg + 'static,
    E: std::error::Error + Send + Sync + 'static,
> Task<Input, Output, E>
{
    pub fn new() -> Self {
        let noop = NoopNode::<Output, E> {
            _marker: std::marker::PhantomData,
        };
        let node_id = NodeId {
            id: 0,
            _marker: std::marker::PhantomData::<NoopNode<Output, E>>,
        };
        let noop_executor = Box::new(NodeExecutor {
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

    // unused
    pub fn done_node_id(&self) -> NodeId<NoopNode<Output, E>> {
        NodeId {
            id: 0,
            _marker: std::marker::PhantomData,
        }
    }

    // unused
    pub fn set_start_node<T: TaskNode<Input = Input> + 'static>(&mut self, node_id: NodeId<T>) {
        self.start_node = node_id.id;
    }

    fn validate_transitions(&self) -> Result<(), TaskError> {
        for node_executor in self.nodes.iter() {
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
        let node_id = NodeId {
            id,
            _marker: std::marker::PhantomData::<T>,
        };
        let node_executor = Box::new(NodeExecutor::<T> {
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
        transition: Box<dyn Fn(&From::Output) -> NodeId<To> + Send + Sync>,
    ) {
        let node_executor = self
            .nodes
            .get_mut(from.id)
            .expect("Impossible transition for node not in workflow.");

        let any_executor: &mut dyn Any = node_executor.as_mut();
        //
        let node_executor = any_executor.downcast_mut::<NodeExecutor<From>>().unwrap();

        let wrapped = move |output: From::Output| {
            let next_node = transition(&output);

            TransitionPayload::new(&next_node, output)
        };

        node_executor.transition_fn = Box::new(wrapped);
        node_executor.transition_is_set = true;
    }
}
