//! Control tools manage control flow during agent's lifecycle.
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use swiftide_core::{
    AgentContext,
    chat_completion::{Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError},
};

/// `Stop` tool is a default tool used by agents to stop
#[derive(Clone, Debug, Default)]
pub struct Stop {}

#[async_trait]
impl Tool for Stop {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        _tool_call: &ToolCall,
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

#[derive(Clone)]
pub struct FeedbackRequired(pub Box<dyn Tool>);

#[async_trait]
impl Tool for FeedbackRequired {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        _tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::Stop)
    }

    fn name(&self) -> Cow<'_, str> {
        self.0.name()
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("stop")
            .description("When you have completed, or cannot complete, your task, call this")
            .build()
            .unwrap()
    }
}
