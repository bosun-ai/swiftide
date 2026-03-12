//! This example illustrates how to fan out a task into multiple branches and join them again.

use anyhow::Result;
use swiftide::agents::tasks::{
    closures::SyncFn,
    task::Task,
    transition::{JoinInput, JoinPolicy, TransitionDirective},
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut task: Task<i32, i32> = Task::new();

    let start = task.register_node(SyncFn::new(|input: &i32| Ok(*input)));
    let increment = task.register_node(SyncFn::new(|input: &i32| Ok(*input + 1)));
    let double = task.register_node(SyncFn::new(|input: &i32| Ok(*input * 2)));
    let join = task.register_node(SyncFn::new(|input: &JoinInput| {
        Ok(input.ready_values::<i32>().into_iter().copied().sum())
    }));

    task.starts_with(start);

    task.register_transition_directive(start, move |input| {
        TransitionDirective::fan_out([increment.target_with(input), double.target_with(input)])
            .with_join(join, JoinPolicy::All)
    })?;

    task.register_transition_directive(increment, move |output| TransitionDirective::join(output))?;
    task.register_transition_directive(double, move |output| TransitionDirective::join(output))?;
    task.register_transition(join, task.transitions_to_done())?;

    let result = task.run(5).await?;

    println!("Joined result: {result:?}");

    Ok(())
}
