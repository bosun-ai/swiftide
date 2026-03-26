use derive_builder::Builder;
use schemars::Schema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use super::tool_schema::{StrictToolParametersSchema, ToolSchemaError};

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
    Stop(Option<serde_json::Value>),

    /// Indicates that the agent failed and should stop
    AgentFailed(Option<serde_json::Value>),
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

    pub fn stop_with_args(output: impl Into<serde_json::Value>) -> Self {
        ToolOutput::Stop(Some(output.into()))
    }

    pub fn agent_failed(output: impl Into<serde_json::Value>) -> Self {
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
    pub fn as_stop(&self) -> Option<&serde_json::Value> {
        match self {
            ToolOutput::Stop(args) => args.as_ref(),
            _ => None,
        }
    }

    /// Get the inner text if the output is an `AgentFailed` variant.
    pub fn as_agent_failed(&self) -> Option<&serde_json::Value> {
        match self {
            ToolOutput::AgentFailed(args) => args.as_ref(),
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
            ToolOutput::Stop(args) => {
                if let Some(value) = args {
                    write!(f, "Stop {value}")
                } else {
                    write!(f, "Stop")
                }
            }
            ToolOutput::FeedbackRequired(_) => {
                write!(f, "Feedback required")
            }
            ToolOutput::AgentFailed(args) => write!(
                f,
                "Agent failed with output: {}",
                args.as_ref().unwrap_or_default()
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
#[builder(setter(into), derive(Debug, Serialize, Deserialize), build_fn(skip))]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Error)]
pub enum ToolSpecError {
    #[error(transparent)]
    InvalidParametersSchema(#[from] ToolSchemaError),
}

#[derive(Debug, Error)]
pub enum ToolSpecBuildError {
    #[error("missing required field `{field}`")]
    MissingField { field: &'static str },
    #[error(transparent)]
    InvalidParametersSchema(#[from] ToolSchemaError),
}

impl ToolSpec {
    pub fn builder() -> ToolSpecBuilder {
        ToolSpecBuilder::default()
    }

    /// Returns the provider-neutral strict parameters schema for this tool.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured parameters schema is not compatible
    /// with Swiftide's strict tool-schema contract.
    pub fn strict_parameters_schema(&self) -> Result<StrictToolParametersSchema, ToolSpecError> {
        Ok(StrictToolParametersSchema::try_from_raw(
            self.parameters_schema.as_ref(),
        )?)
    }
}

impl ToolSpecBuilder {
    /// Builds a tool specification and validates its parameters schema.
    ///
    /// # Errors
    ///
    /// Returns an error when a required field is missing or when the provided
    /// parameters schema is not compatible with Swiftide's strict tool-schema
    /// contract.
    pub fn build(&self) -> Result<ToolSpec, ToolSpecBuildError> {
        let name = self
            .name
            .clone()
            .ok_or(ToolSpecBuildError::MissingField { field: "name" })?;
        let description = self
            .description
            .clone()
            .ok_or(ToolSpecBuildError::MissingField {
                field: "description",
            })?;
        let parameters_schema = self.parameters_schema.clone().unwrap_or(None);

        StrictToolParametersSchema::try_from_raw(parameters_schema.as_ref())?;

        Ok(ToolSpec {
            name,
            description,
            parameters_schema,
        })
    }
}

impl Eq for ToolSpec {}

impl std::hash::Hash for ToolSpec {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.description.hash(state);
        if let Some(schema) = &self.parameters_schema
            && let Ok(serialized) = serde_json::to_vec(schema)
        {
            serialized.hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::collections::HashSet;

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct ExampleArgs {
        value: String,
    }

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct NestedCommentArgs {
        request: NestedCommentRequest,
    }

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct NestedCommentRequest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        discussion_id: Option<String>,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(transparent)]
    struct FreeformObject(serde_json::Map<String, Value>);

    impl schemars::JsonSchema for FreeformObject {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "FreeformObject".into()
        }

        fn json_schema(_generator: &mut schemars::SchemaGenerator) -> Schema {
            serde_json::from_value(json!({
                "type": "object",
                "additionalProperties": true
            }))
            .expect("freeform object schema should serialize")
        }
    }

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct CreateViewArgs {
        request: CreateViewRequest,
    }

    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct CreateViewRequest {
        body: FreeformObject,
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

    #[test]
    fn strict_parameters_schema_returns_canonical_nested_schema() {
        let spec = ToolSpec::builder()
            .name("comment")
            .description("Create a comment")
            .parameters_schema(schemars::schema_for!(NestedCommentArgs))
            .build()
            .unwrap();

        let normalized = spec.strict_parameters_schema().unwrap().into_json();

        assert_eq!(normalized["type"], Value::String("object".into()));
        assert_eq!(normalized["additionalProperties"], Value::Bool(false));
        assert_eq!(
            normalized["required"],
            Value::Array(vec![Value::String("request".into())])
        );

        let nested_ref = normalized["properties"]["request"]["$ref"]
            .as_str()
            .expect("nested request should be referenced");
        let nested_name = nested_ref
            .rsplit('/')
            .next()
            .expect("nested request ref name");
        assert!(
            normalized["$defs"][nested_name].get("required").is_none(),
            "strict schema parsing should preserve optional nested fields before provider shaping"
        );
    }

    #[test]
    fn strict_parameters_schema_sets_additional_properties_false_on_nested_typed_objects() {
        let spec = ToolSpec::builder()
            .name("comment")
            .description("Create a comment")
            .parameters_schema(schemars::schema_for!(NestedCommentArgs))
            .build()
            .unwrap();

        let normalized = spec.strict_parameters_schema().unwrap().into_json();

        let nested_ref = normalized["properties"]["request"]["$ref"]
            .as_str()
            .expect("nested request should be referenced");
        let nested_name = nested_ref
            .rsplit('/')
            .next()
            .expect("nested request ref name");

        assert_eq!(
            normalized["$defs"][nested_name]["additionalProperties"],
            Value::Bool(false)
        );
    }

    #[test]
    fn tool_spec_builder_rejects_nested_freeform_objects_in_strict_mode() {
        let error = ToolSpec::builder()
            .name("create_view")
            .description("Create a view")
            .parameters_schema(schemars::schema_for!(CreateViewArgs))
            .build()
            .expect_err("freeform object should be rejected in strict mode");

        let message = error.to_string();
        assert!(message.contains("strict tool schemas do not support open object schemas"));
        assert!(message.contains("FreeformObject"));
    }

    #[test]
    fn strict_parameters_schema_preserves_optional_nested_fields() {
        let spec = ToolSpec::builder()
            .name("comment")
            .description("Create a comment")
            .parameters_schema(schemars::schema_for!(NestedCommentArgs))
            .build()
            .unwrap();

        let normalized = spec.strict_parameters_schema().unwrap().into_json();

        assert_eq!(normalized["type"], Value::String("object".into()));
        assert_eq!(normalized["additionalProperties"], Value::Bool(false));
        assert_eq!(
            normalized["required"],
            Value::Array(vec![Value::String("request".into())])
        );

        let nested_ref = normalized["properties"]["request"]["$ref"]
            .as_str()
            .expect("nested request should be referenced");
        let nested_name = nested_ref
            .rsplit('/')
            .next()
            .expect("nested request ref name");
        assert_eq!(
            normalized["$defs"][nested_name]["additionalProperties"],
            Value::Bool(false)
        );
        assert!(normalized["$defs"][nested_name].get("required").is_none());
    }
}
