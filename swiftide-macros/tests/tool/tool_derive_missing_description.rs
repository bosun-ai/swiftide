use anyhow::Result;
use swiftide::chat_completion::ToolOutput;
use swiftide::traits::AgentContext;
use swiftide_macros::Tool;

#[derive(Clone, Tool)]
struct MyToolNoArgs {
    test: String,
}

impl MyToolNoArgs {
    async fn my_tool_no_args(&self, agent_context: &dyn AgentContext) -> Result<ToolOutput> {
        Ok(format!("Hello world").into())
    }
}

fn main() {}
