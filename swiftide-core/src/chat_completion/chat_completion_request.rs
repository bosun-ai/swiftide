use std::collections::HashSet;

use derive_builder::Builder;

use super::{chat_message::ChatMessage, tools::ToolSpec};

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
        &self.messages
    }

    pub fn tools_spec(&self) -> &HashSet<ToolSpec> {
        &self.tools_spec
    }
}
