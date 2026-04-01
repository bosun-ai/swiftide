use std::{borrow::Cow, collections::BTreeSet};

use derive_builder::Builder;

use super::{chat_message::ChatMessage, tools::ToolSpec, traits::Tool};

/// A chat completion request represents a series of chat messages and tool interactions that can
/// be send to any LLM.
#[derive(Builder, Clone, PartialEq, Debug)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest<'a> {
    pub messages: Cow<'a, [ChatMessage]>,
    #[builder(default, setter(custom))]
    pub tools_spec: BTreeSet<ToolSpec>,
}

impl<'a> ChatCompletionRequest<'a> {
    pub fn builder() -> ChatCompletionRequestBuilder<'a> {
        ChatCompletionRequestBuilder::default()
    }

    /// Returns the chat messages included in the request.
    pub fn messages(&self) -> &[ChatMessage] {
        self.messages.as_ref()
    }

    /// Returns the tool specifications currently attached to the request.
    pub fn tools_spec(&self) -> &BTreeSet<ToolSpec> {
        &self.tools_spec
    }

    /// Returns an owned request with `'static` data.
    pub fn to_owned(&self) -> ChatCompletionRequest<'static> {
        ChatCompletionRequest {
            messages: Cow::Owned(self.messages.iter().map(ChatMessage::to_owned).collect()),
            tools_spec: self.tools_spec.clone(),
        }
    }
}

impl From<Vec<ChatMessage>> for ChatCompletionRequest<'_> {
    fn from(messages: Vec<ChatMessage>) -> Self {
        ChatCompletionRequest {
            messages: Cow::Owned(messages),
            tools_spec: BTreeSet::new(),
        }
    }
}

impl<'a> From<&'a [ChatMessage]> for ChatCompletionRequest<'a> {
    fn from(messages: &'a [ChatMessage]) -> Self {
        ChatCompletionRequest {
            messages: Cow::Borrowed(messages),
            tools_spec: BTreeSet::new(),
        }
    }
}

impl ChatCompletionRequestBuilder<'_> {
    #[deprecated(note = "Use `tools` with real Tool instances instead")]
    pub fn tools_spec<I>(&mut self, tools_spec: I) -> &mut Self
    where
        I: IntoIterator<Item = ToolSpec>,
    {
        self.tools_spec = Some(tools_spec.into_iter().collect());
        self
    }

    /// Adds multiple tools by deriving their specs from the provided instances.
    pub fn tools<I, T>(&mut self, tools: I) -> &mut Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Box<dyn Tool>>,
    {
        let specs = tools.into_iter().map(|tool| {
            let boxed: Box<dyn Tool> = tool.into();
            boxed.tool_spec()
        });
        self.tool_specs(specs)
    }

    /// Adds a single tool instance to the request by deriving its spec.
    pub fn tool<T>(&mut self, tool: T) -> &mut Self
    where
        T: Into<Box<dyn Tool>>,
    {
        let boxed: Box<dyn Tool> = tool.into();
        self.tool_specs(std::iter::once(boxed.tool_spec()))
    }

    /// Extends the request with additional tool specifications.
    pub fn tool_specs<I>(&mut self, specs: I) -> &mut Self
    where
        I: IntoIterator<Item = ToolSpec>,
    {
        let entry = self.tools_spec.get_or_insert_with(BTreeSet::new);
        entry.extend(specs);
        self
    }

    /// Adds a single chat message to the request
    pub fn message(&mut self, message: impl Into<ChatMessage>) -> &mut Self {
        let mut messages = self
            .messages
            .take()
            .map(Cow::into_owned)
            .unwrap_or_default();
        messages.push(message.into());

        self.messages = Some(Cow::Owned(messages));
        self
    }

    /// Extends the request with multiple chat messages.
    pub fn messages_iter<I>(&mut self, messages: I) -> &mut Self
    where
        I: IntoIterator<Item = ChatMessage>,
    {
        let mut new_messages = self
            .messages
            .take()
            .map(Cow::into_owned)
            .unwrap_or_default();
        new_messages.extend(messages);
        self.messages = Some(Cow::Owned(new_messages));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::ChatCompletionRequest;
    use crate::chat_completion::{ChatMessage, ToolSpec};
    use schemars::Schema;
    use serde_json::json;

    #[test]
    fn tool_specs_are_stored_in_deterministic_order() {
        let zebra = ToolSpec::builder()
            .name("zebra")
            .description("later alphabetically")
            .parameters_schema(schema_from_json(json!({
                "type": "object",
                "properties": {
                    "b": { "type": "string" },
                    "a": { "type": "string" }
                }
            })))
            .build()
            .unwrap();

        let alpha = ToolSpec::builder()
            .name("alpha")
            .description("earlier alphabetically")
            .parameters_schema(schema_from_json(json!({
                "properties": {
                    "z": { "type": "string" },
                    "m": { "type": "string" }
                },
                "type": "object"
            })))
            .build()
            .unwrap();

        let request = ChatCompletionRequest::builder()
            .messages(vec![ChatMessage::User("hi".into())])
            .tool_specs([zebra, alpha])
            .build()
            .unwrap();

        let names = request
            .tools_spec()
            .iter()
            .map(|spec| spec.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["alpha", "zebra"]);
    }

    fn schema_from_json(value: serde_json::Value) -> Schema {
        serde_json::from_value(value).expect("valid schema")
    }
}
