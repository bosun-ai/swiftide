use anyhow::Result;
use swiftide::chat_completion::ToolOutput;
use swiftide::traits::AgentContext;

#[swiftide_macros::tool(description = "My first tool")]
async fn basic_tool(_agent_context: &dyn AgentContext) -> Result<ToolOutput> {
    Ok(format!("Hello tool").into())
}

fn main() {}
