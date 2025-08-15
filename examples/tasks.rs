//! This example illustrates how to set up a basic tasks
//!
//! Tasks follow  a graph model where each output of a node must match the input of the next node.
//!
//! To set up a task, you register nodes that implement the `TaskNode` trait. Most swiftide
//! primiteves implement this trait, including agents, prompts, and closures.
//!
//! Then each node can be connected to the next node using the `register_transition` method. There
//! is also a `register_transition_async` method that allows you to register an async transition.
//!
//! Since running an autonomous agent in a task is subject to taste, there is a basic
//! `TaskAgent` that wraps it in an `Arc<Mutex>`, but your own implementation might want to toy
//! with the state instead of the task instead.
//!
//! The API for closures as task nodes is still a bit clunky and subject to change.
use anyhow::Result;
use swiftide::{
    agents::{
        self,
        tasks::{closures::SyncClosureTaskNode, impls::TaskAgent, task::Task},
    },
    prompt::Prompt,
};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, agents!");

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let agent = agents::Agent::builder().llm(&openai).build()?;

    let mut task: Task<Prompt, ()> = Task::new();

    let agent_id = task.register_node(TaskAgent::from(agent));

    let hello_id = task.register_node(SyncClosureTaskNode::new(move |_context: &()| {
        println!("Hello from a task!");

        Ok(())
    }));

    task.starts_with(agent_id);

    // Async is also supported
    task.register_transition_async(agent_id, move |context| {
        Box::pin(async move { hello_id.transitions_with(context) })
    })?;
    task.register_transition(hello_id, task.transitions_to_done())?;

    task.run("Hello there!").await?;

    Ok(())
}
