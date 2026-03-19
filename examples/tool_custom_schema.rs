use std::borrow::Cow;

use anyhow::Result;
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use swiftide::chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError};
use swiftide::traits::AgentContext;

#[derive(Clone)]
struct WorkflowTool;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(
    title = "WorkflowInstruction",
    description = "Choose a workflow action and optional payload",
    deny_unknown_fields
)]
struct WorkflowInstruction {
    #[schemars(description = "Which workflow action to execute")]
    action: WorkflowAction,
    #[schemars(description = "Optional payload forwarded to the workflow engine")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum WorkflowAction {
    Start,
    Stop,
    Status,
}

#[swiftide::reexports::async_trait::async_trait]
impl Tool for WorkflowTool {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        _tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text(
            "Workflow execution not implemented in this example",
        ))
    }

    fn name<'tool>(&'tool self) -> Cow<'tool, str> {
        Cow::Borrowed("workflow_tool")
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("workflow_tool")
            .description("Executes a workflow action with strict input choices")
            .parameters_schema(workflow_schema())
            .build()
            .expect("tool spec should be valid")
    }
}

fn workflow_schema() -> Schema {
    schema_for!(WorkflowInstruction)
}

fn main() -> Result<()> {
    let tool = WorkflowTool;
    let spec = tool.tool_spec();

    println!(
        "{}",
        serde_json::to_string_pretty(&spec).expect("tool spec should serialize"),
    );

    Ok(())
}
