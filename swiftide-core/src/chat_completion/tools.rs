use std::borrow::Cow;

use derive_builder::Builder;
use schemars::Schema;
use serde::{Deserialize, Serialize};

/// Output of a `ToolCall` which will be added as a message for the agent to use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, strum_macros::EnumIs)]
#[non_exhaustive]
pub enum ToolOutput {
    /// Adds the result of the toolcall to messages
    Text(String),

    /// Indicates that the toolcall requires feedback, i.e. in a human-in-the-loop
    FeedbackRequired(Option<serde_json::Value>),

    /// Indicates that the toolcall failed, but can be handled by the llm
    Fail(String),

    /// Stops an agent with an optional message
    Stop(Option<Cow<'static, str>>),

    /// Indicates that the agent failed and should stop
    AgentFailed(Option<Cow<'static, str>>),
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        ToolOutput::Text(text.into())
    }

    pub fn feedback_required(feedback: Option<serde_json::Value>) -> Self {
        ToolOutput::FeedbackRequired(feedback)
    }

    pub fn stop() -> Self {
        ToolOutput::Stop(None)
    }

    pub fn stop_with_args(output: impl Into<Cow<'static, str>>) -> Self {
        ToolOutput::Stop(Some(output.into()))
    }

    pub fn agent_failed(output: impl Into<Cow<'static, str>>) -> Self {
        ToolOutput::AgentFailed(Some(output.into()))
    }

    pub fn fail(text: impl Into<String>) -> Self {
        ToolOutput::Fail(text.into())
    }

    pub fn content(&self) -> Option<&str> {
        match self {
            ToolOutput::Fail(s) | ToolOutput::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Get the inner text if the output is a `Text` variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ToolOutput::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Get the inner text if the output is a `Fail` variant.
    pub fn as_fail(&self) -> Option<&str> {
        match self {
            ToolOutput::Fail(s) => Some(s),
            _ => None,
        }
    }

    /// Get the inner text if the output is a `Stop` variant.
    pub fn as_stop(&self) -> Option<&str> {
        match self {
            ToolOutput::Stop(args) => args.as_deref(),
            _ => None,
        }
    }

    /// Get the inner text if the output is an `AgentFailed` variant.
    pub fn as_agent_failed(&self) -> Option<&str> {
        match self {
            ToolOutput::AgentFailed(args) => args.as_deref(),
            _ => None,
        }
    }

    /// Get the inner feedback if the output is a `FeedbackRequired` variant.
    pub fn as_feedback_required(&self) -> Option<&serde_json::Value> {
        match self {
            ToolOutput::FeedbackRequired(args) => args.as_ref(),
            _ => None,
        }
    }
}

impl<S: AsRef<str>> From<S> for ToolOutput {
    fn from(value: S) -> Self {
        ToolOutput::Text(value.as_ref().to_string())
    }
}
impl std::fmt::Display for ToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolOutput::Text(value) => write!(f, "{value}"),
            ToolOutput::Fail(value) => write!(f, "Tool call failed: {value}"),
            ToolOutput::Stop(args) => write!(f, "Stop {}", args.as_deref().unwrap_or_default()),
            ToolOutput::FeedbackRequired(_) => {
                write!(f, "Feedback required")
            }
            ToolOutput::AgentFailed(args) => write!(
                f,
                "Agent failed with output: {}",
                args.as_deref().unwrap_or_default()
            ),
        }
    }
}

/// A tool call that can be executed by the executor
#[derive(Clone, Debug, Builder, PartialEq, Serialize, Deserialize, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[builder(setter(into, strip_option))]
pub struct ToolCall {
    id: String,
    name: String,
    #[builder(default)]
    args: Option<String>,
}

/// Hash is used for finding tool calls that have been retried by agents
impl std::hash::Hash for ToolCall {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.args.hash(state);
    }
}

impl std::fmt::Display for ToolCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{id}#{name} {args}",
            id = self.id,
            name = self.name,
            args = self.args.as_deref().unwrap_or("")
        )
    }
}

impl ToolCall {
    pub fn builder() -> ToolCallBuilder {
        ToolCallBuilder::default()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn args(&self) -> Option<&str> {
        self.args.as_deref()
    }

    pub fn with_args(&mut self, args: Option<String>) {
        self.args = args;
    }
}

impl ToolCallBuilder {
    pub fn maybe_args<T: Into<Option<String>>>(&mut self, args: T) -> &mut Self {
        self.args = Some(args.into());
        self
    }

    pub fn maybe_id<T: Into<Option<String>>>(&mut self, id: T) -> &mut Self {
        self.id = id.into();
        self
    }

    pub fn maybe_name<T: Into<Option<String>>>(&mut self, name: T) -> &mut Self {
        self.name = name.into();
        self
    }
}

/// A typed tool specification intended to be usable for multiple LLMs
///
/// i.e. the json spec `OpenAI` uses to define their tools
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Builder, Default)]
#[builder(setter(into), derive(Debug, Serialize, Deserialize))]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ToolSpec {
    /// Name of the tool
    pub name: String,
    /// Description passed to the LLM for the tool
    pub description: String,

    #[builder(default, setter(strip_option))]
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Optional JSON schema describing the tool arguments
    pub parameters_schema: Option<Schema>,
}

impl ToolSpec {
    pub fn builder() -> ToolSpecBuilder {
        ToolSpecBuilder::default()
    }
}

impl Eq for ToolSpec {}

impl std::hash::Hash for ToolSpec {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.description.hash(state);
        if let Some(schema) = &self.parameters_schema {
            if let Ok(serialized) = serde_json::to_vec(schema) {
                serialized.hash(state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct ExampleArgs {
        value: String,
    }

    #[test]
    fn tool_spec_serializes_schema() {
        let schema = schemars::schema_for!(ExampleArgs);

        let spec = ToolSpec::builder()
            .name("example")
            .description("An example tool")
            .parameters_schema(schema)
            .build()
            .unwrap();

        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json.get("name").and_then(|v| v.as_str()), Some("example"));
        assert!(json.get("parameters_schema").is_some());
    }

    #[test]
    fn tool_spec_is_hashable() {
        let schema = schemars::schema_for!(ExampleArgs);
        let spec = ToolSpec::builder()
            .name("example")
            .description("An example tool")
            .parameters_schema(schema)
            .build()
            .unwrap();

        let mut set = HashSet::new();
        set.insert(spec.clone());

        assert!(set.contains(&spec));
    }
}
