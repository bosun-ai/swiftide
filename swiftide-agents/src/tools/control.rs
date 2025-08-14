//! Control tools manage control flow during agent's lifecycle.
use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;
use swiftide_core::{
    AgentContext, ToolFeedback,
    chat_completion::{
        ParamSpec, ParamType, Tool, ToolCall, ToolOutput, ToolSpec, errors::ToolError,
    },
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
        Ok(ToolOutput::stop())
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

/// `StopWithArgs` is an alternative stop tool that takes arguments
#[derive(Clone, Debug, Default)]
pub struct StopWithArgs {}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct StopWithArgsSpec {
    pub output: String,
}

#[async_trait]
impl Tool for StopWithArgs {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args: StopWithArgsSpec = serde_json::from_str(
            tool_call
                .args()
                .ok_or(ToolError::missing_arguments("output"))?,
        )?;

        Ok(ToolOutput::stop_with_args(args.output))
    }

    fn name(&self) -> Cow<'_, str> {
        "stop".into()
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("stop")
            .description("When you have completed, your task, call this with your expected output")
            .parameters(vec![
                ParamSpec::builder()
                    .name("output")
                    .description("The expected output of the task")
                    .ty(ParamType::String)
                    .required(true)
                    .build()
                    .unwrap(),
            ])
            .build()
            .unwrap()
    }
}

impl From<StopWithArgs> for Box<dyn Tool> {
    fn from(val: StopWithArgs) -> Self {
        Box::new(val)
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct AgentFailedArgsSpec {
    pub reason: String,
}

/// A utility tool that can be used to let an agent decide it failed
///
/// This will _NOT_ have the agent return an error, instead, look at the stop reason of the agent.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AgentCanFail {}

#[async_trait]
impl Tool for AgentCanFail {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let args: StopWithArgsSpec = serde_json::from_str(
            tool_call
                .args()
                .ok_or(ToolError::missing_arguments("reason"))?,
        )?;

        Ok(ToolOutput::agent_failed(args.output))
    }

    fn name(&self) -> Cow<'_, str> {
        "task_failed".into()
    }

    fn tool_spec(&self) -> ToolSpec {
        ToolSpec::builder()
            .name("stop")
            .description("If you cannot complete your task, or have otherwise failed, call this with your reason for failure")
            .parameters(vec![
                ParamSpec::builder()
                    .name("reason")
                    .description("The reason for failure")
                    .ty(ParamType::String)
                    .required(true)
                    .build()
                    .unwrap(),
            ])
            .build()
            .unwrap()
    }
}

impl From<AgentCanFail> for Box<dyn Tool> {
    fn from(val: AgentCanFail) -> Self {
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
