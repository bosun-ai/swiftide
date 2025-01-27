use swiftide::chat_completion::{errors::ToolError, ToolOutput};
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
        test: &str,
    ) -> Result<ToolOutput, ToolError> {
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
    ) -> Result<ToolOutput, ToolError> {
        Ok(format!("Hello {test} {other}").into())
    }
}

#[derive(Clone, Tool)]
#[tool(description = "Hello tool")]
struct MyToolNoArgs {
    test: String,
}

impl MyToolNoArgs {
    async fn my_tool_no_args(
        &self,
        agent_context: &dyn AgentContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(format!("Hello world").into())
    }
}

#[derive(Clone, Tool)]
#[tool(description = "Hello tool")]
struct MyToolLifetime<'a> {
    test: &'a str,
}

impl MyToolLifetime<'_> {
    async fn my_tool_lifetime(
        &self,
        agent_context: &dyn AgentContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(format!("Hello world").into())
    }
}

const DESCRIPTION: &str = "Hello tool";
#[derive(Clone, Tool)]
#[tool(description = DESCRIPTION)]
struct MyToolConst<'a> {
    test: &'a str,
}

impl MyToolConst<'_> {
    async fn my_tool_const(
        &self,
        agent_context: &dyn AgentContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(format!("Hello world").into())
    }
}
fn main() {}
