//! This example illustrates how to resume an agent from existing messages.
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
