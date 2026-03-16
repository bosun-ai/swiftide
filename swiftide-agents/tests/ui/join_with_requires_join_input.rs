use async_trait::async_trait;
use swiftide_agents::tasks::{NodeId, Task, TaskNode};

#[derive(Debug, thiserror::Error)]
#[error("error")]
struct Error;

#[derive(Clone, Debug)]
struct WrongJoinNode;

#[async_trait]
impl TaskNode for WrongJoinNode {
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
        Ok(*input)
    }
}

fn main() {
    let mut task: Task<i32, i32> = Task::new();
    let join = task.register_node(WrongJoinNode);

    let _ = join.join();
    let _ = join.join_with(swiftide_agents::tasks::JoinPolicy::All);
    let _ = join.join_at_least(2);
}
