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
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swiftide::{
    agents,
    chat_completion::{ToolOutput, errors::ToolError},
    integrations::aws_bedrock_v2::AwsBedrock,
    traits::{AgentContext, Command},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct FormatTimestampRequest {
    /// Prefix to prepend to the timestamp.
    prefix: String,
    /// Timestamp to format.
    timestamp: String,
}

#[swiftide::tool(description = "Get the current UTC date and time in RFC3339 format")]
async fn current_utc_time(context: &dyn AgentContext) -> Result<ToolOutput, ToolError> {
    let command_output = context
        .executor()
        .exec_cmd(&Command::shell("date -u +\"%Y-%m-%dT%H:%M:%SZ\""))
        .await?;

    Ok(command_output.into())
}

#[swiftide::tool(
    description = "Format a timestamp with a caller-provided prefix",
    param(name = "request", description = "Timestamp formatting input")
)]
async fn format_timestamp(
    _context: &dyn AgentContext,
    request: FormatTimestampRequest,
) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::text(format!(
        "{}{}",
        request.prefix, request.timestamp
    )))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let bedrock = AwsBedrock::builder()
        .default_prompt_model("global.anthropic.claude-sonnet-4-6")
        .build()?;

    let mut agent = agents::Agent::builder()
        .llm(&bedrock)
        .tools(vec![current_utc_time(), format_timestamp()])
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
        .query(
            "Call current_utc_time once. Then call format_timestamp with prefix \"UTC now: \" and \
             that timestamp. After that, report the formatted result and stop.",
        )
        .await?;

    Ok(())
}
