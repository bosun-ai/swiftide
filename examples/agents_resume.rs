//! This example illustrates how to resume an agent from existing messages.
use anyhow::Result;
use swiftide::agents::{self, DefaultContext};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Hello, agents!");

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let mut first_agent = agents::Agent::builder().llm(&openai).build()?;

    first_agent.query("Say hello!").await?;

    // Let's store the messages in a database, retrieve them back, and start a new agent
    let stored_history = serde_json::to_string(&first_agent.history().await)?;
    let retrieved_history: Vec<_> = serde_json::from_str(&stored_history)?;

    let restored_context = DefaultContext::default()
        .with_message_history(retrieved_history)
        .to_owned();

    let mut second_agent = agents::Agent::builder()
        .llm(&openai)
        .context(restored_context)
        // We'll use the one from the first agent, alternatively we could also pop it from the
        // previous history and add a new one here
        .no_system_prompt()
        .build()?;

    second_agent.query("What did you say?").await?;

    Ok(())
}
