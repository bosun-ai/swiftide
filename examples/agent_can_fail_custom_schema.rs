//! Demonstrates how to replace the default failure arguments for `AgentCanFail` with a custom
//! JSON schema and capture the structured failure payload when the agent stops.
//!
//! Set the `OPENAI_API_KEY` environment variable before running the example. The agent is guided
//! to use the `task_failed` tool with the schema defined below whenever it cannot complete the
//! task.
use anyhow::Result;
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{self, to_string_pretty};
use swiftide::agents::tools::control::AgentCanFail;
use swiftide::agents::{Agent, StopReason};
use swiftide::traits::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FailureCategory {
    MissingDependency,
    PermissionDenied,
    UnexpectedRegression,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RemediationStatus {
    Planned,
    Blocked,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct FailureReport {
    category: FailureCategory,
    summary: String,
    impact: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recommended_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remediation_status: Option<RemediationStatus>,
}

fn failure_schema() -> Schema {
    schema_for!(FailureReport)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let schema = failure_schema();
    let failure_tool = AgentCanFail::with_parameters_schema(schema.clone());

    println!(
        "task_failed tool schema:\n{}",
        to_string_pretty(&failure_tool.tool_spec())?,
    );

    let openai = swiftide::integrations::openai::OpenAI::builder()
        .default_prompt_model("gpt-4o-mini")
        .default_embed_model("text-embedding-3-small")
        .build()?;

    let mut builder = Agent::builder();
    builder
        .llm(&openai)
        .tools([failure_tool.clone()])
        .on_stop(|_, reason, _| {
            Box::pin(async move {
                if let StopReason::AgentFailed(Some(payload)) = reason {
                    let json = to_string_pretty(&payload).unwrap();
                    println!("agent reported failure:\n{json}");
                }
                Ok(())
            })
        });

    if let Some(prompt) = builder.system_prompt_mut() {
        prompt
            .with_role("Incident response coordinator")
            .with_guidelines([
                "If the task cannot be completed, call the `task_failed` tool using the provided JSON schema.",
                "Populate all required fields and list at least one `recommended_actions` entry.",
                "Clearly document the impact so downstream teams can prioritize remediation.",
            ])
            .with_constraints(["Do not claim success when blockers remain unresolved."]);
    }

    let mut agent = builder.build()?;

    agent
        .query_once(
            "You must restore last night's database backup, but the only backup file is corrupted and no redundant copy exists. Report the failure.",
        )
        .await?;

    Ok(())
}
