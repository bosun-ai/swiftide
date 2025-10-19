use std::collections::HashSet;
use std::sync::Arc;

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

    pub fn messages(&self) -> &[ChatMessage] {
        self.messages.as_slice()
    }

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

    pub fn tools<I>(&mut self, tools: I) -> &mut Self
    where
        I: IntoIterator<Item = Arc<dyn Tool>>,
    {
        self.tool_specs(tools.into_iter().map(|tool| tool.tool_spec()))
    }

    pub fn tool<T>(&mut self, tool: Arc<T>) -> &mut Self
    where
        T: Tool + 'static,
    {
        self.tool_specs(std::iter::once(tool.tool_spec()))
    }

    pub fn tool_specs<I>(&mut self, specs: I) -> &mut Self
    where
        I: IntoIterator<Item = ToolSpec>,
    {
        let entry = self.tools_spec.get_or_insert_with(HashSet::new);
        entry.extend(specs);
        self
    }

    pub fn message(&mut self, message: impl Into<ChatMessage>) -> &mut Self {
        self.messages
            .get_or_insert_with(Vec::new)
            .push(message.into());
        self
    }

    pub fn messages_iter<I>(&mut self, messages: I) -> &mut Self
    where
        I: IntoIterator<Item = ChatMessage>,
    {
        let entry = self.messages.get_or_insert_with(Vec::new);
        entry.extend(messages);
        self
    }
}
