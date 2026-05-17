//! This example illustrates how to fan out a task into multiple branches and join them again.

use anyhow::Result;
use swiftide::agents::tasks::{JoinInput, NodeError, Task, TaskRunState, Transition};

#[tokio::main]
async fn main() -> Result<()> {
    let mut task: Task<i32, i32> = Task::new();

    let start =
        task.register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input) });
    let increment = task
        .register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input + 1) });
    let double = task
        .register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input * 2) });
    let join = task.register_node_fn(|input: &JoinInput| -> std::result::Result<i32, NodeError> {
        Ok(input
            .ready_values::<i32>()
            .into_iter()
            .copied()
            .sum::<i32>())
    });

    task.starts_with(start);

    task.register_transition(start, move |input| {
        Transition::fan_out([increment.target_with(input), double.target_with(input)])
            .join_with(join.join())
    })?;

    task.register_transition(increment, join.join())?;
    task.register_transition(double, join.join())?;
    task.register_transition(join, task.transitions_to_finish())?;

    match task.run(5).await? {
        TaskRunState::Completed(result) => println!("Joined result: {result}"),
        TaskRunState::Paused => println!("Task paused"),
    }

    Ok(())
}
