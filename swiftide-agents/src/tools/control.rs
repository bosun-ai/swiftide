//! Control tools manage control flow during agent's lifecycle.
use anyhow::Result;
use async_trait::async_trait;
use schemars::{Schema, schema_for};
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
#[derive(Clone, Debug)]
pub struct StopWithArgs {
    parameters_schema: Option<Schema>,
    expects_output_field: bool,
}

impl Default for StopWithArgs {
    fn default() -> Self {
        Self {
            parameters_schema: Some(schema_for!(DefaultStopWithArgsSpec)),
            expects_output_field: true,
        }
    }
}

impl StopWithArgs {
    /// Create a new `StopWithArgs` tool with a custom parameters schema.
    ///
    /// When providing a custom schema the full argument payload will be forwarded to the
    /// stop output without requiring an `output` field wrapper.
    pub fn with_parameters_schema(schema: Schema) -> Self {
        Self {
            parameters_schema: Some(schema),
            expects_output_field: false,
        }
    }

    fn parameters_schema(&self) -> Schema {
        self.parameters_schema
            .clone()
            .unwrap_or_else(|| schema_for!(DefaultStopWithArgsSpec))
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
struct DefaultStopWithArgsSpec {
    pub output: String,
}

#[async_trait]
impl Tool for StopWithArgs {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let raw_args = tool_call
            .args()
            .ok_or_else(|| ToolError::missing_arguments("arguments"))?;

        let json: serde_json::Value = serde_json::from_str(raw_args)?;

        let output = if self.expects_output_field {
            json.get("output")
                .cloned()
                .ok_or_else(|| ToolError::missing_arguments("output"))?
        } else {
            json
        };

        Ok(ToolOutput::stop_with_args(output))
    }

    fn name(&self) -> Cow<'_, str> {
        "stop".into()
    }

    fn tool_spec(&self) -> ToolSpec {
        let schema = self.parameters_schema();

        ToolSpec::builder()
            .name("stop")
            .description("When you have completed, your task, call this with your expected output")
            .parameters_schema(schema)
            .build()
            .unwrap()
    }
}

impl From<StopWithArgs> for Box<dyn Tool> {
    fn from(val: StopWithArgs) -> Self {
        Box::new(val)
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
struct AgentFailedArgsSpec {
    pub reason: String,
}

/// A utility tool that can be used to let an agent decide it failed
///
/// This will _NOT_ have the agent return an error, instead, look at the stop reason of the agent.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AgentCanFail {
    parameters_schema: Option<Schema>,
    expects_reason_field: bool,
}

impl Default for AgentCanFail {
    fn default() -> Self {
        Self {
            parameters_schema: Some(schema_for!(AgentFailedArgsSpec)),
            expects_reason_field: true,
        }
    }
}

impl AgentCanFail {
    /// Create a new `AgentCanFail` tool with a custom parameters schema.
    ///
    /// When providing a custom schema the full argument payload will be forwarded to the failure
    /// reason without requiring a `reason` field wrapper.
    pub fn with_parameters_schema(schema: Schema) -> Self {
        Self {
            parameters_schema: Some(schema),
            expects_reason_field: false,
        }
    }

    fn parameters_schema(&self) -> Schema {
        self.parameters_schema
            .clone()
            .unwrap_or_else(|| schema_for!(AgentFailedArgsSpec))
    }
}

#[async_trait]
impl Tool for AgentCanFail {
    async fn invoke(
        &self,
        _agent_context: &dyn AgentContext,
        tool_call: &ToolCall,
    ) -> Result<ToolOutput, ToolError> {
        let raw_args = tool_call.args().ok_or_else(|| {
            if self.expects_reason_field {
                ToolError::missing_arguments("reason")
            } else {
                ToolError::missing_arguments("arguments")
            }
        })?;

        let reason = if self.expects_reason_field {
            let args: AgentFailedArgsSpec = serde_json::from_str(raw_args)?;
            args.reason
        } else {
            let json: serde_json::Value = serde_json::from_str(raw_args)?;
            json.to_string()
        };

        Ok(ToolOutput::agent_failed(reason))
    }

    fn name(&self) -> Cow<'_, str> {
        "task_failed".into()
    }

    fn tool_spec(&self) -> ToolSpec {
        let schema = self.parameters_schema();

        ToolSpec::builder()
            .name("task_failed")
            .description("If you cannot complete your task, or have otherwise failed, call this with your reason for failure")
            .parameters_schema(schema)
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

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::schema_for;
    use serde_json::json;

    fn dummy_tool_call(name: &str, args: Option<&str>) -> ToolCall {
        let mut builder = ToolCall::builder().name(name).id("1").to_owned();
        if let Some(args) = args {
            builder.args(args.to_string());
        }
        builder.build().unwrap()
    }

    #[tokio::test]
    async fn test_stop_tool() {
        let stop = Stop::default();
        let ctx = ();
        let tool_call = dummy_tool_call("stop", None);
        let out = stop.invoke(&ctx, &tool_call).await.unwrap();
        assert_eq!(out, ToolOutput::stop());
    }

    #[tokio::test]
    async fn test_stop_with_args_tool() {
        let tool = StopWithArgs::default();
        let ctx = ();
        let args = r#"{"output":"expected result"}"#;
        let tool_call = dummy_tool_call("stop", Some(args));
        let out = tool.invoke(&ctx, &tool_call).await.unwrap();
        assert_eq!(out, ToolOutput::stop_with_args("expected result"));
    }

    #[tokio::test]
    async fn test_agent_can_fail_tool() {
        let tool = AgentCanFail::default();
        let ctx = ();
        let args = r#"{"reason":"something went wrong"}"#;
        let tool_call = dummy_tool_call("task_failed", Some(args));
        let out = tool.invoke(&ctx, &tool_call).await.unwrap();
        assert_eq!(out, ToolOutput::agent_failed("something went wrong"));
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct CustomFailArgs {
        code: i32,
        message: String,
    }

    #[test]
    fn test_agent_can_fail_custom_schema_in_spec() {
        let schema = schema_for!(CustomFailArgs);
        let tool = AgentCanFail::with_parameters_schema(schema.clone());
        let spec = tool.tool_spec();
        assert_eq!(spec.parameters_schema, Some(schema));
    }

    #[tokio::test]
    async fn test_agent_can_fail_custom_schema_forwards_payload() {
        let schema = schema_for!(CustomFailArgs);
        let tool = AgentCanFail::with_parameters_schema(schema);
        let ctx = ();
        let args = r#"{"code":7,"message":"error"}"#;
        let tool_call = dummy_tool_call("task_failed", Some(args));
        let out = tool.invoke(&ctx, &tool_call).await.unwrap();
        assert_eq!(
            out,
            ToolOutput::agent_failed(json!({"code":7,"message":"error"}).to_string())
        );
    }

    #[test]
    fn test_agent_can_fail_default_schema_matches_previous() {
        let tool = AgentCanFail::default();
        let spec = tool.tool_spec();
        let expected = schema_for!(AgentFailedArgsSpec);
        assert_eq!(spec.parameters_schema, Some(expected));
    }

    #[tokio::test]
    async fn test_approval_required_feedback_required() {
        let stop = Stop::default();
        let tool = ApprovalRequired::new(stop);
        let ctx = ();
        let tool_call = dummy_tool_call("stop", None);
        let out = tool.invoke(&ctx, &tool_call).await.unwrap();

        // On unit; existing feedback is always present
        assert_eq!(out, ToolOutput::Stop(None));
    }

    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct CustomStopArgs {
        value: i32,
    }

    #[test]
    fn test_stop_with_args_custom_schema_in_spec() {
        let schema = schema_for!(CustomStopArgs);
        let tool = StopWithArgs::with_parameters_schema(schema.clone());
        let spec = tool.tool_spec();
        assert_eq!(spec.parameters_schema, Some(schema));
    }

    #[tokio::test]
    async fn test_stop_with_args_custom_schema_forwards_payload() {
        let schema = schema_for!(CustomStopArgs);
        let tool = StopWithArgs::with_parameters_schema(schema);
        let ctx = ();
        let args = r#"{"value":42}"#;
        let tool_call = dummy_tool_call("stop", Some(args));
        let out = tool.invoke(&ctx, &tool_call).await.unwrap();
        assert_eq!(out, ToolOutput::stop_with_args(json!({"value": 42})));
    }

    #[test]
    fn test_stop_with_args_default_schema_matches_previous() {
        let tool = StopWithArgs::default();
        let spec = tool.tool_spec();
        let expected = schema_for!(DefaultStopWithArgsSpec);
        assert_eq!(spec.parameters_schema, Some(expected));
    }
}
