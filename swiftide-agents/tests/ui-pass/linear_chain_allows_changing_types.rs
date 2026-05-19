use swiftide_agents::tasks::{NodeError, Task};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, bool>::new();

    let first = task.register_node_fn(|input: &i32| -> Result<String, NodeError> {
        Ok(input.to_string())
    });
    let second = task.register_node_fn(|input: &String| -> Result<usize, NodeError> {
        Ok(input.len())
    });
    let third = task.register_node_fn(|input: &usize| -> Result<bool, NodeError> { Ok(*input > 0) });

    task.starts_with(first);
    task.register_transition(first, move |output| second.transitions_with(output))?;
    task.register_transition(second, move |output| third.transitions_with(output))?;
    task.register_transition(third, task.transitions_to_finish())?;

    Ok(())
}
