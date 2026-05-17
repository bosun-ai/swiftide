use swiftide_agents::tasks::{NodeError, Task, Transition};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, usize>::new();

    let first = task.register_node_fn(|input: &i32| -> Result<String, NodeError> {
        Ok(input.to_string())
    });
    let second = task.register_node_fn(|input: &String| -> Result<usize, NodeError> {
        Ok(input.len())
    });
    let third = task.register_node_fn(|input: &String| -> Result<usize, NodeError> {
        Ok(input.len() + 1)
    });

    task.starts_with(first);
    task.register_transition(first, move |output: String| {
        if output.is_empty() {
            Transition::next(&second, output)
        } else {
            Transition::next(&third, output)
        }
    })?;

    Ok(())
}
