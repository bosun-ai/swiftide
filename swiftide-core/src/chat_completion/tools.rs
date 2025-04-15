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
    #[serde(with = "param_type_serde")]
    Nullable(Box<ParamType>), /* Enum
                               * Object
                               * anyOf */
}

mod param_type_serde {
    use super::ParamType;
    use serde::de::{Deserializer, Error as DeError, SeqAccess, Unexpected, Visitor};
    use serde::ser::{Error as SerError, SerializeSeq, Serializer};
    use std::fmt;

    pub fn serialize<S>(value: &ParamType, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            // Single variants => single string
            ParamType::String => serializer.serialize_str("string"),
            ParamType::Number => serializer.serialize_str("number"),
            ParamType::Boolean => serializer.serialize_str("boolean"),
            ParamType::Array => serializer.serialize_str("array"),

            // Nullable => 2-element array, e.g. ["string", "null"]
            ParamType::Nullable(inner) => {
                // If `inner` itself is `Nullable`, we can decide what to do:
                // for example, reject nested nullables altogether:
                if let ParamType::Nullable(_) = inner.as_ref() {
                    return Err(S::Error::custom("Nested Nullable is not supported"));
                }

                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element(&primitive_variant_name(inner).map_err(S::Error::custom)?)?;
                seq.serialize_element("null")?;
                seq.end()
            }
        }
    }

    /// Returns the string for a non-nullable `ParamType` or an error if it is itself Nullable.
    fn primitive_variant_name(pt: &ParamType) -> Result<&'static str, &'static str> {
        match pt {
            ParamType::String => Ok("string"),
            ParamType::Number => Ok("number"),
            ParamType::Boolean => Ok("boolean"),
            ParamType::Array => Ok("array"),
            ParamType::Nullable(_) => Err("Found nested Nullable while serializing"),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Box<ParamType>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ParamTypeVisitor).map(Box::new)
    }

    struct ParamTypeVisitor;

    impl<'de> Visitor<'de> for ParamTypeVisitor {
        type Value = ParamType;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str(
                "a string (\"string\", \"number\", etc.) \
                 or a 2-element array that includes \"null\"",
            )
        }

        // For single strings: "string", "number", "boolean", or "array".
        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            match value {
                "string" => Ok(ParamType::String),
                "number" => Ok(ParamType::Number),
                "boolean" => Ok(ParamType::Boolean),
                "array" => Ok(ParamType::Array),
                other => Err(E::unknown_variant(
                    other,
                    &["string", "number", "boolean", "array"],
                )),
            }
        }

        // For arrays: e.g. ["string", "null"] or ["null", "array"].
        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut items = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                items.push(item);
            }

            // We only handle exactly 2 items, one of them must be "null"
            if items.len() != 2 {
                return Err(A::Error::invalid_length(items.len(), &"2"));
            }

            let mut t1 = items[0].as_str();
            let mut t2 = items[1].as_str();

            // If the first is "null", swap so that t1 is the real type, t2 = "null"
            if t1 == "null" {
                std::mem::swap(&mut t1, &mut t2);
            }

            if t2 != "null" {
                return Err(A::Error::invalid_value(
                    Unexpected::Str(t2),
                    &"exactly one 'null' in a 2-element array",
                ));
            }

            let inner = match t1 {
                "string" => ParamType::String,
                "number" => ParamType::Number,
                "boolean" => ParamType::Boolean,
                "array" => ParamType::Array,
                other => {
                    // If it's neither recognized nor "null," it's unknown
                    return Err(A::Error::unknown_variant(
                        other,
                        &["string", "number", "boolean", "array", "null"],
                    ));
                }
            };

            Ok(ParamType::Nullable(Box::new(inner)))
        }
    }
}
//

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
