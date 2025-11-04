//! Demonstrates how to plug a custom JSON schema into the stop tool for an OpenAI-powered agent.
//!
//! Set the `OPENAI_API_KEY` environment variable before running the example. The agent guides the
//! model to call the `stop` tool with a structured payload that matches the schema defined below.
//! The on-stop hook prints the structured payload that made the agent stop.
use anyhow::Result;
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use swiftide::agents::tools::control::StopWithArgs;
use swiftide::agents::{Agent, StopReason};
use swiftide::traits::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TaskStatus {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct StopPayload {
    status: TaskStatus,
    summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

fn stop_schema() -> Schema {
    schema_for!(StopPayload)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let schema = stop_schema();
    let stop_tool = StopWithArgs::with_parameters_schema(schema.clone());

    println!(
        "stop tool schema:\n{}",
        to_string_pretty(&stop_tool.tool_spec())?,
    );

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-4o-mini")
        .default_embed_model("text-embedding-3-small")
        .build()?;

    let mut builder = Agent::builder();
    builder
        .llm(&openai)
        .without_default_stop_tool()
        .tools([stop_tool.clone()])
        .on_stop(|_, reason, _| {
            Box::pin(async move {
                if let StopReason::RequestedByTool(_, payload) = reason {
                    if let Some(payload) = payload {
                        println!(
                            "agent stopped with structured payload:\n{}",
                            to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string()),
                        );
                    }
                }
                Ok(())
            })
        });

    if let Some(prompt) = builder.system_prompt_mut() {
        prompt
            .with_role("Workflow finisher")
            .with_guidelines([
                "Summarize the work that was just completed and recommend next actions.",
                "When you are done, call the `stop` tool using the provided JSON schema.",
                "Always include the `details` field; use null when there is nothing to add.",
            ])
            .with_constraints(["Never fabricate task status values outside the schema."]);
    }

    let mut agent = builder.build()?;

    agent
        .query_once(
            "You completed onboarding five merchants today. Prepare a final handoff report and stop.",
        )
        .await?;

    Ok(())
}
