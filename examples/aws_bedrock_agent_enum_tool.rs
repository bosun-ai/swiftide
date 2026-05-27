//! Reproduces Bedrock Converse tool-schema handling for enum arguments.
//!
//! The tool below uses the normal `#[swiftide::tool]` + `schemars` flow. Running this example
//! prints the generated tool schema before sending a single agent request to Bedrock.

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swiftide::{
    agents,
    chat_completion::{ToolOutput, errors::ToolError},
    integrations::aws_bedrock_v2::AwsBedrock,
    traits::AgentContext,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TicketPriority {
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct TriageTicketRequest {
    /// Support ticket title.
    title: String,
    /// Priority to assign to the ticket.
    priority: TicketPriority,
}

#[swiftide::tool(
    description = "Triage a support ticket with a fixed priority",
    param(name = "request", description = "Support ticket triage request")
)]
async fn triage_ticket(
    _context: &dyn AgentContext,
    request: TriageTicketRequest,
) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::text(format!(
        "triaged `{}` as {:?}",
        request.title, request.priority
    )))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let tool = triage_ticket();
    let spec = tool.tool_spec();

    println!("generated tool spec:");
    println!("{}", serde_json::to_string_pretty(&spec)?);
    println!();
    println!("canonical parameters schema sent to Bedrock:");
    println!(
        "{}",
        serde_json::to_string_pretty(&spec.canonical_parameters_schema_json()?)?
    );

    if std::env::var_os("SWIFTIDE_SCHEMA_ONLY").is_some() {
        return Ok(());
    }

    let bedrock = AwsBedrock::builder()
        .default_prompt_model("global.anthropic.claude-sonnet-4-6")
        .build()?;

    let mut agent = agents::Agent::builder()
        .llm(&bedrock)
        .without_default_stop_tool()
        .tools([tool])
        .on_new_message(|_, msg| {
            let rendered = msg.to_string();
            Box::pin(async move {
                println!("{rendered}");
                Ok(())
            })
        })
        .limit(1)
        .build()?;

    agent
        .query(
            "Call triage_ticket once for a ticket titled \"Database is down\" with high priority.",
        )
        .await?;

    Ok(())
}
