use std::collections::HashSet;

use derive_builder::Builder;

use super::{chat_message::ChatMessage, tools::ToolSpec, traits::Tool};

/// A chat completion request represents a series of chat messages and tool interactions that can
/// be send to any LLM.
#[derive(Builder, Clone, PartialEq, Debug)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest {
    pub messages: Vec<ChatMessage>,
    #[builder(default, setter(custom))]
    pub tools_spec: HashSet<ToolSpec>,
}

impl ChatCompletionRequest {
    pub fn builder() -> ChatCompletionRequestBuilder {
        ChatCompletionRequestBuilder::default()
    }

    /// Returns the chat messages included in the request.
    pub fn messages(&self) -> &[ChatMessage] {
        self.messages.as_slice()
    }

    /// Returns the tool specifications currently attached to the request.
    pub fn tools_spec(&self) -> &HashSet<ToolSpec> {
        &self.tools_spec
    }
}

impl From<Vec<ChatMessage>> for ChatCompletionRequest {
    fn from(messages: Vec<ChatMessage>) -> Self {
        ChatCompletionRequest {
            messages,
            tools_spec: HashSet::new(),
        }
    }
}

impl ChatCompletionRequestBuilder {
    #[deprecated(note = "Use `tools` with real Tool instances instead")]
    pub fn tools_spec(&mut self, tools_spec: HashSet<ToolSpec>) -> &mut Self {
        self.tools_spec = Some(tools_spec);
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
        let entry = self.tools_spec.get_or_insert_with(HashSet::new);
        entry.extend(specs);
        self
    }

    /// Appends a single chat message to the request.
    pub fn message(&mut self, message: impl Into<ChatMessage>) -> &mut Self {
        self.messages
            .get_or_insert_with(Vec::new)
            .push(message.into());
        self
    }

    /// Extends the request with multiple chat messages.
    pub fn messages_iter<I>(&mut self, messages: I) -> &mut Self
    where
        I: IntoIterator<Item = ChatMessage>,
    {
        let entry = self.messages.get_or_insert_with(Vec::new);
        entry.extend(messages);
        self
    }
}
