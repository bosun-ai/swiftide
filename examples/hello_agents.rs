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
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swiftide::{
    agents,
    chat_completion::{ToolOutput, errors::ToolError},
    traits::{AgentContext, Command},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CodeSearchRequest {
    /// Search query to pass to ripgrep
    query: String,
    /// Optional repository root (defaults to the current working directory)
    repo: Option<String>,
    /// Optional list of glob filters for the search
    file_globs: Option<Vec<String>>,
}

#[swiftide::tool(
    description = "Searches code",
    param(name = "request", description = "Code search parameters")
)]
async fn search_code(
    context: &dyn AgentContext,
    request: CodeSearchRequest,
) -> Result<ToolOutput, ToolError> {
    let repo = request.repo.as_deref().unwrap_or(".");
    let mut command = format!("cd {repo} && rg '{query}'", query = request.query);

    if let Some(globs) = &request.file_globs {
        for glob in globs {
            command.push_str(&format!(" -g '{glob}'"));
        }
    }

    let command_output = context
        .executor()
        .exec_cmd(&Command::shell(command))
        .await?;

    Ok(command_output.into())
}

const READ_FILE: &str = "Read a file";

#[swiftide::tool(
    description = READ_FILE,
    param(name = "path", description = "Path to the file")
)]
async fn read_file(context: &dyn AgentContext, path: &str) -> Result<ToolOutput, ToolError> {
    let command_output = context
        .executor()
        .exec_cmd(&Command::shell(format!("cat {path}")))
        .await?;

    Ok(command_output.into())
}

// The macro understands common Rust types (strings, numbers, bools, vectors, maps, structs, etc.)
// and automatically derives a JSON Schema via `schemars`. If you need to tweak the schema
// manually, implement the `Tool` trait and attach your own `parameters_schema`.
//
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
    println!("Hello, agents!");

    tracing_subscriber::fmt::init();

    let openai = swiftide::integrations::gemini::Gemini::builder()
        .default_embed_model("gemini-embedding-exp-03-07")
        .default_prompt_model("gemini-2.0-flash")
        .build()?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            println!("{msg}");
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
