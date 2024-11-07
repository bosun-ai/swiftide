use anyhow::Result;
use swiftide_agents::AgentContext;
use swiftide_core::chat_completion::ToolOutput;

#[swiftide_macros::tool(
    description = "My first tool",
    param(name = "Message", description = "A message for testing")
)]
async fn basic_tool(_agent_context: &dyn AgentContext, msg: &str) -> Result<ToolOutput> {
    Ok(format!("Hello {msg}").into())
}

fn main() {}
