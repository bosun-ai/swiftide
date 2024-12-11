#[swiftide_macros::tool(
    description = "My first tool",
    param(name = "msg", description = "A message for testing")
)]
async fn basic_tool(
    _agent_context: &dyn AgentContext,
    msg: &str,
    other: &str,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Hello {msg}").into())
}

fn main() {}
