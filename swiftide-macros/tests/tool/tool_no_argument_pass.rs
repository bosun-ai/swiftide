use anyhow::Result;
use swiftide_core::chat_completion::ToolOutput;
use swiftide_core::AgentContext;

#[swiftide_macros::tool(description = "My first tool")]
async fn basic_tool(_agent_context: &dyn AgentContext) -> Result<ToolOutput> {
    Ok(format!("Hello tool").into())
}

fn main() {}
