use swiftide_agents::tasks::{NodeError, Task};

fn main() {
    let mut task = Task::<i32, usize>::new();

    let branch = task.register_node_fn(|input: &String| -> Result<usize, NodeError> {
        Ok(input.len())
    });

    let _ = branch.target_with(42);
}
