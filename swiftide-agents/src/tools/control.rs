//! Control tools manage control flow during agent's lifecycle.
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use swiftide_core::{
    AgentContext, ToolFeedback,
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
/// Wraps a tool and requires approval before it can be used
pub struct ApprovalRequired(pub Box<dyn Tool>);

impl ApprovalRequired {
    /// Creates a new `ApprovalRequired` tool
    pub fn new(tool: impl Tool + 'static) -> Self {
        Self(Box::new(tool))
    }
}

#[async_trait]
impl Tool for ApprovalRequired {
    async fn invoke(
        &self,
        context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        if let Some(feedback) = context.has_received_feedback(tool_call).await {
            match feedback {
                ToolFeedback::Approved { .. } => return self.0.invoke(context, tool_call).await,
                ToolFeedback::Refused { .. } => {
                    return Ok(ToolOutput::text("This tool call was refused"));
                }
            }
        }

        Ok(ToolOutput::FeedbackRequired(None))
    }

    fn name(&self) -> Cow<'_, str> {
        self.0.name()
    }

    fn tool_spec(&self) -> ToolSpec {
        self.0.tool_spec()
    }
}

impl From<ApprovalRequired> for Box<dyn Tool> {
    fn from(val: ApprovalRequired) -> Self {
        Box::new(val)
    }
}
