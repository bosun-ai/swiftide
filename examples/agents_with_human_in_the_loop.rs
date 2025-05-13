//! This is an example of how to build a Swiftide agent
//!
//! A swiftide agent runs completions in a loop, optionally with tools, to complete a task
//! autonomously. Agents stop when either the LLM calls the always included `stop` tool, or
//! (configurable) if the last message in the completion chain was from the assistant.
//!
//! Tools can be created by using the `tool` attribute macro as shown here. For more control (i.e.
//! internal state), there
//! is also a `Tool` derive macro for convenience. Anything that implements the `Tool` trait can
//! act as a tool.
//!
//! Agents operate on an `AgentContext`, which is responsible for managaging the completion history
//! and providing access to the outside world. For the latter, the context is expected to have a
//! `ToolExecutor`, which by default runs locally.
//!
//! When building the agent, hooks are available to influence the state, completions, and general
//! behaviour of the agent. Hooks are also traits.
//!
//! Refer to the api documentation for more detailed information.
use std::pin::Pin;

use anyhow::Result;
use swiftide::{
    agents::{self, StopReason, tools::control::ApprovalRequired},
    chat_completion::{ToolCall, ToolOutput, errors::ToolError},
    traits::{AgentContext, Command},
};

// The macro supports strings/strs, vectors/slices, booleans and numbers.
//
// This is currently only supported for the attribute macro, not the derive macro.
//
// If you need more control or need to use full objects, we recommend to implement the `Tool` trait
// and prove the Json spec yourself. Builders are available.
//
// For non-string types, the `json_type` is required to be specified.
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
    tracing_subscriber::fmt().compact().init();

    println!("Hello, agents!");

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ToolCall>();

    // ApprovalRequired is a simple wrapper. You can also implement your own approval
    // flows by returning a `ToolOutput::FeedbackRequired` in a tool,
    // you can then use `has_received_feedback` and `received_feedback` on the context
    // to build your custom workflow.
    let guess_with_approval = ApprovalRequired(guess_a_number());

    let mut agent = agents::Agent::builder()
        .llm(&openai)
        .tools(vec![guess_with_approval])
        // Every message added by the agent will be printed to stdout
        .on_new_message(move |_, msg| {
            println!("{msg}");

            Box::pin(async move { Ok(()) })
        })
        .on_stop(move |_agent, reason, _| {
            if let StopReason::FeedbackRequired { tool_call, .. } = reason {
                tx.send(tool_call).unwrap();
            }

            Box::pin(async { Ok(()) })
        })
        .limit(5)
        .build()?;

    // First query the agent, the agent will stop with a reason that feedback is required
    agent.query("Guess a number between 0 and 100").await?;

    // The agent stopped, lets get the tool call
    let Some(tool_call) = rx.recv().await else {
        panic!("expected a tool call to approve")
    };

    // Register that this tool call is ok.
    println!("Approving number guessing");
    agent
        .context()
        .feedback_received(&tool_call, None)
        .await
        .unwrap();

    // Run the agent again and it will pick up where it stopped.
    agent.run().await.unwrap();

    Ok(())
}
