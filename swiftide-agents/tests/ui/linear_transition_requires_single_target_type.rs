use async_trait::async_trait;
use swiftide_agents::tasks::{NodeError, NodeId, Task, TaskNode};

#[derive(Clone, Debug)]
struct FirstNode;

#[derive(Clone, Debug)]
struct SecondNode;

#[derive(Clone, Debug)]
struct ThirdNode;

#[async_trait]
impl TaskNode for FirstNode {
    type Input = i32;
    type Output = String;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        _input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(String::new())
    }
}

#[async_trait]
impl TaskNode for SecondNode {
    type Input = String;
    type Output = usize;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        _input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(0)
    }
}

#[async_trait]
impl TaskNode for ThirdNode {
    type Input = String;
    type Output = usize;
    type Error = NodeError;

    async fn evaluate(
        &self,
        _node_id: &NodeId<
            dyn TaskNode<Input = Self::Input, Output = Self::Output, Error = Self::Error>,
        >,
        _input: &Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        Ok(0)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, usize>::new();

    let first = task.register_node(FirstNode);
    let second = task.register_node(SecondNode);
    let third = task.register_node(ThirdNode);

    task.starts_with(first);
    task.register_transition(first, move |output: String| {
        if output.is_empty() {
            second.transitions_with(output)
        } else {
            third.transitions_with(output)
        }
    })?;

    Ok(())
}
