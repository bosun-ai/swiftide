use std::collections::HashSet;

use derive_builder::Builder;

use super::{chat_message::ChatMessage, tools::ToolSpec};

/// A chat completion request represents a series of chat messages and tool interactions that can
/// be send to any LLM.
#[derive(Builder, Clone, PartialEq, Debug)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest {
    pub messages: Vec<ChatMessage>,
    #[builder(default)]
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
