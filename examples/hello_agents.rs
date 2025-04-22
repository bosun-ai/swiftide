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
use anyhow::Result;
use swiftide::{
    agents,
    chat_completion::{errors::ToolError, ToolOutput},
    traits::{AgentContext, Command},
};

#[swiftide_macros::tool(
    description = "Searches code",
    param(name = "code_query", description = "The code query")
)]
async fn search_code(
    context: &dyn AgentContext,
    code_query: &str,
) -> Result<ToolOutput, ToolError> {
    let command_output = context
        .exec_cmd(&Command::shell(format!("rg '{code_query}'")))
        .await?;

    Ok(command_output.into())
}

const READ_FILE: &str = "Read a file";

#[swiftide_macros::tool(
    description = READ_FILE,
    param(name = "path", description = "Path to the file")
)]
async fn read_file(context: &dyn AgentContext, path: &str) -> Result<ToolOutput, ToolError> {
    let command_output = context
        .exec_cmd(&Command::shell(format!("cat {path}")))
        .await?;

    Ok(command_output.into())
}

// The macro supports strings/strs, vectors/slices, booleans and numbers.
//
// This is currently only supported for the attribute macro, not the derive macro.
//
// If you need more control or need to use full objects, we recommend to implement the `Tool` trait
// and prove the Json spec yourself. Builders are available.
//
// For non-string types, the `json_type` is required to be specified.
#[swiftide_macros::tool(
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
    println!("Hello, agents!");

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_embed_model("text-embeddings-3-small")
        .default_prompt_model("gpt-4o-mini")
        .build()?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("{}", msg);
        }
    });

    agents::Agent::builder()
        .llm(&openai)
        .tools(vec![search_code(), read_file(), guess_a_number()])
        .before_all(move |_context| {
            // This is a hook that runs before any command is executed
            // No native async closures in Rust yet, so we have to use Box::pin
            Box::pin(async move {
                println!("Hello hook!");
                Ok(())
            })
        })
        // Every message added by the agent will be printed to stdout
        .on_new_message(move |_, msg| {
            let msg = msg.to_string();
            let tx = tx.clone();
            Box::pin(async move {
                tx.send(msg).unwrap();
                Ok(())
            })
        })
        .limit(5)
        .build()?
        .query("In what file can I find an example of a swiftide agent? When you are done guess a number and stop")
        .await?;

    Ok(())
}
