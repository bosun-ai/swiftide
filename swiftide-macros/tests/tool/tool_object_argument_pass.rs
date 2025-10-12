use std::collections::BTreeMap;

use serde_json::Value;
use swiftide::chat_completion::{errors::ToolError, ToolOutput};
use swiftide::traits::AgentContext;

#[swiftide_macros::tool(
    description = "Tool that accepts object payloads",
    param(name = "payload", description = "Arbitrary JSON object")
)]
async fn object_tool(
    _ctx: &dyn AgentContext,
    payload: BTreeMap<String, Value>,
) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::text(format!("keys={}", payload.len())))
}

fn main() {}
