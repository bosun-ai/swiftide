use swiftide_agents::tasks::{AnyJoinInput, NodeError, Task, Transition};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, usize>::new();

    let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    let left = task.register_node_fn(|input: &i32| -> Result<usize, NodeError> {
        Ok(usize::try_from(*input).unwrap_or_default())
    });
    let right = task.register_node_fn(|input: &i32| -> Result<String, NodeError> {
        Ok(input.to_string())
    });
    let join = task.register_node_fn(|input: &AnyJoinInput| -> Result<usize, NodeError> {
        Ok(input.iter::<usize>().copied().sum::<usize>() + input.iter_any().count())
    });

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&left, input)
            .and(&right, input)
            .join_with(join.join_any())
    })?;
    task.register_transition(left, join.join_any())?;
    task.register_transition(right, join.join_any())?;
    task.register_transition(join, task.transitions_to_finish())?;

    Ok(())
}
