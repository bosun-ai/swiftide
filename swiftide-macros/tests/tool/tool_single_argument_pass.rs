use anyhow::Result;
use swiftide::chat_completion::ToolOutput;
use swiftide::traits::AgentContext;

#[swiftide_macros::tool(
    description = "My first tool",
    param(name = "msg", description = "A message for testing")
)]
async fn basic_tool(_agent_context: &dyn AgentContext, msg: &str) -> Result<ToolOutput> {
    Ok(format!("Hello {msg}").into())
}

fn main() {}
