use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use derive_builder::Builder;
use dyn_clone::DynClone;
use serde::ser::SerializeMap as _;

#[async_trait]
pub trait ChatCompletion: Send + Sync + DynClone {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse>;
}

#[async_trait]
impl ChatCompletion for Box<dyn ChatCompletion> {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        (**self).complete(request).await
    }
}

#[async_trait]
impl ChatCompletion for &dyn ChatCompletion {
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        (**self).complete(request).await
    }
}

#[async_trait]
impl<T> ChatCompletion for &T
where
    T: ChatCompletion + Clone + 'static,
{
    async fn complete(&self, request: &ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        (**self).complete(request).await
    }
}

impl<LLM> From<&LLM> for Box<dyn ChatCompletion>
where
    LLM: ChatCompletion + Clone + 'static,
{
    fn from(llm: &LLM) -> Self {
        Box::new(llm.clone())
    }
}

dyn_clone::clone_trait_object!(ChatCompletion);

#[derive(Clone, Builder, Debug)]
#[builder(setter(strip_option, into), build_fn(error = anyhow::Error))]
pub struct ChatCompletionResponse {
    pub message: Option<String>,

    // Can be a better type
    // Perhaps should be typed to actual functions already?
    #[builder(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatCompletionResponse {
    pub fn builder() -> ChatCompletionResponseBuilder {
        ChatCompletionResponseBuilder::default()
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn tool_calls(&self) -> Option<&[ToolCall]> {
        self.tool_calls.as_deref()
    }
}

impl ChatCompletionResponseBuilder {
    pub fn maybe_message<T: Into<Option<String>>>(&mut self, message: T) -> &mut Self {
        self.message = Some(message.into());
        self
    }

    pub fn maybe_tool_calls<T: Into<Option<Vec<ToolCall>>>>(&mut self, tool_calls: T) -> &mut Self {
        self.tool_calls = Some(tool_calls.into());
        self
    }
}

#[derive(Builder, Clone, PartialEq, Debug)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest {
    // TODO: Alternatively maybe, we can also have an instruction, and build a system prompt for it
    // and add it to message if present
    messages: Vec<ChatMessage>,
    #[builder(default)]
    tools_spec: HashSet<ToolSpec>,
}

impl ChatCompletionRequest {
    pub fn builder() -> ChatCompletionRequestBuilder {
        ChatCompletionRequestBuilder::default()
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
}

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant(String),
    ToolCall(ToolCall),
    ToolOutput(ToolCall, ToolOutput),
}

impl std::fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessage::System(s) => write!(f, "System: \"{s}\""),
            ChatMessage::User(s) => write!(f, "User: \"{s}\""),
            ChatMessage::Assistant(s) => write!(f, "Assistant: \"{s}\""),
            ChatMessage::ToolCall(tc) => write!(f, "ToolCall: \"{tc}\""),
            ChatMessage::ToolOutput(tc, to) => write!(f, "ToolOutput: \"{tc}\": \"{to}\""),
        }
    }
}

pub enum ChatRole {
    System,
    User,
    Assistant,
}

// TODO: Naming
#[derive(Debug, Clone, PartialEq, strum_macros::Display)]
#[non_exhaustive]
pub enum ToolOutput {
    /// Adds the result of the toolcall to messages
    Text(String),
    Ok,
    /// Stops an agent
    ///
    Stop,
}

impl ToolOutput {
    pub fn content(&self) -> Option<&str> {
        match self {
            ToolOutput::Text(s) => Some(s),
            _ => None,
        }
    }
}

impl<T: AsRef<str>> From<T> for ToolOutput {
    fn from(s: T) -> Self {
        ToolOutput::Text(s.as_ref().to_string())
    }
}

/// TODO: Needs more values, i.e. `OpenAI` needs a reference to the original call
#[derive(Clone, Debug, Builder, PartialEq)]
#[builder(setter(into, strip_option))]
pub struct ToolCall {
    id: String,
    name: String,
    #[builder(default)]
    args: Option<String>,
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

// Example jsonspec for a tool
// {
//     "type": "function",
//     "function": {
//         "name": "get_delivery_date",
//         "description": "Get the delivery date for a customer's order. Call this whenever you need to know the delivery date, for example when a customer asks 'Where is my package'",
//         "parameters": {
//             "type": "object",
//             "properties": {
//                 "order_id": {
//                     "type": "string",
//                     "description": "The customer's order ID.",
//                 },
//             },
//             "required": ["order_id"],
//             "additionalProperties": False,
//         },
//     }
// }

#[derive(Clone, Debug, Hash, Eq, PartialEq, Default, Builder)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,

    pub parameters: Vec<ParamSpec>,
}

impl ToolSpec {
    pub fn builder() -> ToolSpecBuilder {
        ToolSpecBuilder::default()
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Builder)]
pub struct ParamSpec {
    pub name: &'static str,
    pub description: &'static str,
}

impl ParamSpec {
    pub fn builder() -> ParamSpecBuilder {
        ParamSpecBuilder::default()
    }
}

/*
Returns a serialized json spec

i.e. given a `PrameterSpec { name: "order_id", description: "The customer's order ID." }`

```json
{
"order_id": {
     "type": "string",
     "description": "The customer's order ID."
}
}
```

*/
impl serde::Serialize for ParamSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Use a hashmap to serialize
        let mut map = serializer.serialize_map(Some(1))?;
        let child_values = std::collections::HashMap::<&str, &str>::from_iter(vec![
            ("type", "string"),
            ("description", self.description),
        ]);

        map.serialize_entry(self.name, &child_values)?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parameter_spec_serialize() {
        let spec = ParamSpec {
            name: "order_id",
            description: "The customer's order ID.",
        };
        let serialized = serde_json::to_string(&spec).unwrap();
        let expected = r#"{"order_id":{"type":"string","description":"The customer's order ID."}}"#;
        assert_eq!(serialized, expected);
    }
}
