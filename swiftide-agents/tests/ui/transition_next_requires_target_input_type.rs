use swiftide_agents::tasks::{NodeError, Task, Transition};

fn main() {
    let mut task = Task::<i32, usize>::new();

    let target = task.register_node_fn(|input: &String| -> Result<usize, NodeError> {
        Ok(input.len())
    });

    let _ = Transition::next(&target, 42);
}
