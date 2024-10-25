use async_trait::async_trait;
use indoc::indoc;
use swiftide_core::chat_completion::ToolOutput;

use crate::Tool;

/// Manually for now so we can delay the macro

#[derive(Debug, Clone, Default)]
pub struct Stop {}

#[async_trait]
impl Tool for Stop {
    async fn invoke(
        &self,
        _agent_context: &dyn crate::AgentContext,
        _raw_args: Option<&str>,
    ) -> anyhow::Result<swiftide_core::chat_completion::ToolOutput> {
        Ok(ToolOutput::Stop)
    }

    fn name(&self) -> &'static str {
        "stop"
    }

    fn json_spec(&self) -> swiftide_core::chat_completion::JsonSpec {
        indoc! {r#"
           {
               "name": "stop",
               "description": "When you have completed, or cannot complete, your task, call this",
           } 

        "#}
    }
}
