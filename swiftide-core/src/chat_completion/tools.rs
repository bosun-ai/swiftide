use std::fmt;

use derive_builder::Builder;
use serde::de::{Deserializer, Error as DeError, SeqAccess, Unexpected, Visitor};
use serde::ser::{Error as SerError, SerializeSeq, Serializer};
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

#[derive(Clone, Debug, Hash, Eq, PartialEq, Default, strum_macros::AsRefStr)]
#[strum(serialize_all = "camelCase")]
pub enum ParamType {
    #[default]
    String,
    Number,
    Boolean,
    Array,
    Nullable(Box<ParamType>),
}

pub enum InnerParamType {
    String,
    Number,
    Boolean,
    Array,
}

impl Serialize for ParamType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // Non-nullable => single string
            ParamType::String => serializer.serialize_str("string"),
            ParamType::Number => serializer.serialize_str("number"),
            ParamType::Boolean => serializer.serialize_str("boolean"),
            ParamType::Array => serializer.serialize_str("array"),

            // Nullable => an array of exactly two items, e.g. ["string", "null"]
            ParamType::Nullable(inner) => {
                // If you want to forbid nested nullables:
                if let ParamType::Nullable(_) = inner.as_ref() {
                    return Err(serde::ser::Error::custom("Nested Nullable not supported"));
                }
                // Otherwise, produce an array like `["string", "null"]`.
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element(&primitive_variant_str(inner).map_err(S::Error::custom)?)?;
                seq.serialize_element("null")?;
                seq.end()
            }
        }
    }
}

fn primitive_variant_str(pt: &ParamType) -> Result<&'static str, &'static str> {
    match pt {
        ParamType::String => Ok("string"),
        ParamType::Number => Ok("number"),
        ParamType::Boolean => Ok("boolean"),
        ParamType::Array => Ok("array"),
        ParamType::Nullable(_) => Err("Nested Nullable found"),
    }
}

impl<'de> Deserialize<'de> for ParamType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ParamTypeVisitor)
    }
}

struct ParamTypeVisitor;

impl<'de> Visitor<'de> for ParamTypeVisitor {
    type Value = ParamType;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "a string (e.g. \"string\", \"number\") \
             or a 2-element array [<type>, \"null\"]"
        )
    }

    // Single strings => simple ParamType
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

    // Arrays => expect exactly 2 items, one must be "null"
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<String>()? {
            items.push(item);
        }

        // Must have exactly 2 elements
        if items.len() != 2 {
            return Err(A::Error::invalid_length(items.len(), &"2"));
        }

        let mut first = items[0].as_str();
        let mut second = items[1].as_str();

        // If the first is "null", swap so second is "null" and first is the real type
        if first == "null" {
            std::mem::swap(&mut first, &mut second);
        }

        // Now 'second' must be "null".
        if second != "null" {
            return Err(A::Error::invalid_value(
                Unexpected::Str(second),
                &"expected exactly one 'null' in [<type>, 'null']",
            ));
        }

        // 'first' must be a known primitive
        let inner = match first {
            "string" => ParamType::String,
            "number" => ParamType::Number,
            "boolean" => ParamType::Boolean,
            "array" => ParamType::Array,
            other => {
                return Err(A::Error::unknown_variant(
                    other,
                    &["string", "number", "boolean", "array", "null"],
                ));
            }
        };

        Ok(ParamType::Nullable(Box::new(inner)))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_param_type() {
        let param = ParamType::Nullable(Box::new(ParamType::String));
        let serialized = serde_json::to_string(&param).unwrap();
        assert_eq!(serialized, r#"["string","null"]"#);

        let deserialized: ParamType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(param, deserialized);
    }

    #[test]
    fn test_deserialize_param_type() {
        let serialized = r#"["string","null"]"#;
        let deserialized: ParamType = serde_json::from_str(serialized).unwrap();
        assert_eq!(
            deserialized,
            ParamType::Nullable(Box::new(ParamType::String))
        );

        let serialized = r#""string""#;
        let deserialized: ParamType = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized, ParamType::String);
    }
}
