use anyhow::Result;
use swiftide_core::{chat_completion::ToolOutput, AgentContext};
use swiftide_macros::tool;

#[tool(description = "When you have completed, or cannot complete, your task, call this")]
async fn stop(_agent_context: &dyn AgentContext) -> Result<ToolOutput> {
    Ok(ToolOutput::Stop)
}
