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
        let specs = tools.into_iter().map(|tool| tool.tool_spec());
        let entry = self.tools_spec.get_or_insert_with(HashSet::new);
        entry.extend(specs);
        self
    }
}
