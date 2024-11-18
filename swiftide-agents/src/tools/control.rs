use anyhow::Result;
use async_trait::async_trait;
use swiftide_core::{chat_completion::ToolOutput, AgentContext, Tool};

// TODO: Cannot use macros in our own crates because of import shenanigans
#[derive(Clone, Debug, Default)]
pub struct Stop {}

#[async_trait]
impl Tool for Stop {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        _raw_args: Option<&str>,
    ) -> Result<ToolOutput> {
        Ok(ToolOutput::Stop)
    }

    fn name(&self) -> &'static str {
        "stop"
    }

    fn json_spec(&self) -> swiftide_core::chat_completion::JsonSpec {
        r#"
        {
            "name": "stop",
            "description": "When you have completed, or cannot complete, your task, call this",
        }
        "#
    }
}