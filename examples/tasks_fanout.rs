//! This example illustrates how to fan out a task into multiple branches and join them again.

use anyhow::Result;
use swiftide::agents::tasks::{JoinInput, SyncFn, Task, TaskRunState, Transition};

#[tokio::main]
async fn main() -> Result<()> {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(SyncFn::new(|input: &i32| Ok(*input)));
    let increment = task.register_node(SyncFn::new(|input: &i32| Ok(*input + 1)));
    let double = task.register_node(SyncFn::new(|input: &i32| Ok(*input * 2)));
    let join = task.register_node(SyncFn::new(|input: &JoinInput| {
        Ok(input
            .ready_values::<i32>()
            .into_iter()
            .copied()
            .sum::<i32>())
    }));

    task.starts_with(start);

    task.register_transition(start, move |input| {
        Transition::fan_out([increment.target_with(input), double.target_with(input)])
    })?;

    task.register_transition(increment, join.join_at_least(2).cancel_remaining())?;
    task.register_transition(double, join.join_at_least(2).cancel_remaining())?;
    task.register_transition(join, task.transitions_to_finish())?;

    match task.run(5).await? {
        TaskRunState::Completed(result) => println!("Joined result: {result}"),
        TaskRunState::Paused => println!("Task paused"),
    }

    Ok(())
}
