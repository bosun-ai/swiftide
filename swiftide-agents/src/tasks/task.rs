//! Tasks enable you to to define a graph of interacting nodes
//!
//! The nodes can be any type that implements the `TaskNode` trait, which defines how the node
//! will be evaluated with its input and output.
//!
//! Most swiftide primitives implement `TaskNode`, and it's easy to implement your own. Since how
//! agents interact is subject to taste, we recommend implementing your own.
//!
//! WARN: Here be dragons! This api is not stable yet. We are using it in production, and is
//! subject to rapid change. However, do not hesitate to open an issue if you find anything.
use std::{any::Any, pin::Pin, sync::Arc};

use tracing::Instrument as _;

use crate::tasks::{errors::NodeError, transition::TransitionFn};

use super::{
    errors::TaskError,
    node::{NodeArg, NodeId, NoopNode, TaskNode},
    transition::{AnyNodeTransition, MarkedTransitionPayload, Transition, TransitionPayload},
};

#[derive(Debug)]
pub struct Task<Input: NodeArg, Output: NodeArg> {
    nodes: Vec<Box<dyn AnyNodeTransition>>,
    current_node: usize,
    start_node: usize,
    current_context: Option<Arc<dyn Any + Send + Sync>>,
    _marker: std::marker::PhantomData<(Input, Output)>,
}

impl<Input: NodeArg, Output: NodeArg> Clone for Task<Input, Output> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            current_node: 0,
            start_node: self.start_node,
            current_context: None,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> Default for Task<Input, Output> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Input: NodeArg + Clone, Output: NodeArg + Clone> Task<Input, Output> {
    pub fn new() -> Self {
        let noop = NoopNode::<Output>::default();

        let node_id = NodeId::new(0, &noop).as_dyn();

        let noop_executor = Box::new(Transition {
            node: Box::new(noop),
            node_id: Box::new(node_id),
            r#fn: Arc::new(|_output| {
                Box::pin(async { unreachable!("Done node should never be evaluated.") })
            }),
            is_set: false,
        });
        Self {
            nodes: vec![noop_executor],
            current_node: 0,
            start_node: 0,
            current_context: None,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn done(&self) -> NodeId<NoopNode<Output>> {
        NodeId::new(0, &NoopNode::default())
    }

    /// Creates a transition to the done node
    pub fn transitions_to_done(
        &self,
    ) -> impl Fn(Output) -> MarkedTransitionPayload<NoopNode<Output>> + Send + Sync + 'static {
        let done = self.done();
        move |context| done.transitions_with(context)
    }

    /// Defines the start node of the task
    pub fn starts_with<T: TaskNode<Input = Input> + Clone + 'static>(
        &mut self,
        node_id: NodeId<T>,
    ) {
        self.current_node = node_id.id;
        self.start_node = node_id.id;
    }

    /// Validates that all nodes have transitions set
    ///
    /// # Errors
    ///
    /// Errors if a node is missing a transition
    pub fn validate_transitions(&self) -> Result<(), TaskError> {
        // TODO: Validate that the task can complete
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

    /// Runs the task with the given input
    ///
    /// # Errors
    ///
    /// Errors if the task fails
    #[tracing::instrument(skip(self, input), name = "task.run")]
    pub async fn run(&mut self, input: impl Into<Input>) -> Result<Option<Output>, TaskError> {
        self.validate_transitions()?;

        self.current_context = Some(Arc::new(input.into()) as Arc<dyn Any + Send + Sync>);

        self.resume().await
    }

    /// Resets the task to the start node
    ///
    /// WARN: This **will** lead to a type mismatch if the previous context is not the same as the
    /// input of the start node
    pub fn reset(&mut self) {
        self.current_node = self.start_node;
    }

    /// Resumes the task from the current node
    ///
    /// # Errors
    ///
    /// Errors if the task fails
    pub async fn resume(&mut self) -> Result<Option<Output>, TaskError> {
        self.validate_transitions()?;

        let mut span = tracing::info_span!("task.step", node = self.current_node);
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
                .clone()
                .ok_or_else(|| TaskError::missing_input(self.current_node))?;

            tracing::debug!("Running node {}", self.current_node);

            let span_id = span.id().clone();
            let transition_payload = node_transition
                .evaluate_next(input)
                .instrument(span.or_current())
                .await?;

            match transition_payload {
                TransitionPayload::Pause => {
                    tracing::info!("Task paused at node {}", self.current_node);
                    return Ok(None);
                }
                TransitionPayload::NextNode(transition_payload) => {
                    self.current_node = transition_payload.node_id;
                    self.current_context = Some(transition_payload.context);
                }
                TransitionPayload::Error(error) => {
                    return Err(TaskError::NodeError(NodeError::new(
                        error,
                        self.current_node,
                        None,
                    )));
                }
            }
            if self.current_node == 0 {
                tracing::debug!("Task completed at node {}", self.current_node);
                break;
            }

            span = tracing::info_span!("task.step", node = self.current_node).or_current();
            span.follows_from(span_id);
        }

        let output = self
            .current_context
            .clone()
            .ok_or_else(|| TaskError::missing_output(self.current_node))?;
        let output = output
            .downcast::<Output>()
            .map_err(|e| TaskError::type_error(&e))?
            .as_ref()
            .clone();

        Ok(Some(output))
    }

    /// Gets the current node of the task
    pub fn current_node<T: TaskNode + 'static>(&self) -> Option<&T> {
        self.node_at_index(self.current_node)
    }

    /// Gets the node at the given `NodeId`
    pub fn node_at<T: TaskNode + 'static>(&self, node_id: NodeId<T>) -> Option<&T> {
        self.node_at_index(node_id.id)
    }

    /// Gets the node at the given index
    pub fn node_at_index<T: TaskNode + 'static>(&self, index: usize) -> Option<&T> {
        let transition = self.transition_at_index::<T>(index)?;

        let node = &*transition.node;

        (node as &dyn Any).downcast_ref::<T>()
    }

    /// Gets the current transition of the task
    #[allow(dead_code)]
    fn current_transition<T: TaskNode + 'static>(
        &self,
    ) -> Option<&Transition<T::Input, T::Output, T::Error>> {
        self.transition_at_index::<T>(self.current_node)
    }

    /// Gets the transition at the given `NodeId`
    fn transition_at_index<T: TaskNode + 'static>(
        &self,
        index: usize,
    ) -> Option<&Transition<T::Input, T::Output, T::Error>> {
        tracing::debug!("Getting transition at index {}", index);
        let transition = self.nodes.get(index)?;

        dbg!(&transition);

        (&**transition as &dyn Any).downcast_ref::<Transition<T::Input, T::Output, T::Error>>()
    }

    /// Registers a new node in the task
    pub fn register_node<T>(&mut self, node: T) -> NodeId<T>
    where
        T: TaskNode + 'static + Clone,
        <T as TaskNode>::Input: Clone,
        <T as TaskNode>::Output: Clone,
    {
        let id = self.nodes.len();
        let node_id = NodeId::new(id, &node);
        let node_executor = Box::new(Transition::<T::Input, T::Output, T::Error> {
            node_id: Box::new(node_id.as_dyn()),
            node: Box::new(node),
            r#fn: Arc::new(move |_output| unreachable!("No transition for node {}.", node_id.id)),
            is_set: false,
        });
        // Debug the type name
        tracing::debug!(node_id = ?node_id, type_name = std::any::type_name_of_val(&node_executor), "Registering node");

        self.nodes.push(node_executor);

        node_id
    }

    /// Registers a transition from one node to another
    ///
    /// Note that there are various helpers and conversions for the `MarkedTransitionPayload`
    ///
    /// # Errors
    ///
    /// Errors if the node does not exist
    pub fn register_transition<'a, From, To, F>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        To: TaskNode<Input = From::Output> + 'a + ?Sized,
        F: Fn(To::Input) -> MarkedTransitionPayload<To> + Send + Sync + 'static,
    {
        let node_executor = self
            .nodes
            .get_mut(from.id)
            .ok_or_else(|| TaskError::missing_node(from.id))?;

        let any_executor: &mut dyn Any = node_executor.as_mut();

        let Some(exec) =
            any_executor.downcast_mut::<Transition<From::Input, From::Output, From::Error>>()
        else {
            let expected =
                std::any::type_name::<Transition<From::Input, From::Output, From::Error>>();
            let actual = std::any::type_name_of_val(node_executor);

            unreachable!(
                "Transition at index {:?} is not a {expected:?}; Mismatched types, should not never happen. Actual: {actual:?}",
                from.id
            );
        };
        let transition = Arc::new(transition);
        let wrapped: Arc<dyn TransitionFn<From::Output>> = Arc::new(move |output: From::Output| {
            let transition = transition.clone();
            Box::pin(async move {
                let output = transition(output);
                output.into_inner()
            })
        });

        exec.r#fn = wrapped;
        exec.is_set = true;
        // set function as before

        Ok(())
    }

    /// Registers a transition from one node to another asynchronously
    ///
    /// Note that there are various helpers and conversions for the `MarkedTransitionPayload`
    ///
    /// # Errors
    ///
    /// Errors if the node does not exist
    ///
    /// NOTE: `AsyncFn` traits' returned future are not 'Send' and the inner type is unstable.
    /// When they are, we can update Fn to `AsyncFn`
    pub fn register_transition_async<'a, From, To, F>(
        &mut self,
        from: NodeId<From>,
        transition: F,
    ) -> Result<(), TaskError>
    where
        From: TaskNode + 'static + ?Sized,
        To: TaskNode<Input = From::Output> + 'a + ?Sized,
        F: Fn(To::Input) -> Pin<Box<dyn Future<Output = MarkedTransitionPayload<To>> + Send>>
            + Send
            + Sync
            + 'static,
    {
        let node_executor = self
            .nodes
            .get_mut(from.id)
            .ok_or_else(|| TaskError::missing_node(from.id))?;

        let any_executor: &mut dyn Any = node_executor.as_mut();

        let Some(exec) =
            any_executor.downcast_mut::<Transition<From::Input, From::Output, From::Error>>()
        else {
            let expected =
                std::any::type_name::<Transition<From::Input, From::Output, From::Error>>();
            let actual = std::any::type_name_of_val(node_executor);

            unreachable!(
                "Transition at index {:?} is not a {expected:?}; Mismatched types, should not never happen. Actual: {actual:?}",
                from.id
            );
        };
        let transition = Arc::new(transition);
        let wrapped: Arc<dyn TransitionFn<From::Output>> = Arc::new(move |output: From::Output| {
            let transition = transition.clone();

            Box::pin(async move {
                let output = transition(output).await;
                output.into_inner()
            })
        });

        exec.r#fn = wrapped;
        exec.is_set = true;
        // set function as before

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

    #[derive(thiserror::Error, Debug)]
    struct Error(String);

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[derive(Clone, Default, Debug)]
    struct IntNode;
    #[async_trait]
    impl TaskNode for IntNode {
        type Input = i32;
        type Output = i32;
        type Error = Error;

        async fn evaluate(
            &self,
            _node_id: &NodeId<
                dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
            >,
            input: &Self::Input,
        ) -> Result<Self::Output, Self::Error> {
            Ok(input + 1)
        }
    }
    // Implement other required traits if necessary...

    #[test_log::test(tokio::test)]
    async fn sequential_3_node_task_reset_works() {
        let mut task: Task<i32, i32> = Task::new();

        // Register three nodes
        let node1 = task.register_node(IntNode);
        let node2 = task.register_node(IntNode);
        let node3 = task.register_node(IntNode);

        // Set start node
        task.starts_with(node1);

        // Register transitions (node1 → node2 → node3 → done)
        task.register_transition::<_, _, _>(node1, move |input| node2.transitions_with(input))
            .unwrap();
        task.register_transition::<_, _, _>(node2, move |input| node3.transitions_with(input))
            .unwrap();
        task.register_transition::<_, _, _>(node3, task.transitions_to_done())
            .unwrap();

        // Run the task to completion
        let res = task.run(1).await.unwrap();
        assert_eq!(res, Some(4)); // 1 + 1 + 1 + 1

        // Reset the task
        task.reset();

        // Assert current_node returns the correct node (node1)
        dbg!(&task);
        let n1_transition = task.transition_at_index::<IntNode>(1);

        assert!(n1_transition.is_some());

        let n1_transition = task.current_transition::<IntNode>();
        assert!(n1_transition.is_some());

        let n1_ref = task.current_node::<IntNode>();
        assert!(n1_ref.is_some());
    }
}
