use swiftide::chat_completion::{errors::ToolError, ToolOutput};
use swiftide::traits::AgentContext;
use swiftide_macros::Tool;

#[derive(Clone, Tool)]
struct MyToolNoArgs {
    test: String,
}

impl MyToolNoArgs {
    async fn my_tool_no_args(
        &self,
        _agent_context: &dyn AgentContext,
    ) -> Result<ToolOutput, ToolError> {
        Ok(format!("Hello world").into())
    }
}

fn main() {}
