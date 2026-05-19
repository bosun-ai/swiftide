use swiftide_agents::tasks::{NodeError, Task, Transition};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut task = Task::<i32, i32>::new();

    let start = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input) });
    let left = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 1) });
    let right = task.register_node_fn(|input: &i32| -> Result<i32, NodeError> { Ok(*input + 2) });

    task.starts_with(start);
    task.register_transition(start, move |value| {
        Transition::fan_out(&left, value).and(&right, value)
    })?;

    Ok(())
}
