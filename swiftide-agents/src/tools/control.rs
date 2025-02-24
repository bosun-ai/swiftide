//! Control tools manage control flow during agent's lifecycle.
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use swiftide_core::{
    chat_completion::{errors::ToolError, Tool, ToolOutput, ToolSpec},
    AgentContext,
};

// TODO: Cannot use macros in our own crates because of import shenanigans
#[derive(Clone, Debug, Default)]
pub struct Stop {}

#[async_trait]
impl Tool for Stop {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        _raw_args: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Stop)
    }

    fn name(&self) -> Cow<'_, str> {
        "stop".into()
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("stop")
            .description("When you have completed, or cannot complete, your task, call this")
            .build()
            .unwrap()
    }
}

impl From<Stop> for Box<dyn Tool> {
    fn from(val: Stop) -> Self {
        Box::new(val)
    }
}
