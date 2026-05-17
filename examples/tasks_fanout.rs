//! This example illustrates how to fan out a task into multiple branches and join them again.

use anyhow::Result;
use swiftide::agents::tasks::{
    ConcurrencyModel, JoinInput, NodeError, Task, TaskRunState, Transition,
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut task: Task<i32, i32> = Task::builder()
        .concurrency_model(ConcurrencyModel::Parallel)
        .build();

    let start =
        task.register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input) });
    let increment = task
        .register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input + 1) });
    let double = task
        .register_node_fn(|input: &i32| -> std::result::Result<i32, NodeError> { Ok(*input * 2) });
    let join = task.register_node_fn(
        |input: &JoinInput<i32>| -> std::result::Result<i32, NodeError> {
            Ok(input.iter().copied().sum::<i32>())
        },
    );

    task.starts_with(start);

    // The fan-out transition defines the branch group and the join that waits for that group.
    task.register_transition(start, move |input| {
        Transition::fan_out(&increment, input)
            .and(&double, input)
            .join_with(join.join())
    })?;

    // Each branch still declares where its own output should go. Branches can do any amount of work
    // before eventually transitioning to the join.
    task.register_transition(increment, join.join())?;
    task.register_transition(double, join.join())?;
    task.register_transition(join, task.transitions_to_finish())?;

    match task.run(5).await? {
        TaskRunState::Completed(result) => {
            assert_eq!(result, 16);
            println!("Joined result: {result}");
        }
        TaskRunState::Paused => println!("Task paused"),
    }

    Ok(())
}
