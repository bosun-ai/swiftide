use derive_builder::Builder;
use serde::{Deserialize, Serialize};

/// Output of a `ToolCall` which will be added as a message for the agent to use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolOutput {
    /// Adds the result of the toolcall to messages
    Text(String),

    /// Indicates that the toolcall failed, but can be handled by the llm
    Fail(String),
    /// Stops an agent
    Stop,
}

impl ToolOutput {
    pub fn content(&self) -> Option<&str> {
        match self {
            ToolOutput::Fail(s) | ToolOutput::Text(s) => Some(s),
            _ => None,
        }
    }
}

impl<T: AsRef<str>> From<T> for ToolOutput {
    fn from(s: T) -> Self {
        ToolOutput::Text(s.as_ref().to_string())
    }
}

impl std::fmt::Display for ToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolOutput::Text(value) => write!(f, "{value}"),
            ToolOutput::Fail(value) => write!(f, "Tool call failed: {value}"),
            ToolOutput::Stop => write!(f, "Stop"),
        }
    }
}

/// A tool call that can be executed by the executor
#[derive(Clone, Debug, Builder, PartialEq, Serialize, Deserialize)]
#[builder(setter(into, strip_option))]
pub struct ToolCall {
    id: String,
    name: String,
    #[builder(default)]
    args: Option<String>,
}

/// Hash is used for finding tool calls that have been retried by agents
impl std::hash::Hash for &ToolCall {
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
#[derive(Clone, Debug, Hash, Eq, PartialEq, Default, Builder)]
#[builder(setter(into))]
pub struct ToolSpec {
    /// Name of the tool
    pub name: String,
    /// Description passed to the LLM for the tool
    pub description: String,

    #[builder(default)]
    /// Optional parameters for the tool
    pub parameters: Vec<ParamSpec>,
}

impl ToolSpec {
    pub fn builder() -> ToolSpecBuilder {
        ToolSpecBuilder::default()
    }
}

#[derive(
    Clone, Debug, Hash, Eq, PartialEq, Default, strum_macros::AsRefStr, Serialize, Deserialize,
)]
#[strum(serialize_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum ParamType {
    #[default]
    String,
    Number,
    Boolean,
    Array,
    // Enum
    // Object
    // anyOf
}

/// Parameters for tools
#[derive(Clone, Debug, Hash, Eq, PartialEq, Builder)]
#[builder(setter(into))]
pub struct ParamSpec {
    /// Name of the parameter
    pub name: String,
    /// Description of the parameter
    pub description: String,
    /// Json spec type of the parameter
    #[builder(default)]
    pub ty: ParamType,
    /// Whether the parameter is required
    #[builder(default = true)]
    pub required: bool,
}

impl ParamSpec {
    pub fn builder() -> ParamSpecBuilder {
        ParamSpecBuilder::default()
    }
}
