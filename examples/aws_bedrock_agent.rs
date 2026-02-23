//! # [Swiftide] AWS Bedrock Agent Example
//!
//! This example demonstrates a simple agent setup with `AwsBedrock` v2.
//!
//! Requirements:
//! - AWS credentials and region configured (CLI profile or environment variables)
//! - Access to the Bedrock model you choose
//! - A model with tool use support (the Claude model below supports this)
//!
//! [Swiftide]: https://github.com/bosun-ai/swiftide

use anyhow::Result;
use swiftide::{
    agents,
    chat_completion::{ToolOutput, errors::ToolError},
    integrations::aws_bedrock_v2::AwsBedrock,
    traits::{AgentContext, Command},
};

#[swiftide::tool(
    description = "Get the current UTC date and time in RFC3339 format"
)]
async fn current_utc_time(context: &dyn AgentContext) -> Result<ToolOutput, ToolError> {
    let command_output = context
        .executor()
        .exec_cmd(&Command::shell("date -u +\"%Y-%m-%dT%H:%M:%SZ\""))
        .await?;

    Ok(command_output.into())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let bedrock = AwsBedrock::builder()
        .default_prompt_model("anthropic.claude-3-5-sonnet-20241022-v2:0")
        .build()?;

    let mut agent = agents::Agent::builder()
        .llm(&bedrock)
        .tools(vec![current_utc_time()])
        .on_new_message(|_, msg| {
            let rendered = msg.to_string();
            Box::pin(async move {
                println!("{rendered}");
                Ok(())
            })
        })
        .limit(6)
        .build()?;

    agent
        .query("Call current_utc_time once, then tell me the timestamp and stop.")
        .await?;

    Ok(())
}
