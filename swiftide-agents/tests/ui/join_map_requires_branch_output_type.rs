use swiftide_agents::tasks::{JoinInput, NodeError, Task};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, usize>::new();

    let branch = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    let join = task.register_node_fn(|input: &JoinInput| -> Result<usize, NodeError> {
        Ok(input.ready_values::<usize>().len())
    });

    task.starts_with(branch);
    task.register_transition(branch, join.join().map(|value: String| value.len()))?;

    Ok(())
}
