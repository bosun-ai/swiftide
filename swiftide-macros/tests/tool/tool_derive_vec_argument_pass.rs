#![allow(unused_variables)]
use swiftide::chat_completion::{errors::ToolError, ToolOutput};
use swiftide::traits::AgentContext;
use swiftide_macros::Tool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, swiftide::reexports::schemars::JsonSchema)]
struct CustomType {
    value: String,
}

#[derive(Clone, Tool)]
#[tool(
    description = "Tool that takes a Vec<CustomType>",
    param(name = "items", description = "items", rust_type = "Vec<CustomType>")
)]
struct VecTool;

impl VecTool {
    async fn vec_tool(
        &self,
        agent_context: &dyn AgentContext,
        items: Vec<CustomType>,
    ) -> Result<ToolOutput, ToolError> {
        Ok(format!("Received {} items", items.len()).into())
    }
}

#[derive(Clone, Tool)]
#[tool(
    description = "Tool that takes nested Vec<CustomType>",
    param(name = "items", description = "nested items", rust_type = "Vec<Vec<CustomType>>")
)]
struct NestedVecTool;

impl NestedVecTool {
    async fn nested_vec_tool(
        &self,
        agent_context: &dyn AgentContext,
        items: Vec<Vec<CustomType>>,
    ) -> Result<ToolOutput, ToolError> {
        Ok(format!("Received {} groups", items.len()).into())
    }
}

fn main() {}
