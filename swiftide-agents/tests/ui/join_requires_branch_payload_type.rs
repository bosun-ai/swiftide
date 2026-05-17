use swiftide_agents::tasks::{JoinInput, NodeError, Task, Transition};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, usize>::new();

    let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    let branch = task.register_node_fn(|input: &i32| -> Result<String, NodeError> {
        Ok(input.to_string())
    });
    let join = task.register_node_fn(|input: &JoinInput<usize>| -> Result<usize, NodeError> {
        Ok(input.iter().copied().sum())
    });

    task.starts_with(start);
    task.register_transition(start, move |input| {
        Transition::fan_out(&branch, input).join_with(join.join())
    })?;
    task.register_transition(branch, join.join())?;

    Ok(())
}
