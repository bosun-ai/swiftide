//! This is an example of using a human in the loop pattern with switfide agents.
//!
//! In the example we send the tool call over an channel, and then manually approve it.
//!
//! In a more realistic example, you can use other rust primitives to make it work for your
//! usecase. I.e., make an api request with a callback url that will add the feedback.
//!
//! Both requesting feedback and providing feedback support an optional payload (as a
//! `serde_json::Value`).
//!
//! This allows for more custom workflows, to either display or provide more input to the
//! underlying tool call.
//!
//! For an example on how to implement your own custom wrappers, refer to
//! `tools::control::ApprovalRequired`

use anyhow::Result;
use swiftide::{
    agents::{self, StopReason, tools::control::ApprovalRequired},
    chat_completion::{ToolOutput, errors::ToolError},
    traits::{AgentContext, ToolFeedback},
};
use tracing_subscriber::EnvFilter;

#[swiftide::tool(
    description = "Guess a number",
    param(name = "number", description = "Number to guess")
)]
async fn guess_a_number(
    _context: &dyn AgentContext,
    number: usize,
) -> Result<ToolOutput, ToolError> {
    let actual_number = 42;

    if number == actual_number {
        Ok("You guessed it!".into())
    } else {
        Ok("Try again!".into())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    println!("Hello, agents!");

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-4o")
        .build()?;

    // ApprovalRequired is a simple wrapper. You can also implement your own approval
    // flows by returning a `ToolOutput::FeedbackRequired` in a tool,
    // you can then use `has_received_feedback` and `received_feedback` on the context
    // to build your custom workflow.
    let guess_with_approval = ApprovalRequired::new(guess_a_number());

    let mut agent = agents::Agent::builder()
        .llm(&openai)
        .tools(vec![guess_with_approval])
        // Every message added by the agent will be printed to stdout
        .on_new_message(move |_, msg| {
            println!("{msg}");

            Box::pin(async move { Ok(()) })
        })
        .limit(5)
        .build()?;

    // First query the agent, the agent will stop with a reason that feedback is required
    agent
        .query("Guess a number between 0 and 100 using the `guess_a_number` tool")
        .await?;

    // The agent stopped, lets get the tool call
    let Some(StopReason::FeedbackRequired { tool_call, .. }) = agent.stop_reason() else {
        panic!("expected a tool call to approve")
    };

    // Alternatively, you can also get the stop reason from the agent state
    // agent.state().stop_reason().unwrap().feedback_required().unwrap()

    // Register that this tool call is ok.
    println!("Approving number guessing");
    agent
        .context()
        .feedback_received(tool_call, &ToolFeedback::approved())
        .await
        .unwrap();

    // Run the agent again and it will pick up where it stopped.
    agent.run().await.unwrap();

    Ok(())
}
