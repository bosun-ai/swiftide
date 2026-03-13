//! This example illustrates how to fan out a task into multiple branches and join them again.

use anyhow::Result;
use swiftide::agents::tasks::{
    JoinInput, JoinLeftoverBehavior, JoinPolicy, SyncFn, Task, TransitionDirective,
};

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
        TransitionDirective::fan_out_join(
            [increment.target_with(input), double.target_with(input)],
            join,
            JoinPolicy::AtLeast {
                count: 2,
                leftovers: JoinLeftoverBehavior::CancelRemaining,
            },
        )
    })?;

    task.register_transition(increment, move |output| TransitionDirective::join(output))?;
    task.register_transition(double, move |output| TransitionDirective::join(output))?;
    task.register_transition(join, TransitionDirective::finish)?;

    let result = task.run(5).await?;

    println!("Joined result: {result:?}");

    Ok(())
}
