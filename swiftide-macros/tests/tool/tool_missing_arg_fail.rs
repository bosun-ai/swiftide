use swiftide::chat_completion::errors::ToolError;
use swiftide::chat_completion::ToolOutput;
use swiftide::traits::AgentContext;

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

const READ_FILE: &str = "Read a file";

#[swiftide_macros::tool(
    description = READ_FILE,
    param(name = "number", description = "Number to guess")
)]
async fn guess_a_number(
    _context: &dyn AgentContext,
    number: usize,
) -> Result<ToolOutput, ToolError> {
    let actual_number = 42;

    if number == actual_number {
        Ok("You guessed it!".into())
    } else {
        Ok("Try again!".into())
    }
}
fn main() {}
