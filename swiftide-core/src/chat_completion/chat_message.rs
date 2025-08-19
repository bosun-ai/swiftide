use serde::{Deserialize, Serialize};

use super::tools::{ToolCall, ToolOutput};

#[derive(Clone, strum_macros::EnumIs, PartialEq, Debug, Serialize, Deserialize)]
pub enum ChatMessage {
    System(String),
    User(String),
    Assistant(Option<String>, Option<Vec<ToolCall>>),
    ToolOutput(ToolCall, ToolOutput),

    // A summary of the chat. If encountered all previous messages are ignored, except the system
    // prompt
    Summary(String),
}

impl std::fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMessage::System(s) => write!(f, "System: \"{s}\""),
            ChatMessage::User(s) => write!(f, "User: \"{s}\""),
            ChatMessage::Assistant(message, tool_calls) => write!(
                f,
                "Assistant: \"{}\", tools: {}",
                message.as_deref().unwrap_or("None"),
                tool_calls.as_deref().map_or("None".to_string(), |tc| {
                    tc.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
            ),
            ChatMessage::ToolOutput(tc, to) => write!(f, "ToolOutput: \"{tc}\": \"{to}\""),
            ChatMessage::Summary(s) => write!(f, "Summary: \"{s}\""),
        }
    }
}

impl ChatMessage {
    pub fn new_system(message: impl Into<String>) -> Self {
        ChatMessage::System(message.into())
    }

    pub fn new_user(message: impl Into<String>) -> Self {
        ChatMessage::User(message.into())
    }

    pub fn new_assistant(
        message: Option<impl Into<String>>,
        tool_calls: Option<Vec<ToolCall>>,
    ) -> Self {
        ChatMessage::Assistant(message.map(Into::into), tool_calls)
    }

    pub fn new_tool_output(tool_call: impl Into<ToolCall>, output: impl Into<ToolOutput>) -> Self {
        ChatMessage::ToolOutput(tool_call.into(), output.into())
    }

    pub fn new_summary(message: impl Into<String>) -> Self {
        ChatMessage::Summary(message.into())
    }
}

/// Returns the content of the message as a string slice.
///
/// Note that this omits the tool calls from the assistant message.
///
/// If used for estimating tokens, consider this a very rought estimate
impl AsRef<str> for ChatMessage {
    fn as_ref(&self) -> &str {
        match self {
            ChatMessage::System(s) | ChatMessage::User(s) | ChatMessage::Summary(s) => s,
            ChatMessage::Assistant(message, _) => message.as_deref().unwrap_or(""),
            ChatMessage::ToolOutput(_, output) => output.content().unwrap_or(""),
        }
    }
}
