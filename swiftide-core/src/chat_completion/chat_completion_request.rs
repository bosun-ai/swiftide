use std::collections::HashSet;

use derive_builder::Builder;

use super::{chat_message::ChatMessage, tools::ToolSpec};

/// A chat completion request represents a series of chat messages and tool interactions that can
/// be send to any LLM.
///
/// LLM providers are expected to use `messages()` to get the current messages for completion.
/// If the completion request includes a `ChatMessage::Summary`, previous messages that are not
/// `ChatMessage::System` are ignored.
#[derive(Builder, Clone, PartialEq, Debug)]
#[builder(setter(into, strip_option))]
pub struct ChatCompletionRequest {
    messages: Vec<ChatMessage>,
    #[builder(default)]
    tools_spec: HashSet<ToolSpec>,
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
