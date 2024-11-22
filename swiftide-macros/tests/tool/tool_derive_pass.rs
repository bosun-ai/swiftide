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
    async fn my_tool(&self, agent_context: &dyn AgentContext, test: &str) -> Result<ToolOutput> {
        Ok(format!("Hello {test}").into())
    }
}

#[derive(Clone, Tool)]
#[tool(
    description = "Hello tool",
    param(name = "test", description = "My param"),
    param(name = "other", description = "My other param")
)]
struct MyToolMultiParams {}

impl MyToolMultiParams {
    async fn my_tool_multi_params(
        &self,
        agent_context: &dyn AgentContext,
        test: &str,
        other: &str,
    ) -> Result<ToolOutput> {
        Ok(format!("Hello {test} {other}").into())
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

#[derive(Clone, Tool)]
#[tool(description = "Hello tool")]
struct MyToolLifetime<'a> {
    test: &'a str,
}

impl MyToolLifetime<'_> {
    async fn my_tool_lifetime(&self, agent_context: &dyn AgentContext) -> Result<ToolOutput> {
        Ok(format!("Hello world").into())
    }
}

fn main() {}
