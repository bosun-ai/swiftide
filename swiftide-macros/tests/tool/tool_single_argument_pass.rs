use swiftide::chat_completion::{errors::ToolError, ToolOutput};
use swiftide::traits::AgentContext;

#[swiftide_macros::tool(
    description = "My first tool",
    param(name = "msg", description = "A message for testing")
)]
async fn basic_tool(_agent_context: &dyn AgentContext, msg: &str) -> Result<ToolOutput, ToolError> {
    Ok(format!("Hello {msg}").into())
}

#[swiftide_macros::tool(
    description = "My first num tool",
    param(
        name = "msg",
        description = "A message for testing",
        json_type = "number"
    )
)]
async fn basic_tool_num(
    _agent_context: &dyn AgentContext,
    msg: i32,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Hello {msg}").into())
}

#[swiftide_macros::tool(
    description = "My first num tool",
    param(name = "msg", description = "A message for testing")
)]
async fn basic_tool_num_no_type(
    _agent_context: &dyn AgentContext,
    msg: i32,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Hello {msg}").into())
}

#[swiftide_macros::tool(
    description = "My first array tool",
    param(
        name = "msg",
        description = "A message for testing",
        json_type = "array"
    )
)]
async fn basic_tool_vec(
    _agent_context: &dyn AgentContext,
    msg: Vec<String>,
) -> Result<ToolOutput, ToolError> {
    let msg = msg.join(", ");
    Ok(format!("Hello {msg}").into())
}

#[swiftide_macros::tool(
    description = "My first bool tool",
    param(
        name = "msg",
        description = "A message for testing",
        json_type = "boolean"
    )
)]
async fn basic_tool_bool(
    _agent_context: &dyn AgentContext,
    msg: bool,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Hello {msg}").into())
}

#[swiftide_macros::tool(
    description = "My first num slice tool",
    param(
        name = "msg",
        description = "A message for testing",
        json_type = "array"
    )
)]
async fn basic_tool_num_slice(
    _agent_context: &dyn AgentContext,
    msg: &[i32],
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Hello {msg:?}").into())
}

#[swiftide_macros::tool(
    description = "My first num slice tool",
    param(name = "msg", description = "A message for testing")
)]
async fn basic_tool_num_optional(
    _agent_context: &dyn AgentContext,
    msg: Option<i32>,
) -> Result<ToolOutput, ToolError> {
    Ok(format!("Hello {msg:?}").into())
}

fn main() {}
