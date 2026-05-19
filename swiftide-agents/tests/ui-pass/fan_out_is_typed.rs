use swiftide_agents::tasks::{JoinInput, NodeError, Task, Transition};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, usize>::new();

    let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    let left = task.register_node_fn(|input: &i32| -> Result<usize, NodeError> {
        Ok(usize::try_from(*input).unwrap_or_default())
    });
    let right = task.register_node_fn(|input: &String| -> Result<usize, NodeError> {
        Ok(input.len())
    });
    let join = task.register_node_fn(|input: &JoinInput<usize>| -> Result<usize, NodeError> {
        Ok(input.iter().copied().sum())
    });

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&left, input)
            .and(&right, "right")
            .join_with(join.join())
    })?;
    task.register_transition(left, join.join())?;
    task.register_transition(right, join.join())?;
    task.register_transition(join, task.transitions_to_finish())?;

    Ok(())
}
