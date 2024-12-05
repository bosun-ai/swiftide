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

    pub fn messages(&self) -> Vec<&ChatMessage> {
        let mut summary_found = false;
        let mut messages = self
            .messages
            .iter()
            .rev()
            .filter(|m| {
                if summary_found {
                    return matches!(m, ChatMessage::System(_));
                }
                if let ChatMessage::Summary(_) = m {
                    summary_found = true;
                }
                true
            })
            .collect::<Vec<_>>();

        messages.reverse();

        messages
    }

    pub fn tools_spec(&self) -> &HashSet<ToolSpec> {
        &self.tools_spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_completion::chat_message::ChatMessage;
    use std::collections::HashSet;

    #[test]
    fn test_chat_completion_request_with_summary() {
        let messages = vec![
            ChatMessage::System("System message".into()),
            ChatMessage::User("Hello".into()),
            ChatMessage::Assistant(Some("Hello there".into()), None),
            ChatMessage::Summary("Summary message".into()),
            ChatMessage::User("This should be ignored".into()),
        ];

        let tools_spec = HashSet::new();

        let request = ChatCompletionRequest::builder()
            .messages(messages)
            .tools_spec(tools_spec)
            .build()
            .unwrap();

        let filtered_messages: Vec<&ChatMessage> = request.messages();

        assert_eq!(filtered_messages.len(), 3);
        assert!(matches!(filtered_messages[0], ChatMessage::System(_)));
        assert!(matches!(filtered_messages[1], ChatMessage::Summary(_)));
        assert!(matches!(filtered_messages[2], ChatMessage::User(_)));
    }
}
