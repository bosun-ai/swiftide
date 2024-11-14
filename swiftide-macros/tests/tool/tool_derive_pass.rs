use anyhow::Result;
use swiftide::chat_completion::ToolOutput;
use swiftide::traits::AgentContext;
use swiftide_macros::Tool;

#[derive(Clone, Tool)]
#[tool(
    description = "Hello tool",
    param(name = "test", description = "My param")
)]
struct MyTool {
    test: String,
}

impl MyTool {
    async fn my_tool(
        &self,
        agent_context: &dyn AgentContext,
        args: MyToolArgs,
    ) -> Result<ToolOutput> {
        let arg = args.test;
        Ok(format!("Hello {arg}").into())
    }
}

#[derive(Clone, Tool)]
#[tool(description = "Hello tool")]
struct MyToolNoArgs {
    test: String,
}

impl MyToolNoArgs {
    async fn my_tool_no_args(&self, agent_context: &dyn AgentContext) -> Result<ToolOutput> {
        Ok(format!("Hello world").into())
    }
}

fn main() {}
